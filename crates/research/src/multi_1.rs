//! `MULTI-1`: deterministic multi-actor in-process harness (`PS-032`).
//!
//! Folds two or more concurrent RFC 8032 Ed25519 signed actor action streams through one
//! deterministic in-process executor (`PrimaryLaunchExecutorV1` + `ExecutionRootCompositionV1`
//! bootstrapped from `GENESIS-ROOT-PARITY-1`-verified state) using an explicit canonical total
//! ordering key:
//! ```text
//! (priority_tip DESC, principal_actor_id ASC, nonce ASC, envelope_digest ASC)
//! ```
//!
//! Arrival-order permutations of concurrent actor streams converge to **one byte-identical
//! 11-slot `ExecutionRootCompositionV1` and `MultiActorFoldReceiptV1`** across:
//! - Same-coordinate concurrent races (higher `priority_tip` or lower `actor_id` wins; competing
//!   actor atomically rolls back with `RejectedSameCoordRaceLost`),
//! - Depth-1 scoped delegations (`Actor A -> Delegate C` succeeds while expired, unscoped,
//!   native-transfer, or depth-2 transitive `A -> C -> B` delegations fail closed),
//! - Same-batch burn → re-mint sequences producing distinct `MintIdV1` episode provenance with
//!   exact `ASSET-1` conservation,
//! - Stale world-root anchors, duplicate/stale nonces, bedrock/redundant no-ops, and admitted
//!   insufficient-balance failures with atomic rollback.
//!
//! Multiple actors in one process are not networking, decentralization, Sybil resistance,
//! availability, or consensus.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::asset_d1::AssetPolicyDescriptorV1;
use crate::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, CryptoSuiteDescriptorV1,
    HashSuiteIdV1, SignatureSuiteIdV1, SigningDomainTagV1, SigningPreimageV1,
    sign_preimage_deterministic, verify_preimage_strict,
};
use crate::execution_root_1::{
    EXECUTION_ROOT_1_VERSION, ExecutionDomainRegistryV1, ExecutionRoot1Error,
    ExecutionRootCompositionV1, ExecutionRootPreimageContextV1, ExecutionRootRefV1,
    compose_from_executor_snapshot, verify_header_and_manifest_correspondence,
};
use crate::executor_1::{
    Executor1Error, ExecutorBlockBatchV1, ExecutorBlockReceiptV1, ExecutorDispositionV1,
    ExecutorDomainSnapshotV1, ExecutorForkContextV1, ExecutorGenesisConfigV1, ExecutorOpV1,
    PrimaryLaunchExecutorV1,
};
use crate::genesis_d1::{
    DEVNET_V1_IDENTITY_NAMESPACE, GenesisActorEntryV1, GenesisPackageV1, LaunchGenesisStateV1,
};
use crate::genesis_root_parity_1::{
    CanonicalDevnetGenesisFixtureV1, GenesisParityInputV1, GenesisPreimageBundleV1,
    GenesisRootParity1Error, verify_genesis_root_parity,
};
use crate::governance_1::GovernanceContextV1;
use crate::history::world_tag;
use crate::identity_1::{ActorAuthStateV1, IdentityContextV1};
use crate::identity_d1::{
    ActorIdV1, AuthorizationDescriptorV1, CredentialRoleV1, DelegationGrantV1, ThresholdPolicyV1,
};
use crate::profile_d1::{BoundProfileManifestV1, CandidateProfileManifestV1};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::transition_spec_1::ResolvedProfileTransitionSpecV1;
use crate::validator_schedule_1::ValidatorKeyPairV1;
use crate::world::{BEDROCK_COORD, Coord, World};

/// Canonical schema string for a [`MultiActorStreamBundleV1`] input carrier (`MULTI-1`).
pub const MULTI_1_STREAM_BUNDLE_SCHEMA: &[u8] = b"bitblock/multi-1/signed-stream-bundle/v1";
/// Canonical schema string for a [`MultiActorFoldReceiptV1`] deterministic fold receipt (`MULTI-1`).
pub const MULTI_1_FOLD_RECEIPT_SCHEMA: &[u8] = b"bitblock/multi-1/fold-receipt/v1";

/// Canonical version identifier for `MULTI-1` envelopes, bundles, and receipts.
pub const MULTI_1_VERSION: u16 = 1;

const TX_PAYLOAD_PREFIX: &[u8] = b"bb.multi1.tx_payload.v1";
const TX_ENVELOPE_PREFIX: &[u8] = b"bb.multi1.tx_env.v1";
const INPUT_SET_PREFIX: &[u8] = b"bb.multi1.input_set.v1";
const OUTCOMES_PREFIX: &[u8] = b"bb.multi1.outcomes.v1";
const MINT_ID_PREFIX: &[u8] = b"bb.multi1.mint_id.v1";

/// Fail-closed validation, codec, permutation, or folding error for `MULTI-1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Multi1Error {
    /// Wire buffer had invalid version, length, discriminant, or trailing bytes.
    InvalidWire(&'static str),
    /// Bundle context or height disagreed with harness state.
    ContextOrHeightMismatch(&'static str),
    /// Two arrival-order permutations produced different canonical fold outputs.
    PermutationDivergenceDetected(&'static str),
    /// Underlying `GENESIS-ROOT-PARITY-1` bootstrap failed.
    GenesisParity(GenesisRootParity1Error),
    /// Underlying `EXECUTOR-1` execution or rebuild parity failed.
    Executor(Executor1Error),
    /// Underlying `EXECUTION-ROOT-1` composition failed.
    ExecutionRoot(ExecutionRoot1Error),
    /// Fixture or invariant error.
    Invariant(String),
}

impl From<GenesisRootParity1Error> for Multi1Error {
    fn from(value: GenesisRootParity1Error) -> Self {
        Self::GenesisParity(value)
    }
}

impl From<Executor1Error> for Multi1Error {
    fn from(value: Executor1Error) -> Self {
        Self::Executor(value)
    }
}

impl From<ExecutionRoot1Error> for Multi1Error {
    fn from(value: ExecutionRoot1Error) -> Self {
        Self::ExecutionRoot(value)
    }
}

impl fmt::Display for Multi1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Multi1Error {}

/// Multi-actor signed operation kind folded by [`MultiActorHarnessV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum MultiActorOpKindV1 {
    /// Place one non-bedrock voxel at `coord` (requires `allow_on` when delegated).
    PlaceVoxel = 1,
    /// Delete one occupied non-bedrock voxel at `coord` (requires `allow_off` when delegated).
    DeleteVoxel = 2,
    /// Place voxel & lock refundable deposit for a new occupancy episode (`MintIdV1`).
    MintEpisode = 3,
    /// Refund locked deposit & delete occupied voxel for an owned occupancy episode.
    BurnEpisode = 4,
    /// Fungible native transfer from `principal_actor_id` to `target_actor_id` (principal only).
    TransferNative = 5,
}

impl MultiActorOpKindV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte operation-kind discriminant.
    pub const fn from_code(code: u8) -> Result<Self, Multi1Error> {
        match code {
            1 => Ok(Self::PlaceVoxel),
            2 => Ok(Self::DeleteVoxel),
            3 => Ok(Self::MintEpisode),
            4 => Ok(Self::BurnEpisode),
            5 => Ok(Self::TransferNative),
            _ => Err(Multi1Error::InvalidWire("unknown MultiActorOpKindV1")),
        }
    }

    /// Deterministic gas cost associated with this operation class.
    pub const fn nominal_gas(self) -> u64 {
        match self {
            Self::PlaceVoxel | Self::DeleteVoxel => 2_384,
            Self::MintEpisode => 28_792,
            Self::BurnEpisode => 28_280,
            Self::TransferNative => 26_280,
        }
    }
}

/// Deterministic disposition emitted for every envelope folded by [`MultiActorHarnessV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum MultiActorDispositionV1 {
    /// Authorized and committed cleanly through `PrimaryLaunchExecutorV1` + `EXECUTION-ROOT-1`.
    AdmittedExecuted = 0,
    /// RFC 8032 signature, unknown actor, revoked actor, or chain context check failed.
    RejectedInvalidSignatureOrContext = 1,
    /// Missing, expired, unscoped, transfer-forbidden, or depth-2 transitive delegation rejected.
    RejectedUnauthorizedOrExpiredDelegation = 2,
    /// Submitted `expected_world_root` disagreed with batch pre-state `world_root` (stale anchor).
    RejectedStaleWorldAnchor = 3,
    /// Nonce duplicated an already-used nonce or skipped ahead of sequential `next_nonce`.
    RejectedDuplicateOrStaleNonce = 4,
    /// Lost a canonical same-coordinate race or attempted to modify another actor's occupied voxel.
    RejectedSameCoordRaceLost = 5,
    /// Attempted bedrock mutation, redundant occupancy edit, missing burn target, or zero/self transfer.
    RejectedNoOpBedrockOrRedundant = 6,
    /// Passed pre-admission authorization but failed balance/conservation with atomic rollback.
    AdmittedFailureAtomicRollback = 7,
}

impl MultiActorDispositionV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte disposition discriminant.
    pub const fn from_code(code: u8) -> Result<Self, Multi1Error> {
        match code {
            0 => Ok(Self::AdmittedExecuted),
            1 => Ok(Self::RejectedInvalidSignatureOrContext),
            2 => Ok(Self::RejectedUnauthorizedOrExpiredDelegation),
            3 => Ok(Self::RejectedStaleWorldAnchor),
            4 => Ok(Self::RejectedDuplicateOrStaleNonce),
            5 => Ok(Self::RejectedSameCoordRaceLost),
            6 => Ok(Self::RejectedNoOpBedrockOrRedundant),
            7 => Ok(Self::AdmittedFailureAtomicRollback),
            _ => Err(Multi1Error::InvalidWire("unknown MultiActorDispositionV1")),
        }
    }
}

/// Canonical fixed-width signed multi-actor transaction envelope (`299` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SignedActorTxEnvelopeV1 {
    /// Envelope schema version (`1`).
    pub envelope_version: u16,
    /// Target chain identifier bound into signing preimage.
    pub chain_id: u64,
    /// Principal actor whose state/nonce/balance is acted upon.
    pub principal_actor_id: ActorIdV1,
    /// Signer actor (`== principal_actor_id` for direct, or depth-1 delegate actor ID).
    pub signer_actor_id: ActorIdV1,
    /// Sequential principal nonce.
    pub nonce: u64,
    /// Priority tip used as primary descending key in canonical deterministic total order.
    pub priority_tip: u128,
    /// Expected pre-batch `WorldRootV1::root` anchor (stale-anchor protection).
    pub expected_world_root: [u8; 32],
    /// Signed operation kind.
    pub op_kind: MultiActorOpKindV1,
    /// Target 3D coordinate (`BEDROCK_COORD` for `TransferNative`).
    pub coord: Coord,
    /// Recipient actor ID for `TransferNative` (`[0u8; 32]` otherwise).
    pub target_actor_id: ActorIdV1,
    /// Transfer amount for `TransferNative` (`0` otherwise).
    pub amount: u128,
    /// Canonical 32-byte Ed25519 public key of `signer_actor_id`.
    pub signer_pubkey: [u8; 32],
    /// Strict RFC 8032 64-byte Ed25519 signature over the domain-separated signing preimage.
    pub signature: [u8; 64],
}

impl SignedActorTxEnvelopeV1 {
    /// Fixed canonical big-endian wire length (`299` bytes).
    pub const WIRE_LEN: usize = 2 + 8 + 32 + 32 + 8 + 16 + 32 + 1 + 24 + 32 + 16 + 32 + 64;

    /// Computes the signature-free SHA-256 payload digest bound inside [`SigningPreimageV1`].
    pub fn unsigned_payload_digest(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(TX_PAYLOAD_PREFIX);
        h.update(self.envelope_version.to_be_bytes());
        h.update(self.chain_id.to_be_bytes());
        h.update(self.principal_actor_id.as_bytes());
        h.update(self.signer_actor_id.as_bytes());
        h.update(self.nonce.to_be_bytes());
        h.update(self.priority_tip.to_be_bytes());
        h.update(self.expected_world_root);
        h.push_op_fields(self.op_kind, self.coord, self.target_actor_id, self.amount);
        h.finalize().into()
    }

    /// Builds the exact non-circular [`SigningPreimageV1`] for this envelope under `fork`.
    pub fn signing_preimage(&self, fork: &ExecutorForkContextV1) -> SigningPreimageV1 {
        SigningPreimageV1 {
            domain_tag: SigningDomainTagV1::ActorTransaction,
            payload_digest: self.unsigned_payload_digest(),
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            protocol_version: 1,
            chain_id: self.chain_id,
            genesis_id: fork.genesis_id,
            launch_manifest_id: fork.genesis_id,
            profile_id: fork.profile_id,
        }
    }

    /// Signs this envelope deterministically using `signer_seed` under RFC 8032.
    pub fn sign_with_seed(
        mut self,
        signer_seed: &[u8; 32],
        fork: &ExecutorForkContextV1,
    ) -> Result<Self, Multi1Error> {
        let pk = CanonicalEd25519PublicKeyV1::from_secret_seed(signer_seed)
            .map_err(|e| Multi1Error::Invariant(e.to_string()))?;
        let pre = self.signing_preimage(fork);
        let sig = sign_preimage_deterministic(signer_seed, &pre)
            .map_err(|e| Multi1Error::Invariant(e.to_string()))?;
        self.signer_pubkey = *pk.as_bytes();
        self.signature = *sig.as_bytes();
        Ok(self)
    }

    /// Encodes this envelope into 299 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Multi1Error> {
        if self.envelope_version != MULTI_1_VERSION {
            return Err(Multi1Error::InvalidWire("bad envelope_version"));
        }
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.envelope_version.to_be_bytes());
        out.extend_from_slice(&self.chain_id.to_be_bytes());
        out.extend_from_slice(self.principal_actor_id.as_bytes());
        out.extend_from_slice(self.signer_actor_id.as_bytes());
        out.extend_from_slice(&self.nonce.to_be_bytes());
        out.extend_from_slice(&self.priority_tip.to_be_bytes());
        out.extend_from_slice(&self.expected_world_root);
        out.push(self.op_kind.code());
        out.extend_from_slice(&self.coord.x.to_be_bytes());
        out.extend_from_slice(&self.coord.y.to_be_bytes());
        out.extend_from_slice(&self.coord.z.to_be_bytes());
        out.extend_from_slice(self.target_actor_id.as_bytes());
        out.extend_from_slice(&self.amount.to_be_bytes());
        out.extend_from_slice(&self.signer_pubkey);
        out.extend_from_slice(&self.signature);
        Ok(out.try_into().expect("299-byte SignedActorTxEnvelopeV1"))
    }

    /// Strictly decodes 299 canonical big-endian wire bytes into a [`SignedActorTxEnvelopeV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Multi1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Multi1Error::InvalidWire(
                "wrong SignedActorTxEnvelopeV1 len",
            ));
        }
        let mut cur = 0usize;
        let envelope_version = read_u16_be(bytes, &mut cur)?;
        if envelope_version != MULTI_1_VERSION {
            return Err(Multi1Error::InvalidWire("bad envelope_version"));
        }
        let chain_id = read_u64_be(bytes, &mut cur)?;
        let principal_actor_id = ActorIdV1::from_bytes(read_arr32(bytes, &mut cur)?);
        let signer_actor_id = ActorIdV1::from_bytes(read_arr32(bytes, &mut cur)?);
        let nonce = read_u64_be(bytes, &mut cur)?;
        let priority_tip = read_u128_be(bytes, &mut cur)?;
        let expected_world_root = read_arr32(bytes, &mut cur)?;
        let op_kind = MultiActorOpKindV1::from_code(read_u8(bytes, &mut cur)?)?;
        let x = read_i64_be(bytes, &mut cur)?;
        let y = read_i64_be(bytes, &mut cur)?;
        let z = read_i64_be(bytes, &mut cur)?;
        let target_actor_id = ActorIdV1::from_bytes(read_arr32(bytes, &mut cur)?);
        let amount = read_u128_be(bytes, &mut cur)?;
        let signer_pubkey = read_arr32(bytes, &mut cur)?;
        let signature = read_arr64(bytes, &mut cur)?;
        Ok(Self {
            envelope_version,
            chain_id,
            principal_actor_id,
            signer_actor_id,
            nonce,
            priority_tip,
            expected_world_root,
            op_kind,
            coord: Coord::new(x, y, z),
            target_actor_id,
            amount,
            signer_pubkey,
            signature,
        })
    }

    /// Computes the canonical 32-byte envelope digest used as final tie-breaker & outcome ID.
    pub fn envelope_digest(&self) -> Result<[u8; 32], Multi1Error> {
        let raw = self.encode_canonical()?;
        let mut h = Sha256::new();
        h.update(TX_ENVELOPE_PREFIX);
        h.update(raw);
        Ok(h.finalize().into())
    }

    /// Returns the canonical total-ordering sort key:
    /// `(priority_tip DESC, principal_actor_id ASC, nonce ASC, envelope_digest ASC)`.
    pub fn canonical_sort_key(
        &self,
    ) -> Result<(Reverse<u128>, ActorIdV1, u64, [u8; 32]), Multi1Error> {
        Ok((
            Reverse(self.priority_tip),
            self.principal_actor_id,
            self.nonce,
            self.envelope_digest()?,
        ))
    }
}

trait HashOpExt {
    fn push_op_fields(
        &mut self,
        op_kind: MultiActorOpKindV1,
        coord: Coord,
        target: ActorIdV1,
        amount: u128,
    );
}

impl HashOpExt for Sha256 {
    fn push_op_fields(
        &mut self,
        op_kind: MultiActorOpKindV1,
        coord: Coord,
        target: ActorIdV1,
        amount: u128,
    ) {
        self.update([op_kind.code()]);
        self.update(coord.x.to_be_bytes());
        self.update(coord.y.to_be_bytes());
        self.update(coord.z.to_be_bytes());
        self.update(target.as_bytes());
        self.update(amount.to_be_bytes());
    }
}

/// Raw arrival-order multi-actor signed transaction bundle (`bitblock/multi-1/signed-stream-bundle/v1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MultiActorStreamBundleV1 {
    /// Bundle schema version (`1`).
    pub bundle_version: u16,
    /// Target execution batch height (`>= 1`).
    pub batch_height: u64,
    /// Active protocol epoch.
    pub epoch: u64,
    /// Governing fork context (`chain_id`, `world_id`, `genesis_id`, `profile_id`, `spec_digest`).
    pub fork_context: ExecutorForkContextV1,
    /// EIP-1559 base fee per gas unit.
    pub base_fee_per_gas: u64,
    /// Submitted signed envelopes in raw arrival order.
    pub envelopes: Vec<SignedActorTxEnvelopeV1>,
}

impl MultiActorStreamBundleV1 {
    /// Returns a canonically sorted copy of `self.envelopes` under
    /// `(priority_tip DESC, principal_actor_id ASC, nonce ASC, envelope_digest ASC)`.
    pub fn canonical_sorted_envelopes(&self) -> Result<Vec<SignedActorTxEnvelopeV1>, Multi1Error> {
        let mut keyed = Vec::with_capacity(self.envelopes.len());
        for env in &self.envelopes {
            keyed.push((env.canonical_sort_key()?, *env));
        }
        keyed.sort_by_key(|(k, _)| *k);
        Ok(keyed.into_iter().map(|(_, e)| e).collect())
    }

    /// Computes the arrival-order-invariant SHA-256 digest over the canonically sorted envelopes.
    pub fn canonical_input_set_digest(&self) -> Result<[u8; 32], Multi1Error> {
        let sorted = self.canonical_sorted_envelopes()?;
        let mut h = Sha256::new();
        h.update(INPUT_SET_PREFIX);
        h.update((sorted.len() as u32).to_be_bytes());
        for e in &sorted {
            h.update(e.envelope_digest()?);
        }
        Ok(h.finalize().into())
    }

    /// Encodes this stream bundle into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, Multi1Error> {
        if self.bundle_version != MULTI_1_VERSION {
            return Err(Multi1Error::InvalidWire("bad bundle_version"));
        }
        let count = u32::try_from(self.envelopes.len())
            .map_err(|_| Multi1Error::InvalidWire("envelope count overflow"))?;
        let mut out =
            Vec::with_capacity(142 + self.envelopes.len() * SignedActorTxEnvelopeV1::WIRE_LEN);
        out.extend_from_slice(&self.bundle_version.to_be_bytes());
        out.extend_from_slice(&self.batch_height.to_be_bytes());
        out.extend_from_slice(&self.epoch.to_be_bytes());
        out.extend_from_slice(&self.fork_context.encode_canonical());
        out.extend_from_slice(&self.base_fee_per_gas.to_be_bytes());
        out.extend_from_slice(&count.to_be_bytes());
        for env in &self.envelopes {
            out.extend_from_slice(&env.encode_canonical()?);
        }
        Ok(out)
    }

    /// Strictly decodes canonical big-endian wire bytes into a [`MultiActorStreamBundleV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Multi1Error> {
        const HEAD: usize = 2 + 8 + 8 + ExecutorForkContextV1::WIRE_LEN + 8 + 4;
        if bytes.len() < HEAD {
            return Err(Multi1Error::InvalidWire(
                "truncated MultiActorStreamBundleV1",
            ));
        }
        let mut cur = 0usize;
        let bundle_version = read_u16_be(bytes, &mut cur)?;
        if bundle_version != MULTI_1_VERSION {
            return Err(Multi1Error::InvalidWire("bad bundle_version"));
        }
        let batch_height = read_u64_be(bytes, &mut cur)?;
        let epoch = read_u64_be(bytes, &mut cur)?;
        let fork_slice = take_slice(bytes, &mut cur, ExecutorForkContextV1::WIRE_LEN)?;
        let fork_context = ExecutorForkContextV1::decode_canonical(fork_slice)?;
        let base_fee_per_gas = read_u64_be(bytes, &mut cur)?;
        let count = read_u32_be(bytes, &mut cur)? as usize;
        if count > 4_096 {
            return Err(Multi1Error::InvalidWire("excessive envelopes"));
        }
        let expected = HEAD + count * SignedActorTxEnvelopeV1::WIRE_LEN;
        if bytes.len() != expected {
            return Err(Multi1Error::InvalidWire("bundle length mismatch"));
        }
        let mut envelopes = Vec::with_capacity(count);
        for _ in 0..count {
            let esl = take_slice(bytes, &mut cur, SignedActorTxEnvelopeV1::WIRE_LEN)?;
            envelopes.push(SignedActorTxEnvelopeV1::decode_canonical(esl)?);
        }
        Ok(Self {
            bundle_version,
            batch_height,
            epoch,
            fork_context,
            base_fee_per_gas,
            envelopes,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` for this stream bundle.
    pub fn tag(&self) -> Result<ContentTag, Multi1Error> {
        let bytes = self.encode_canonical()?;
        hash_canonical(
            TagKind::Receipt,
            MULTI_1_STREAM_BUNDLE_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .map_err(|_| Multi1Error::InvalidWire("hash_canonical failed"))
    }
}

/// Deterministic ordered per-envelope outcome in [`MultiActorFoldReceiptV1`] (`102` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MultiActorStepOutcomeV1 {
    /// Zero-based index in canonical sorted order.
    pub canonical_index: u32,
    /// Envelope digest (`SignedActorTxEnvelopeV1::envelope_digest`).
    pub tx_digest: [u8; 32],
    /// Principal actor ID.
    pub principal_actor_id: ActorIdV1,
    /// Submitted nonce.
    pub nonce: u64,
    /// Operation kind.
    pub op_kind: MultiActorOpKindV1,
    /// Deterministic disposition.
    pub disposition: MultiActorDispositionV1,
    /// Deterministic gas charged (`0` for pre-admission rejections).
    pub gas_charged: u64,
    /// Base fee burned (`gas_charged * base_fee_per_gas`).
    pub fee_burned: u128,
}

impl MultiActorStepOutcomeV1 {
    /// Fixed canonical big-endian wire length (`102` bytes).
    pub const WIRE_LEN: usize = 4 + 32 + 32 + 8 + 1 + 1 + 8 + 16;

    /// Encodes this step outcome into 102 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.canonical_index.to_be_bytes());
        out.extend_from_slice(&self.tx_digest);
        out.extend_from_slice(self.principal_actor_id.as_bytes());
        out.extend_from_slice(&self.nonce.to_be_bytes());
        out.push(self.op_kind.code());
        out.push(self.disposition.code());
        out.extend_from_slice(&self.gas_charged.to_be_bytes());
        out.extend_from_slice(&self.fee_burned.to_be_bytes());
        out.try_into().expect("102-byte MultiActorStepOutcomeV1")
    }

    /// Strictly decodes 102 canonical big-endian wire bytes into a [`MultiActorStepOutcomeV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Multi1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Multi1Error::InvalidWire("wrong step outcome len"));
        }
        let mut cur = 0usize;
        Ok(Self {
            canonical_index: read_u32_be(bytes, &mut cur)?,
            tx_digest: read_arr32(bytes, &mut cur)?,
            principal_actor_id: ActorIdV1::from_bytes(read_arr32(bytes, &mut cur)?),
            nonce: read_u64_be(bytes, &mut cur)?,
            op_kind: MultiActorOpKindV1::from_code(read_u8(bytes, &mut cur)?)?,
            disposition: MultiActorDispositionV1::from_code(read_u8(bytes, &mut cur)?)?,
            gas_charged: read_u64_be(bytes, &mut cur)?,
            fee_burned: read_u128_be(bytes, &mut cur)?,
        })
    }
}

/// Complete arrival-order-invariant multi-actor fold receipt (`bitblock/multi-1/fold-receipt/v1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MultiActorFoldReceiptV1 {
    /// Receipt schema version (`1`).
    pub receipt_version: u16,
    /// Executed batch height.
    pub batch_height: u64,
    /// Active protocol epoch.
    pub epoch: u64,
    /// Governing fork context.
    pub fork_context: ExecutorForkContextV1,
    /// Arrival-order-invariant SHA-256 over canonically sorted input envelope digests.
    pub canonical_input_set_digest: [u8; 32],
    /// Pre-batch composed 11-slot [`ExecutionRootRefV1`].
    pub pre_execution_root_ref: ExecutionRootRefV1,
    /// Post-batch composed 11-slot [`ExecutionRootRefV1`].
    pub post_execution_root_ref: ExecutionRootRefV1,
    /// Post-batch Layer-1 `application_root`.
    pub post_application_root: [u8; 32],
    /// Post-batch complete 11-slot `root_entry_set_digest`.
    pub post_root_entry_set_digest: [u8; 32],
    /// Total admitted & executed transactions in this fold.
    pub admitted_count: u32,
    /// Total rejected / rolled-back transactions in this fold.
    pub rejected_count: u32,
    /// Total deterministic gas charged across this fold.
    pub total_gas_charged: u64,
    /// Total base fee burned across this fold.
    pub total_fee_burned: u128,
    /// SHA-256 commitment over all ordered [`MultiActorStepOutcomeV1`] records.
    pub outcomes_digest: [u8; 32],
    /// Complete ordered per-envelope outcome lane in canonical order.
    pub step_outcomes: Vec<MultiActorStepOutcomeV1>,
}

impl MultiActorFoldReceiptV1 {
    /// Encodes this fold receipt into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, Multi1Error> {
        if self.receipt_version != MULTI_1_VERSION {
            return Err(Multi1Error::InvalidWire("bad receipt_version"));
        }
        let count = u32::try_from(self.step_outcomes.len())
            .map_err(|_| Multi1Error::InvalidWire("step count overflow"))?;
        let mut out =
            Vec::with_capacity(660 + self.step_outcomes.len() * MultiActorStepOutcomeV1::WIRE_LEN);
        out.extend_from_slice(&self.receipt_version.to_be_bytes());
        out.extend_from_slice(&self.batch_height.to_be_bytes());
        out.extend_from_slice(&self.epoch.to_be_bytes());
        out.extend_from_slice(&self.fork_context.encode_canonical());
        out.extend_from_slice(&self.canonical_input_set_digest);
        out.extend_from_slice(&self.pre_execution_root_ref.encode_canonical());
        out.extend_from_slice(&self.post_execution_root_ref.encode_canonical());
        out.extend_from_slice(&self.post_application_root);
        out.extend_from_slice(&self.post_root_entry_set_digest);
        out.extend_from_slice(&self.admitted_count.to_be_bytes());
        out.extend_from_slice(&self.rejected_count.to_be_bytes());
        out.extend_from_slice(&self.total_gas_charged.to_be_bytes());
        out.extend_from_slice(&self.total_fee_burned.to_be_bytes());
        out.extend_from_slice(&self.outcomes_digest);
        out.extend_from_slice(&count.to_be_bytes());
        for s in &self.step_outcomes {
            out.extend_from_slice(&s.encode_canonical());
        }
        Ok(out)
    }

    /// Strictly decodes canonical big-endian wire bytes into a [`MultiActorFoldReceiptV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Multi1Error> {
        const HEAD: usize = 2
            + 8
            + 8
            + ExecutorForkContextV1::WIRE_LEN
            + 32
            + ExecutionRootRefV1::WIRE_LEN * 2
            + 32 * 2
            + 4
            + 4
            + 8
            + 16
            + 32
            + 4;
        if bytes.len() < HEAD {
            return Err(Multi1Error::InvalidWire(
                "truncated MultiActorFoldReceiptV1",
            ));
        }
        let mut cur = 0usize;
        let receipt_version = read_u16_be(bytes, &mut cur)?;
        if receipt_version != MULTI_1_VERSION {
            return Err(Multi1Error::InvalidWire("bad receipt_version"));
        }
        let batch_height = read_u64_be(bytes, &mut cur)?;
        let epoch = read_u64_be(bytes, &mut cur)?;
        let fork_context = ExecutorForkContextV1::decode_canonical(take_slice(
            bytes,
            &mut cur,
            ExecutorForkContextV1::WIRE_LEN,
        )?)?;
        let canonical_input_set_digest = read_arr32(bytes, &mut cur)?;
        let pre_execution_root_ref = ExecutionRootRefV1::decode_canonical(take_slice(
            bytes,
            &mut cur,
            ExecutionRootRefV1::WIRE_LEN,
        )?)?;
        let post_execution_root_ref = ExecutionRootRefV1::decode_canonical(take_slice(
            bytes,
            &mut cur,
            ExecutionRootRefV1::WIRE_LEN,
        )?)?;
        let post_application_root = read_arr32(bytes, &mut cur)?;
        let post_root_entry_set_digest = read_arr32(bytes, &mut cur)?;
        let admitted_count = read_u32_be(bytes, &mut cur)?;
        let rejected_count = read_u32_be(bytes, &mut cur)?;
        let total_gas_charged = read_u64_be(bytes, &mut cur)?;
        let total_fee_burned = read_u128_be(bytes, &mut cur)?;
        let outcomes_digest = read_arr32(bytes, &mut cur)?;
        let count = read_u32_be(bytes, &mut cur)? as usize;
        if count > 4_096 {
            return Err(Multi1Error::InvalidWire("excessive outcomes"));
        }
        if bytes.len() != HEAD + count * MultiActorStepOutcomeV1::WIRE_LEN {
            return Err(Multi1Error::InvalidWire("receipt length mismatch"));
        }
        let mut step_outcomes = Vec::with_capacity(count);
        for _ in 0..count {
            let sl = take_slice(bytes, &mut cur, MultiActorStepOutcomeV1::WIRE_LEN)?;
            step_outcomes.push(MultiActorStepOutcomeV1::decode_canonical(sl)?);
        }
        Ok(Self {
            receipt_version,
            batch_height,
            epoch,
            fork_context,
            canonical_input_set_digest,
            pre_execution_root_ref,
            post_execution_root_ref,
            post_application_root,
            post_root_entry_set_digest,
            admitted_count,
            rejected_count,
            total_gas_charged,
            total_fee_burned,
            outcomes_digest,
            step_outcomes,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` for this multi-actor fold receipt.
    pub fn tag(&self) -> Result<ContentTag, Multi1Error> {
        let bytes = self.encode_canonical()?;
        hash_canonical(
            TagKind::Receipt,
            MULTI_1_FOLD_RECEIPT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .map_err(|_| Multi1Error::InvalidWire("hash_canonical failed"))
    }
}

/// Complete output of one [`MultiActorHarnessV1::fold_stream_bundle`] execution pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiActorFoldResultV1 {
    /// Canonical [`ExecutorBlockBatchV1`] dispatched into `PrimaryLaunchExecutorV1`.
    pub executor_batch: ExecutorBlockBatchV1,
    /// Underlying [`ExecutorBlockReceiptV1`] produced by `PrimaryLaunchExecutorV1`.
    pub executor_receipt: ExecutorBlockReceiptV1,
    /// Post-batch 11-slot [`ExecutionRootCompositionV1`] verified against `BoundProfileManifestV1`.
    pub post_composition: ExecutionRootCompositionV1,
    /// Arrival-order-invariant [`MultiActorFoldReceiptV1`].
    pub fold_receipt: MultiActorFoldReceiptV1,
    /// Distinct `MintIdV1` episode identifiers minted during this batch (in canonical order).
    pub minted_episode_ids: Vec<(Coord, [u8; 32])>,
}

/// Deterministic 3-actor delegated devnet fixture (`Actor A`, `Actor B`, `Actor C`) verified
/// through `GENESIS-ROOT-PARITY-1` before multi-actor folding begins.
#[derive(Clone, Debug)]
pub struct CanonicalMultiActorFixtureV1 {
    /// Principal Actor A (`seed = 11/12`, grants depth-1 `On+Off` delegation to Actor C until height 50).
    pub actor_a: ActorIdV1,
    /// Principal Actor B (`seed = 13/14`, direct actor; receives depth-1 delegation from C for depth-2 test).
    pub actor_b: ActorIdV1,
    /// Principal Actor C (`seed = 15/16`, depth-1 delegate of Actor A).
    pub actor_c: ActorIdV1,
    /// Sealed `GENESIS-D1` package for this 3-actor devnet.
    pub package: GenesisPackageV1,
    /// Verified preimage bundle.
    pub bundle: GenesisPreimageBundleV1,
    /// Bound initial launch profile manifest.
    pub bound_manifest: BoundProfileManifestV1,
    /// Governing resolved `TRANSITION-SPEC-1`.
    pub transition_spec: ResolvedProfileTransitionSpecV1,
    /// Governing 11-slot launch execution domain registry.
    pub exec_registry: ExecutionDomainRegistryV1,
    /// Bound execution fork context.
    pub fork_context: ExecutorForkContextV1,
}

impl CanonicalMultiActorFixtureV1 {
    /// Secret action key seed for Actor A.
    pub const ACTOR_A_SEED: [u8; 32] = [11u8; 32];
    /// Secret action key seed for Actor B.
    pub const ACTOR_B_SEED: [u8; 32] = [13u8; 32];
    /// Secret action key seed for Actor C.
    pub const ACTOR_C_SEED: [u8; 32] = [15u8; 32];

    /// Builds and `GENESIS-ROOT-PARITY-1`-verifies the 3-actor devnet fixture (`chain_id = 77`).
    pub fn build() -> Result<Self, Multi1Error> {
        let base = CanonicalDevnetGenesisFixtureV1::build()?;
        let chain_id = 77u64;
        let ns = DEVNET_V1_IDENTITY_NAMESPACE.to_string();

        // Build Actor B first (no outgoing delegations), then C (delegates to B), then A (delegates to C).
        // Acyclic graph: A -> C -> B (proves depth-1 A->C valid and depth-2 A->C->B rejected!).
        let desc_b = mk_actor_desc(13, 14, Vec::new());
        let actor_b = ActorIdV1::derive(chain_id, &ns, &desc_b)
            .map_err(|e| Multi1Error::Invariant(e.to_string()))?;

        let desc_c = mk_actor_desc(
            15,
            16,
            vec![DelegationGrantV1 {
                delegate_actor_id: actor_b,
                allow_on: true,
                allow_off: true,
                expires_at_height: 50,
            }],
        );
        let actor_c = ActorIdV1::derive(chain_id, &ns, &desc_c)
            .map_err(|e| Multi1Error::Invariant(e.to_string()))?;

        let desc_a = mk_actor_desc(
            11,
            12,
            vec![DelegationGrantV1 {
                delegate_actor_id: actor_c,
                allow_on: true,
                allow_off: true,
                expires_at_height: 50,
            }],
        );
        let actor_a = ActorIdV1::derive(chain_id, &ns, &desc_a)
            .map_err(|e| Multi1Error::Invariant(e.to_string()))?;

        let mut actor_pairs = vec![(actor_a, desc_a), (actor_b, desc_b), (actor_c, desc_c)];
        actor_pairs.sort_by_key(|(aid, _)| *aid);
        let mut initial_actors = Vec::with_capacity(actor_pairs.len());
        for (aid, desc) in &actor_pairs {
            let descriptor_digest = desc
                .tag()
                .map_err(|e| Multi1Error::Invariant(e.to_string()))?
                .digest();
            initial_actors.push(GenesisActorEntryV1 {
                actor_id: *aid,
                descriptor_digest,
                initial_nonce: 0,
                initial_liquid_balance: 100_000_000_000,
            });
        }
        let bnd = &base.bundle;
        let launch_state = LaunchGenesisStateV1::build_launch_v1(
            &base.profile_scope,
            &base.commit_domain_registry,
            &bnd.resource_policy,
            &bnd.asset_policy,
            &bnd.validator_econ_policy,
            &bnd.governance_policy,
            &bnd.bounds_profile,
            &bnd.bootstrap_policy,
            chain_id,
            ns,
            initial_actors,
            base.package.state.initial_validators.clone(),
        )
        .map_err(|e| Multi1Error::Invariant(e.to_string()))?;
        let package = GenesisPackageV1::seal(launch_state)
            .map_err(|e| Multi1Error::Invariant(e.to_string()))?;

        let candidate = CandidateProfileManifestV1::canonical_launch_v1(
            package.genesis_id,
            base.profile_scope
                .tag()
                .map_err(|e| Multi1Error::Invariant(e.to_string()))?
                .digest(),
            base.commit_domain_registry.tag().digest(),
            base.bundle
                .bounds_profile
                .tag()
                .map_err(|e| Multi1Error::Invariant(e.to_string()))?
                .digest(),
            CryptoSuiteDescriptorV1::v1_ed25519_sha256()
                .tag()
                .map_err(|e| Multi1Error::Invariant(e.to_string()))?
                .digest(),
            base.bundle
                .resource_policy
                .tag()
                .map_err(|e| Multi1Error::Invariant(e.to_string()))?
                .digest(),
        )
        .map_err(|e| Multi1Error::Invariant(e.to_string()))?;
        let bound_manifest = BoundProfileManifestV1::seal(candidate)
            .map_err(|e| Multi1Error::Invariant(e.to_string()))?;
        let transition_spec = ResolvedProfileTransitionSpecV1::resolve_from_profile(
            &bound_manifest,
            &base.bundle.bounds_profile,
            &base.bundle.resource_policy,
        )
        .map_err(|e| Multi1Error::Invariant(e.to_string()))?;

        let mut bundle = base.bundle;
        bundle.actor_descriptors = actor_pairs;
        bundle.validate()?;

        // Boot strictly through GENESIS-ROOT-PARITY-1 so initial roots are preimage-verified
        let raw_pkg = package
            .encode_canonical()
            .map_err(|e| Multi1Error::Invariant(e.to_string()))?;
        let raw_bundle = bundle.encode_canonical()?;
        let _parity = verify_genesis_root_parity(&GenesisParityInputV1 {
            raw_genesis_package: &raw_pkg,
            raw_preimage_bundle: &raw_bundle,
            profile_scope: &base.profile_scope,
            commit_domain_registry: &base.commit_domain_registry,
            bound_manifest: &bound_manifest,
            transition_spec: &transition_spec,
            exec_registry: &base.exec_registry,
        })?;

        let spec_digest = transition_spec
            .tag()
            .map_err(|e| Multi1Error::Invariant(e.to_string()))?
            .digest();
        let fork_context = ExecutorForkContextV1 {
            chain_id,
            world_id: 1,
            genesis_id: package.genesis_id,
            profile_id: bound_manifest.profile_id,
            transition_spec_digest: spec_digest,
        };

        Ok(Self {
            actor_a,
            actor_b,
            actor_c,
            package,
            bundle,
            bound_manifest,
            transition_spec,
            exec_registry: base.exec_registry,
            fork_context,
        })
    }

    /// Creates a fresh [`MultiActorHarnessV1`] initialized at height `0` from this verified fixture.
    pub fn into_harness(&self) -> Result<MultiActorHarnessV1, Multi1Error> {
        MultiActorHarnessV1::from_fixture(self)
    }

    /// Builds the canonical 8-tx Wave 19 mixed workload exercising same-coord race, depth-1
    /// delegation, stale anchor, duplicate nonce, bedrock no-op, admitted overdraft failure,
    /// and a valid native transfer.
    pub fn canonical_mixed_race_bundle(
        &self,
        batch_height: u64,
        pre_world_root: [u8; 32],
    ) -> Result<MultiActorStreamBundleV1, Multi1Error> {
        let f = &self.fork_context;
        let c_race = Coord::new(3, 0, 0);
        let c_del = Coord::new(4, 0, 0);
        let c_stale = Coord::new(5, 0, 0);

        let mk = |principal: ActorIdV1,
                  signer: ActorIdV1,
                  seed: &[u8; 32],
                  nonce: u64,
                  tip: u128,
                  anchor: [u8; 32],
                  op: MultiActorOpKindV1,
                  coord: Coord,
                  target: ActorIdV1,
                  amount: u128| {
            SignedActorTxEnvelopeV1 {
                envelope_version: MULTI_1_VERSION,
                chain_id: f.chain_id,
                principal_actor_id: principal,
                signer_actor_id: signer,
                nonce,
                priority_tip: tip,
                expected_world_root: anchor,
                op_kind: op,
                coord,
                target_actor_id: target,
                amount,
                signer_pubkey: [0u8; 32],
                signature: [0u8; 64],
            }
            .sign_with_seed(seed, f)
            .unwrap()
        };

        let zero_a = ActorIdV1::from_bytes([0u8; 32]);
        let envelopes = vec![
            // Race loser (lower tip 100 vs Actor A's tip 500 on c_race)
            mk(
                self.actor_b,
                self.actor_b,
                &Self::ACTOR_B_SEED,
                0,
                100,
                pre_world_root,
                MultiActorOpKindV1::MintEpisode,
                c_race,
                zero_a,
                0,
            ),
            // Race winner (higher tip 500 on c_race)
            mk(
                self.actor_a,
                self.actor_a,
                &Self::ACTOR_A_SEED,
                0,
                500,
                pre_world_root,
                MultiActorOpKindV1::MintEpisode,
                c_race,
                zero_a,
                0,
            ),
            // Depth-1 delegated MintEpisode: Principal A, signed by Delegate C (nonce 1, tip 400)
            mk(
                self.actor_a,
                self.actor_c,
                &Self::ACTOR_C_SEED,
                1,
                400,
                pre_world_root,
                MultiActorOpKindV1::MintEpisode,
                c_del,
                zero_a,
                0,
            ),
            // Duplicate nonce (Actor A nonce 0 again, lower tip 300)
            mk(
                self.actor_a,
                self.actor_a,
                &Self::ACTOR_A_SEED,
                0,
                300,
                pre_world_root,
                MultiActorOpKindV1::PlaceVoxel,
                Coord::new(6, 0, 0),
                zero_a,
                0,
            ),
            // Stale world anchor (wrong expected_world_root)
            mk(
                self.actor_c,
                self.actor_c,
                &Self::ACTOR_C_SEED,
                0,
                250,
                [0xEE; 32],
                MultiActorOpKindV1::PlaceVoxel,
                c_stale,
                zero_a,
                0,
            ),
            // Bedrock delete no-op (Actor C nonce 0, tip 200)
            mk(
                self.actor_c,
                self.actor_c,
                &Self::ACTOR_C_SEED,
                0,
                200,
                pre_world_root,
                MultiActorOpKindV1::DeleteVoxel,
                BEDROCK_COORD,
                zero_a,
                0,
            ),
            // Admitted overdraft failure (Actor B nonce 0 -> still 0 because race lost didn't consume nonce; tip 80)
            mk(
                self.actor_b,
                self.actor_b,
                &Self::ACTOR_B_SEED,
                0,
                80,
                pre_world_root,
                MultiActorOpKindV1::TransferNative,
                BEDROCK_COORD,
                self.actor_c,
                999_999_999_999,
            ),
            // Valid native transfer (Actor C nonce 0 -> still 0 because bedrock no-op didn't consume nonce; tip 50)
            mk(
                self.actor_c,
                self.actor_c,
                &Self::ACTOR_C_SEED,
                0,
                50,
                pre_world_root,
                MultiActorOpKindV1::TransferNative,
                BEDROCK_COORD,
                self.actor_b,
                5_000_000,
            ),
        ];

        Ok(MultiActorStreamBundleV1 {
            bundle_version: MULTI_1_VERSION,
            batch_height,
            epoch: 0,
            fork_context: *f,
            base_fee_per_gas: 2,
            envelopes,
        })
    }
}

/// Deterministic in-process multi-actor folding harness wrapping `PrimaryLaunchExecutorV1`
/// and `ExecutionRootCompositionV1`.
#[derive(Clone, Debug)]
pub struct MultiActorHarnessV1 {
    fork_context: ExecutorForkContextV1,
    bound_manifest: BoundProfileManifestV1,
    exec_registry: ExecutionDomainRegistryV1,
    preimage_ctx: ExecutionRootPreimageContextV1,
    asset_policy: AssetPolicyDescriptorV1,
    executor: PrimaryLaunchExecutorV1,
    actors: BTreeMap<ActorIdV1, AuthorizationDescriptorV1>,
    nonces: BTreeMap<ActorIdV1, u64>,
    balances: BTreeMap<ActorIdV1, u128>,
    occupied_owners: BTreeMap<Coord, ActorIdV1>,
    episode_owners: BTreeMap<Coord, (ActorIdV1, [u8; 32])>,
    batch_height: u64,
}

impl MultiActorHarnessV1 {
    /// Initializes a harness from a verified [`CanonicalMultiActorFixtureV1`].
    pub fn from_fixture(fix: &CanonicalMultiActorFixtureV1) -> Result<Self, Multi1Error> {
        let profile_id = fix.bound_manifest.profile_id;
        let initial_actors: Vec<ActorAuthStateV1> = fix
            .bundle
            .actor_descriptors
            .iter()
            .map(|(aid, desc)| ActorAuthStateV1 {
                state_version: 1,
                actor_id: *aid,
                descriptor: desc.clone(),
                next_nonce: 0,
                recovery_guardians: desc.admin_policy.clone(),
                recovery_delay_heights: 100,
                pending_recovery: None,
            })
            .collect();
        let initial_balances: Vec<(ActorIdV1, u128)> = fix
            .package
            .state
            .initial_actors
            .iter()
            .map(|a| (a.actor_id, a.initial_liquid_balance))
            .collect();
        let mut initial_stakes = Vec::with_capacity(fix.package.state.initial_validators.len());
        let mut initial_validator_keys =
            Vec::with_capacity(fix.package.state.initial_validators.len());
        for v in &fix.package.state.initial_validators {
            let vid = ActorIdV1::from_bytes(v.validator_id);
            initial_stakes.push((vid, v.bonded_stake));
            let pair = ValidatorKeyPairV1 {
                vote_key: v.vote_pubkey,
                proposal_key: v.proposal_pubkey,
            };
            initial_validator_keys.push((vid, pair));
        }
        let mut forbidden_actor_keys = BTreeSet::new();
        for (_, d) in &fix.bundle.actor_descriptors {
            for &k in &d.action_policy.keys {
                forbidden_actor_keys.insert(k);
            }
            for &k in &d.admin_policy.keys {
                forbidden_actor_keys.insert(k);
            }
        }
        let cfg = ExecutorGenesisConfigV1 {
            fork_context: fix.fork_context,
            profile_scope_digest: fix.package.state.initial_profile_scope_digest,
            spec: fix.transition_spec.clone(),
            bounds: fix.bundle.bounds_profile,
            resource: fix.bundle.resource_policy,
            auth_context: IdentityContextV1 {
                protocol_version: 1,
                chain_id: fix.package.state.chain_id,
                genesis_id: fix.package.genesis_id,
                launch_manifest_id: fix.package.genesis_id,
                profile_id,
                world_tag: world_tag(&World::new()),
                signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
                hash_suite: HashSuiteIdV1::Sha256V1,
            },
            initial_actors,
            asset_policy: fix.bundle.asset_policy,
            initial_balances: initial_balances.clone(),
            gov_context: GovernanceContextV1 {
                protocol_version: 1,
                chain_id: fix.package.state.chain_id,
                genesis_id: fix.package.genesis_id,
                launch_manifest_id: fix.package.genesis_id,
                profile_id,
            },
            gov_policy: fix.bundle.governance_policy.clone(),
            initial_params: fix.bundle.initial_gov_params.clone(),
            econ_policy: fix.bundle.validator_econ_policy,
            initial_stakes,
            consensus_policy: fix.bundle.consensus_policy,
            initial_validator_keys,
            forbidden_actor_keys,
        };
        let executor = PrimaryLaunchExecutorV1::from_genesis(cfg)?;
        let actors = fix.bundle.actor_descriptors.iter().cloned().collect();
        let nonces = fix
            .bundle
            .actor_descriptors
            .iter()
            .map(|(aid, _)| (*aid, 0u64))
            .collect();
        let balances = initial_balances.into_iter().collect();
        let preimage_ctx = ExecutionRootPreimageContextV1 {
            protocol_version: EXECUTION_ROOT_1_VERSION,
            chain_id: fix.package.state.chain_id,
            world_id: fix.package.state.world_id,
            genesis_id: fix.package.genesis_id,
            profile_scope_digest: fix.package.state.initial_profile_scope_digest,
        };
        Ok(Self {
            fork_context: fix.fork_context,
            bound_manifest: fix.bound_manifest.clone(),
            exec_registry: fix.exec_registry.clone(),
            preimage_ctx,
            asset_policy: fix.bundle.asset_policy,
            executor,
            actors,
            nonces,
            balances,
            occupied_owners: BTreeMap::new(),
            episode_owners: BTreeMap::new(),
            batch_height: 0,
        })
    }

    /// Returns the current 10-domain active root snapshot.
    pub fn snapshot(&self) -> Result<ExecutorDomainSnapshotV1, Multi1Error> {
        Ok(self.executor.snapshot()?)
    }

    /// Returns the current composed 11-slot [`ExecutionRootCompositionV1`].
    pub fn current_composition(&self) -> Result<ExecutionRootCompositionV1, Multi1Error> {
        let snap = self.executor.snapshot()?;
        Ok(compose_from_executor_snapshot(
            &snap,
            &self.exec_registry,
            self.preimage_ctx,
            self.fork_context.profile_id,
        )?)
    }

    /// Returns the current sequential nonce for `actor`.
    pub fn next_nonce(&self, actor: &ActorIdV1) -> u64 {
        self.nonces.get(actor).copied().unwrap_or(0)
    }

    /// Returns the current liquid balance tracked for `actor`.
    pub fn balance_of(&self, actor: &ActorIdV1) -> u128 {
        self.balances.get(actor).copied().unwrap_or(0)
    }

    /// Returns the current live `MintIdV1` episode bound to `coord`, if any.
    pub fn episode_mint_at(&self, coord: Coord) -> Option<[u8; 32]> {
        self.episode_owners.get(&coord).map(|(_, mid)| *mid)
    }

    /// Returns the current committed batch height.
    pub const fn batch_height(&self) -> u64 {
        self.batch_height
    }

    /// Deterministically sorts `bundle.envelopes` into canonical total order, evaluates every
    /// authorization/delegation/anchor/nonce/race/conservation rule, executes admitted operations
    /// via `PrimaryLaunchExecutorV1`, composes the 11-slot execution root, and returns the
    /// arrival-order-invariant [`MultiActorFoldResultV1`].
    pub fn fold_stream_bundle(
        &mut self,
        bundle: &MultiActorStreamBundleV1,
    ) -> Result<MultiActorFoldResultV1, Multi1Error> {
        if bundle.bundle_version != MULTI_1_VERSION {
            return Err(Multi1Error::InvalidWire("bad bundle_version"));
        }
        if bundle.fork_context != self.fork_context {
            return Err(Multi1Error::ContextOrHeightMismatch(
                "bundle fork_context disagrees with harness",
            ));
        }
        if bundle.batch_height != self.batch_height + 1 {
            return Err(Multi1Error::ContextOrHeightMismatch(
                "bundle batch_height must equal current_height + 1",
            ));
        }

        let pre_snap = self.executor.snapshot()?;
        let pre_world_root = pre_snap.world_root;
        let pre_comp = compose_from_executor_snapshot(
            &pre_snap,
            &self.exec_registry,
            self.preimage_ctx,
            self.fork_context.profile_id,
        )?;
        let sorted = bundle.canonical_sorted_envelopes()?;
        let canonical_input_set_digest = bundle.canonical_input_set_digest()?;

        let mut staged_nonces = self.nonces.clone();
        let mut staged_balances = self.balances.clone();
        let mut staged_occ = self.occupied_owners.clone();
        let mut staged_ep = self.episode_owners.clone();
        let mut admitted_ops: Vec<ExecutorOpV1> = Vec::new();
        let mut minted_episode_ids: Vec<(Coord, [u8; 32])> = Vec::new();
        let mut step_outcomes: Vec<MultiActorStepOutcomeV1> = Vec::with_capacity(sorted.len());

        let mut admitted_count = 0u32;
        let mut rejected_count = 0u32;
        let mut total_gas_charged = 0u64;
        let mut total_fee_burned = 0u128;

        for (idx, env) in sorted.iter().enumerate() {
            let tx_digest = env.envelope_digest()?;
            let disp = self.authorize_and_classify(
                env,
                bundle.batch_height,
                pre_world_root,
                &staged_nonces,
                &staged_balances,
                &staged_occ,
                &staged_ep,
            );
            let (gas_charged, fee_burned) = match disp {
                MultiActorDispositionV1::AdmittedExecuted => {
                    let fee = self.asset_policy.base_fee_burn;
                    let dep = self.asset_policy.deposit_per_occupied_block;
                    match env.op_kind {
                        MultiActorOpKindV1::PlaceVoxel => {
                            admitted_ops.push(ExecutorOpV1::base_place_voxel(env.coord));
                            staged_occ.insert(env.coord, env.principal_actor_id);
                        }
                        MultiActorOpKindV1::DeleteVoxel => {
                            admitted_ops.push(ExecutorOpV1::base_delete_voxel(env.coord));
                            staged_occ.remove(&env.coord);
                        }
                        MultiActorOpKindV1::MintEpisode => {
                            let mint_id = derive_mint_episode_id(
                                bundle.batch_height,
                                idx as u32,
                                env.principal_actor_id,
                                env.coord,
                            );
                            admitted_ops.push(ExecutorOpV1::base_place_voxel(env.coord));
                            admitted_ops.push(ExecutorOpV1::asset_mint_deposit(
                                env.principal_actor_id,
                                env.coord,
                                mint_id,
                            ));
                            staged_occ.insert(env.coord, env.principal_actor_id);
                            staged_ep.insert(env.coord, (env.principal_actor_id, mint_id));
                            *staged_balances.entry(env.principal_actor_id).or_insert(0) -=
                                fee + dep;
                            minted_episode_ids.push((env.coord, mint_id));
                        }
                        MultiActorOpKindV1::BurnEpisode => {
                            admitted_ops.push(ExecutorOpV1::asset_burn_refund(
                                env.principal_actor_id,
                                env.coord,
                            ));
                            admitted_ops.push(ExecutorOpV1::base_delete_voxel(env.coord));
                            staged_occ.remove(&env.coord);
                            staged_ep.remove(&env.coord);
                            *staged_balances.entry(env.principal_actor_id).or_insert(0) +=
                                dep - fee;
                        }
                        MultiActorOpKindV1::TransferNative => {
                            admitted_ops.push(ExecutorOpV1::asset_transfer(
                                env.principal_actor_id,
                                env.target_actor_id,
                                env.amount,
                            ));
                            *staged_balances.entry(env.principal_actor_id).or_insert(0) -=
                                env.amount + fee;
                            *staged_balances.entry(env.target_actor_id).or_insert(0) += env.amount;
                        }
                    }
                    *staged_nonces.entry(env.principal_actor_id).or_insert(0) += 1;
                    admitted_count += 1;
                    let g = env.op_kind.nominal_gas();
                    let fb = u128::from(g) * u128::from(bundle.base_fee_per_gas);
                    (g, fb)
                }
                MultiActorDispositionV1::AdmittedFailureAtomicRollback => {
                    rejected_count += 1;
                    let g = env.op_kind.nominal_gas();
                    let fb = u128::from(g) * u128::from(bundle.base_fee_per_gas);
                    (g, fb)
                }
                _ => {
                    rejected_count += 1;
                    (0, 0)
                }
            };
            total_gas_charged = total_gas_charged
                .checked_add(gas_charged)
                .ok_or(Multi1Error::InvalidWire("gas sum overflow"))?;
            total_fee_burned = total_fee_burned
                .checked_add(fee_burned)
                .ok_or(Multi1Error::InvalidWire("fee sum overflow"))?;
            step_outcomes.push(MultiActorStepOutcomeV1 {
                canonical_index: idx as u32,
                tx_digest,
                principal_actor_id: env.principal_actor_id,
                nonce: env.nonce,
                op_kind: env.op_kind,
                disposition: disp,
                gas_charged,
                fee_burned,
            });
        }

        let exec_batch = ExecutorBlockBatchV1 {
            batch_version: 1,
            batch_height: bundle.batch_height,
            epoch: bundle.epoch,
            fork_context: self.fork_context,
            base_fee_per_gas: bundle.base_fee_per_gas,
            ops: admitted_ops,
        };
        let mut staged_exec = self.executor.clone();
        let exec_receipt = staged_exec.execute_batch(&exec_batch)?;
        for s in &exec_receipt.step_outcomes {
            if s.disposition != ExecutorDispositionV1::Admitted {
                return Err(Multi1Error::Invariant(format!(
                    "admitted multi-actor op failed inside PrimaryLaunchExecutorV1: {:?}",
                    s.disposition
                )));
            }
        }
        staged_exec.verify_all_domain_rebuild_parity()?;

        let post_snap = staged_exec.snapshot()?;
        let post_comp = compose_from_executor_snapshot(
            &post_snap,
            &self.exec_registry,
            self.preimage_ctx,
            self.fork_context.profile_id,
        )?;
        verify_header_and_manifest_correspondence(
            &post_comp,
            &post_comp.execution_root_ref,
            &self.exec_registry,
            &self.bound_manifest,
        )?;

        let outcomes_digest = hash_outcomes(&step_outcomes);
        let fold_receipt = MultiActorFoldReceiptV1 {
            receipt_version: MULTI_1_VERSION,
            batch_height: bundle.batch_height,
            epoch: bundle.epoch,
            fork_context: self.fork_context,
            canonical_input_set_digest,
            pre_execution_root_ref: pre_comp.execution_root_ref,
            post_execution_root_ref: post_comp.execution_root_ref,
            post_application_root: post_comp.application_root,
            post_root_entry_set_digest: post_comp.root_entry_set_digest,
            admitted_count,
            rejected_count,
            total_gas_charged,
            total_fee_burned,
            outcomes_digest,
            step_outcomes,
        };

        // Commit staged state atomically only after full verification succeeded
        self.executor = staged_exec;
        self.nonces = staged_nonces;
        self.balances = staged_balances;
        self.occupied_owners = staged_occ;
        self.episode_owners = staged_ep;
        self.batch_height = bundle.batch_height;

        Ok(MultiActorFoldResultV1 {
            executor_batch: exec_batch,
            executor_receipt: exec_receipt,
            post_composition: post_comp,
            fold_receipt,
            minted_episode_ids,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn authorize_and_classify(
        &self,
        env: &SignedActorTxEnvelopeV1,
        batch_height: u64,
        pre_world_root: [u8; 32],
        nonces: &BTreeMap<ActorIdV1, u64>,
        balances: &BTreeMap<ActorIdV1, u128>,
        occ: &BTreeMap<Coord, ActorIdV1>,
        eps: &BTreeMap<Coord, (ActorIdV1, [u8; 32])>,
    ) -> MultiActorDispositionV1 {
        if env.chain_id != self.fork_context.chain_id {
            return MultiActorDispositionV1::RejectedInvalidSignatureOrContext;
        }
        let Some(principal_desc) = self.actors.get(&env.principal_actor_id) else {
            return MultiActorDispositionV1::RejectedInvalidSignatureOrContext;
        };
        if principal_desc.revoked {
            return MultiActorDispositionV1::RejectedInvalidSignatureOrContext;
        }

        // Direct principal vs depth-1 delegated signer check
        let signer_desc = if env.signer_actor_id == env.principal_actor_id {
            principal_desc
        } else {
            if env.op_kind == MultiActorOpKindV1::TransferNative {
                return MultiActorDispositionV1::RejectedUnauthorizedOrExpiredDelegation;
            }
            let grant = principal_desc
                .delegations
                .iter()
                .find(|g| g.delegate_actor_id == env.signer_actor_id);
            let Some(g) = grant else {
                return MultiActorDispositionV1::RejectedUnauthorizedOrExpiredDelegation;
            };
            if batch_height > g.expires_at_height {
                return MultiActorDispositionV1::RejectedUnauthorizedOrExpiredDelegation;
            }
            let scope_ok = match env.op_kind {
                MultiActorOpKindV1::PlaceVoxel | MultiActorOpKindV1::MintEpisode => g.allow_on,
                MultiActorOpKindV1::DeleteVoxel | MultiActorOpKindV1::BurnEpisode => g.allow_off,
                MultiActorOpKindV1::TransferNative => false,
            };
            if !scope_ok {
                return MultiActorDispositionV1::RejectedUnauthorizedOrExpiredDelegation;
            }
            let Some(sd) = self.actors.get(&env.signer_actor_id) else {
                return MultiActorDispositionV1::RejectedUnauthorizedOrExpiredDelegation;
            };
            if sd.revoked {
                return MultiActorDispositionV1::RejectedUnauthorizedOrExpiredDelegation;
            }
            sd
        };

        // Strict RFC 8032 Ed25519 signature verification over domain-separated SigningPreimageV1
        let Ok(pk) = CanonicalEd25519PublicKeyV1::from_bytes(env.signer_pubkey) else {
            return MultiActorDispositionV1::RejectedInvalidSignatureOrContext;
        };
        if !signer_desc.action_policy.keys.contains(&pk) {
            return MultiActorDispositionV1::RejectedInvalidSignatureOrContext;
        }
        let Ok(sig) = CanonicalEd25519SignatureV1::from_bytes(env.signature) else {
            return MultiActorDispositionV1::RejectedInvalidSignatureOrContext;
        };
        if verify_preimage_strict(&pk, &env.signing_preimage(&self.fork_context), &sig).is_err() {
            return MultiActorDispositionV1::RejectedInvalidSignatureOrContext;
        }

        // Stale world-anchor check
        if env.expected_world_root != pre_world_root {
            return MultiActorDispositionV1::RejectedStaleWorldAnchor;
        }

        // Sequential principal nonce check
        let exp_nonce = nonces.get(&env.principal_actor_id).copied().unwrap_or(0);
        if env.nonce != exp_nonce {
            return MultiActorDispositionV1::RejectedDuplicateOrStaleNonce;
        }

        // Domain conflict, no-op, race, and balance conservation checks
        let fee = self.asset_policy.base_fee_burn;
        let dep = self.asset_policy.deposit_per_occupied_block;
        let bal = balances.get(&env.principal_actor_id).copied().unwrap_or(0);
        match env.op_kind {
            MultiActorOpKindV1::PlaceVoxel => {
                if env.coord == BEDROCK_COORD {
                    return MultiActorDispositionV1::RejectedNoOpBedrockOrRedundant;
                }
                match occ.get(&env.coord) {
                    Some(owner) if *owner == env.principal_actor_id => {
                        MultiActorDispositionV1::RejectedNoOpBedrockOrRedundant
                    }
                    Some(_) => MultiActorDispositionV1::RejectedSameCoordRaceLost,
                    None => MultiActorDispositionV1::AdmittedExecuted,
                }
            }
            MultiActorOpKindV1::DeleteVoxel => {
                if env.coord == BEDROCK_COORD {
                    return MultiActorDispositionV1::RejectedNoOpBedrockOrRedundant;
                }
                if eps.contains_key(&env.coord) {
                    return MultiActorDispositionV1::RejectedSameCoordRaceLost;
                }
                match occ.get(&env.coord) {
                    Some(owner) if *owner == env.principal_actor_id => {
                        MultiActorDispositionV1::AdmittedExecuted
                    }
                    Some(_) => MultiActorDispositionV1::RejectedSameCoordRaceLost,
                    None => MultiActorDispositionV1::RejectedNoOpBedrockOrRedundant,
                }
            }
            MultiActorOpKindV1::MintEpisode => {
                if env.coord == BEDROCK_COORD {
                    return MultiActorDispositionV1::RejectedNoOpBedrockOrRedundant;
                }
                match occ.get(&env.coord) {
                    Some(owner) if *owner == env.principal_actor_id => {
                        return MultiActorDispositionV1::RejectedNoOpBedrockOrRedundant;
                    }
                    Some(_) => return MultiActorDispositionV1::RejectedSameCoordRaceLost,
                    None => {}
                }
                if bal < fee.saturating_add(dep) {
                    return MultiActorDispositionV1::AdmittedFailureAtomicRollback;
                }
                MultiActorDispositionV1::AdmittedExecuted
            }
            MultiActorOpKindV1::BurnEpisode => {
                if env.coord == BEDROCK_COORD {
                    return MultiActorDispositionV1::RejectedNoOpBedrockOrRedundant;
                }
                match eps.get(&env.coord) {
                    Some((owner, _)) if *owner == env.principal_actor_id => {
                        if bal < fee {
                            MultiActorDispositionV1::AdmittedFailureAtomicRollback
                        } else {
                            MultiActorDispositionV1::AdmittedExecuted
                        }
                    }
                    Some(_) => MultiActorDispositionV1::RejectedSameCoordRaceLost,
                    None => MultiActorDispositionV1::RejectedNoOpBedrockOrRedundant,
                }
            }
            MultiActorOpKindV1::TransferNative => {
                if env.amount == 0 || env.target_actor_id == env.principal_actor_id {
                    return MultiActorDispositionV1::RejectedNoOpBedrockOrRedundant;
                }
                if bal < env.amount.saturating_add(fee) {
                    return MultiActorDispositionV1::AdmittedFailureAtomicRollback;
                }
                MultiActorDispositionV1::AdmittedExecuted
            }
        }
    }
}

/// Folds every supplied arrival-order permutation of `bundle` from identical clones of `base_harness`
/// and fails closed if any permutation diverges on `ExecutionRootCompositionV1` or `MultiActorFoldReceiptV1`.
pub fn verify_permutation_invariance(
    base_harness: &MultiActorHarnessV1,
    permutations: &[MultiActorStreamBundleV1],
) -> Result<MultiActorFoldResultV1, Multi1Error> {
    let Some(first) = permutations.first() else {
        return Err(Multi1Error::InvalidWire("empty permutation list"));
    };
    let mut reference_h = base_harness.clone();
    let reference = reference_h.fold_stream_bundle(first)?;
    for perm in &permutations[1..] {
        let mut h = base_harness.clone();
        let res = h.fold_stream_bundle(perm)?;
        if res.post_composition != reference.post_composition
            || res.fold_receipt != reference.fold_receipt
            || res.minted_episode_ids != reference.minted_episode_ids
        {
            return Err(Multi1Error::PermutationDivergenceDetected(
                "arrival-order permutation diverged on composed root or fold receipt",
            ));
        }
    }
    Ok(reference)
}

/// Derives a deterministic unique `MintIdV1` episode ID for `(batch_height, canon_idx, actor, coord)`.
pub fn derive_mint_episode_id(
    batch_height: u64,
    canonical_index: u32,
    actor: ActorIdV1,
    coord: Coord,
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(MINT_ID_PREFIX);
    h.update(batch_height.to_be_bytes());
    h.update(canonical_index.to_be_bytes());
    h.update(actor.as_bytes());
    h.update(coord.x.to_be_bytes());
    h.update(coord.y.to_be_bytes());
    h.update(coord.z.to_be_bytes());
    h.finalize().into()
}

fn hash_outcomes(outcomes: &[MultiActorStepOutcomeV1]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(OUTCOMES_PREFIX);
    h.update((outcomes.len() as u32).to_be_bytes());
    for o in outcomes {
        h.update(o.encode_canonical());
    }
    h.finalize().into()
}

fn mk_actor_desc(
    action_seed: u8,
    admin_seed: u8,
    delegations: Vec<DelegationGrantV1>,
) -> AuthorizationDescriptorV1 {
    AuthorizationDescriptorV1 {
        descriptor_version: 1,
        action_policy: ThresholdPolicyV1 {
            role: CredentialRoleV1::ActorAction,
            threshold: 1,
            keys: vec![CanonicalEd25519PublicKeyV1::from_secret_seed(&[action_seed; 32]).unwrap()],
        },
        admin_policy: ThresholdPolicyV1 {
            role: CredentialRoleV1::ActorAdmin,
            threshold: 1,
            keys: vec![CanonicalEd25519PublicKeyV1::from_secret_seed(&[admin_seed; 32]).unwrap()],
        },
        delegations,
        revoked: false,
    }
}

fn take_slice<'a>(bytes: &'a [u8], cur: &mut usize, n: usize) -> Result<&'a [u8], Multi1Error> {
    if bytes.len().saturating_sub(*cur) < n {
        return Err(Multi1Error::InvalidWire("truncated buffer"));
    }
    let sl = &bytes[*cur..*cur + n];
    *cur += n;
    Ok(sl)
}

fn read_u8(bytes: &[u8], cur: &mut usize) -> Result<u8, Multi1Error> {
    Ok(take_slice(bytes, cur, 1)?[0])
}

fn read_u16_be(bytes: &[u8], cur: &mut usize) -> Result<u16, Multi1Error> {
    Ok(u16::from_be_bytes(
        take_slice(bytes, cur, 2)?.try_into().unwrap(),
    ))
}

fn read_u32_be(bytes: &[u8], cur: &mut usize) -> Result<u32, Multi1Error> {
    Ok(u32::from_be_bytes(
        take_slice(bytes, cur, 4)?.try_into().unwrap(),
    ))
}

fn read_u64_be(bytes: &[u8], cur: &mut usize) -> Result<u64, Multi1Error> {
    Ok(u64::from_be_bytes(
        take_slice(bytes, cur, 8)?.try_into().unwrap(),
    ))
}

fn read_i64_be(bytes: &[u8], cur: &mut usize) -> Result<i64, Multi1Error> {
    Ok(i64::from_be_bytes(
        take_slice(bytes, cur, 8)?.try_into().unwrap(),
    ))
}

fn read_u128_be(bytes: &[u8], cur: &mut usize) -> Result<u128, Multi1Error> {
    Ok(u128::from_be_bytes(
        take_slice(bytes, cur, 16)?.try_into().unwrap(),
    ))
}

fn read_arr32(bytes: &[u8], cur: &mut usize) -> Result<[u8; 32], Multi1Error> {
    Ok(take_slice(bytes, cur, 32)?.try_into().unwrap())
}

fn read_arr64(bytes: &[u8], cur: &mut usize) -> Result<[u8; 64], Multi1Error> {
    Ok(take_slice(bytes, cur, 64)?.try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_1_canonical_fold_wire_roundtrip_and_permutation_invariance() {
        let fix = CanonicalMultiActorFixtureV1::build().unwrap();
        let harness = fix.into_harness().unwrap();
        let pre_wr = harness.snapshot().unwrap().world_root;
        let bundle = fix.canonical_mixed_race_bundle(1, pre_wr).unwrap();

        let raw_b = bundle.encode_canonical().unwrap();
        assert_eq!(
            MultiActorStreamBundleV1::decode_canonical(&raw_b).unwrap(),
            bundle
        );

        let mut rev_bundle = bundle.clone();
        rev_bundle.envelopes.reverse();
        let res = verify_permutation_invariance(&harness, &[bundle.clone(), rev_bundle]).unwrap();
        let raw_r = res.fold_receipt.encode_canonical().unwrap();
        assert_eq!(
            MultiActorFoldReceiptV1::decode_canonical(&raw_r).unwrap(),
            res.fold_receipt
        );
        assert_eq!(res.fold_receipt.admitted_count, 3);
        assert_eq!(res.fold_receipt.rejected_count, 5);
        assert_eq!(
            bundle.tag().unwrap().to_string(),
            "#bb.receipt.sha256.h6deuodmzgmgpkyqv2do2rkaonc7gnn7rjgi5hp3oqwkvs7l5kgq"
        );
        assert_eq!(
            res.fold_receipt.tag().unwrap().to_string(),
            "#bb.receipt.sha256.ssqa3yacg6tg6jk6d7eddccshdlikpzwxrqov6cfr5mcldjqbr3q"
        );
        assert_eq!(
            res.post_composition.tag().unwrap().to_string(),
            "#bb.receipt.sha256.lbjy45lcbkxe7qle5cwdo3nlvh3qd253kn5m433mphgkmeua5gia"
        );
    }
}
