//! `ASSET-1`: dual-domain authenticated native balances (`NativeBalanceRootV1`, Domain 3) and
//! occupancy-episode ownership (`EpisodeOwnershipRootV1`, Domain 4) over `AUTH-COMMIT-1`'s
//! reusable 256-bit domain-separated Candidate A binary Merkle Patricia Trie (`PS-038`).
//!
//! Enforces `ASSET-D1`, `IDENTITY-1`, `AUTH-COMMIT-1`, and `RESOURCE-1` invariants without
//! touching `crates/core/**` or `commit_state.rs`:
//! - **Dual-domain root separation:** `NativeBalanceRootV1` (`DomainIdV1::NativeBalance`) and
//!   `EpisodeOwnershipRootV1` (`DomainIdV1::EpisodeOwnership`) share no proof domain with each
//!   other, `AUTH-COMMIT-1`, or `COMMIT-1`.
//! - **Transfer carries the refundable deposit claim:** `TransferEpisode` moves ownership and
//!   refundable deposit authority to `to` while preserving the immutable `mint_episode_id`.
//!   A subsequent burn attempt by the previous owner fails closed (`RejectedNotEpisodeOwner`),
//!   while burn by the current owner refunds `locked_deposit` and deletes the occupancy key.
//! - **Bedrock exclusion:** origin `(0, 0, 0)` holds zero economic token and zero deposit.
//! - **Closed supply conservation & atomic rollback:** every batch checks
//!   `sum(liquid) + sum(locked_deposits) + total_burned_fees == genesis_supply`, rejects
//!   concurrent writes to the same coordinate in one batch, and rolls back both asset roots and
//!   `AuthCommitStateV1` atomically on any authorization, replay, balance, or conflict failure.
//!
//! A consistent authenticated asset ledger is not yet a cryptocurrency network or safe place for
//! production value.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::asset_d1::{AssetEconError, AssetPolicyDescriptorV1};
use crate::auth_commit_1::{
    AuthCommit1Error, AuthCommitStateV1, DomainIdV1, DomainPatriciaProofV1, DomainPatriciaTrieV1,
};
use crate::commit_r1::CoordinateKeyV1;
use crate::crypto_d1::SigningPreimageV1;
use crate::identity_1::IdentityDispositionV1;
use crate::identity_d1::{ActorIdV1, TransactionBodyV1};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::world::{BEDROCK_COORD, Coord};

/// Canonical schema string for an [`AssetStateRootsV1`] dual-domain commitment (`ASSET-1`).
pub const ASSET_1_ROOTS_SCHEMA: &[u8] = b"bitblock/asset-1/asset-state-roots/v1";
/// Canonical schema string for an [`AssetTransitionReceiptV1`] batch receipt (`ASSET-1`).
pub const ASSET_1_RECEIPT_SCHEMA: &[u8] = b"bitblock/asset-1/asset-transition-receipt/v1";

const COORD_KEY_DOMAIN_PREFIX: &[u8] = b"bb.asset1.coord_key.v1";

/// Fail-closed validation, codec, conservation, or Patricia error for `ASSET-1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Asset1Error {
    /// Underlying `ASSET-D1` policy validation error.
    Policy(AssetEconError),
    /// Underlying `AUTH-COMMIT-1` Patricia or authorization state error.
    AuthCommit(AuthCommit1Error),
    /// Canonical wire buffer had wrong length, version, discriminant, or trailing bytes.
    InvalidWire(&'static str),
    /// Arithmetic overflow or closed supply conservation violation.
    ConservationViolation(&'static str),
    /// Genesis actor list contained duplicates or violated supply ceiling.
    InvalidGenesis(&'static str),
}

impl From<AssetEconError> for Asset1Error {
    fn from(value: AssetEconError) -> Self {
        Self::Policy(value)
    }
}

impl From<AuthCommit1Error> for Asset1Error {
    fn from(value: AuthCommit1Error) -> Self {
        Self::AuthCommit(value)
    }
}

impl fmt::Display for Asset1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Asset1Error {}

/// Deterministic disposition recorded inside an [`AssetTransitionReceiptV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum AssetDispositionV1 {
    /// Entire batch succeeded and committed atomically across asset and auth/nonce roots.
    Admitted = 0,
    /// Caller failed `IDENTITY-1` signature, role, delegation, or domain-binding checks.
    RejectedUnauthorized = 1,
    /// Caller presented a stale, replayed, or future sequential nonce.
    RejectedReplayOrNonce = 2,
    /// Operation targeted immutable bedrock `(0, 0, 0)`, which holds zero economic record.
    RejectedBedrockForbidden = 3,
    /// Mint targeted an already occupied coordinate.
    RejectedCoordinateOccupied = 4,
    /// Transfer or burn targeted an unoccupied coordinate.
    RejectedCoordinateEmpty = 5,
    /// Transfer or burn caller does not own the live occupancy-episode token (incl. burn-after-transfer).
    RejectedNotEpisodeOwner = 6,
    /// Actor lacked sufficient liquid native balance for fee, deposit, or transfer amount.
    RejectedInsufficientBalance = 7,
    /// Batch contained concurrent conflicting operations targeting the same coordinate.
    RejectedConcurrentWrite = 8,
    /// Batch was empty, had zero transfer amount, or overflowed checked `u128` bounds.
    RejectedPolicyOrOverflow = 9,
}

impl AssetDispositionV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte disposition discriminant.
    pub const fn from_code(code: u8) -> Result<Self, Asset1Error> {
        match code {
            0 => Ok(Self::Admitted),
            1 => Ok(Self::RejectedUnauthorized),
            2 => Ok(Self::RejectedReplayOrNonce),
            3 => Ok(Self::RejectedBedrockForbidden),
            4 => Ok(Self::RejectedCoordinateOccupied),
            5 => Ok(Self::RejectedCoordinateEmpty),
            6 => Ok(Self::RejectedNotEpisodeOwner),
            7 => Ok(Self::RejectedInsufficientBalance),
            8 => Ok(Self::RejectedConcurrentWrite),
            9 => Ok(Self::RejectedPolicyOrOverflow),
            _ => Err(Asset1Error::InvalidWire("unknown AssetDispositionV1")),
        }
    }
}

/// Computes the 32-byte domain-separated Patricia key for a 3D world [`Coord`].
pub fn episode_trie_key(coord: Coord) -> [u8; 32] {
    let ck = CoordinateKeyV1::from_coord(coord);
    let mut h = Sha256::new();
    h.update(COORD_KEY_DOMAIN_PREFIX);
    h.update(ck.as_bytes());
    h.finalize().into()
}

/// Canonical 80-byte value committed under `DomainIdV1::EpisodeOwnership` (`ASSET-1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EpisodeOwnershipValueV1 {
    /// Current owner controlling transfer and burn authority (and refundable deposit claim).
    pub owner: ActorIdV1,
    /// Immutable mint episode identifier (preserved unchanged across all ownership transfers).
    pub mint_episode_id: [u8; 32],
    /// Refundable native storage deposit locked at this coordinate.
    pub locked_deposit: u128,
}

impl EpisodeOwnershipValueV1 {
    /// Fixed canonical wire byte length (`80` bytes).
    pub const WIRE_LEN: usize = 32 + 32 + 16;

    /// Encodes this episode ownership value into 80 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..32].copy_from_slice(self.owner.as_bytes());
        out[32..64].copy_from_slice(&self.mint_episode_id);
        out[64..80].copy_from_slice(&self.locked_deposit.to_be_bytes());
        out
    }

    /// Decodes an 80-byte canonical [`EpisodeOwnershipValueV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Asset1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Asset1Error::InvalidWire(
                "EpisodeOwnershipValueV1 must be 80 bytes",
            ));
        }
        let owner_bytes: [u8; 32] = bytes[0..32].try_into().expect("32");
        let mint_episode_id: [u8; 32] = bytes[32..64].try_into().expect("32");
        let locked_deposit = u128::from_be_bytes(bytes[64..80].try_into().expect("16"));
        if locked_deposit == 0 {
            return Err(Asset1Error::InvalidWire(
                "EpisodeOwnershipValueV1 locked_deposit must be > 0",
            ));
        }
        Ok(Self {
            owner: ActorIdV1::from_bytes(owner_bytes),
            mint_episode_id,
            locked_deposit,
        })
    }
}

/// Paired dual-domain authenticated roots and conservation totals (`ASSET-1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AssetStateRootsV1 {
    /// Schema version (`1`).
    pub roots_version: u16,
    /// Bound `PROFILE-D1` profile identifier.
    pub profile_id: [u8; 32],
    /// Committed block height.
    pub block_height: u64,
    /// Number of committed native balance entries.
    pub account_count: u32,
    /// Number of live mutable occupancy-episode ownership entries.
    pub episode_count: u32,
    /// Sum of all liquid native balances across all committed accounts.
    pub total_liquid_supply: u128,
    /// Sum of all refundable storage deposits locked across occupied coordinates.
    pub total_locked_deposits: u128,
    /// Cumulative permanently burned base execution fees.
    pub total_burned_fees: u128,
    /// Genesis native supply ceiling conserved across every transition.
    pub genesis_supply: u128,
    /// 32-byte `NativeBalanceRootV1` (`DomainIdV1::NativeBalance = 3`).
    pub native_balance_root: [u8; 32],
    /// 32-byte `EpisodeOwnershipRootV1` (`DomainIdV1::EpisodeOwnership = 4`).
    pub episode_ownership_root: [u8; 32],
}

impl AssetStateRootsV1 {
    /// Fixed canonical big-endian wire length (`178` bytes).
    pub const WIRE_LEN: usize = 2 + 32 + 8 + 4 + 4 + 16 + 16 + 16 + 16 + 32 + 32;

    /// Encodes the paired asset roots into 178 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Asset1Error> {
        if self.roots_version != 1 {
            return Err(Asset1Error::InvalidWire("roots_version must equal 1"));
        }
        let accounted = self
            .total_liquid_supply
            .checked_add(self.total_locked_deposits)
            .and_then(|v| v.checked_add(self.total_burned_fees))
            .ok_or(Asset1Error::ConservationViolation(
                "asset roots supply sum overflow",
            ))?;
        if accounted != self.genesis_supply {
            return Err(Asset1Error::ConservationViolation(
                "liquid + locked_deposits + burned_fees != genesis_supply",
            ));
        }
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.roots_version.to_be_bytes());
        out[2..34].copy_from_slice(&self.profile_id);
        out[34..42].copy_from_slice(&self.block_height.to_be_bytes());
        out[42..46].copy_from_slice(&self.account_count.to_be_bytes());
        out[46..50].copy_from_slice(&self.episode_count.to_be_bytes());
        out[50..66].copy_from_slice(&self.total_liquid_supply.to_be_bytes());
        out[66..82].copy_from_slice(&self.total_locked_deposits.to_be_bytes());
        out[82..98].copy_from_slice(&self.total_burned_fees.to_be_bytes());
        out[98..114].copy_from_slice(&self.genesis_supply.to_be_bytes());
        out[114..146].copy_from_slice(&self.native_balance_root);
        out[146..178].copy_from_slice(&self.episode_ownership_root);
        Ok(out)
    }

    /// Decodes and conservation-checks a 178-byte canonical [`AssetStateRootsV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Asset1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Asset1Error::InvalidWire(
                "AssetStateRootsV1 must be 178 bytes",
            ));
        }
        let roots = Self {
            roots_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            profile_id: bytes[2..34].try_into().expect("32"),
            block_height: u64::from_be_bytes(bytes[34..42].try_into().expect("8")),
            account_count: u32::from_be_bytes(bytes[42..46].try_into().expect("4")),
            episode_count: u32::from_be_bytes(bytes[46..50].try_into().expect("4")),
            total_liquid_supply: u128::from_be_bytes(bytes[50..66].try_into().expect("16")),
            total_locked_deposits: u128::from_be_bytes(bytes[66..82].try_into().expect("16")),
            total_burned_fees: u128::from_be_bytes(bytes[82..98].try_into().expect("16")),
            genesis_supply: u128::from_be_bytes(bytes[98..114].try_into().expect("16")),
            native_balance_root: bytes[114..146].try_into().expect("32"),
            episode_ownership_root: bytes[146..178].try_into().expect("32"),
        };
        let _ = roots.encode_canonical()?;
        Ok(roots)
    }

    /// Computes the canonical SHA-256 digest of the encoded roots bytes.
    pub fn canonical_sha256(&self) -> Result<[u8; 32], Asset1Error> {
        let bytes = self.encode_canonical()?;
        Ok(Sha256::digest(bytes).into())
    }

    /// Computes the canonical `Receipt` `ContentTag` for these dual-domain asset roots.
    pub fn tag(&self) -> Result<ContentTag, Asset1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            ASSET_1_ROOTS_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Deterministic proof-carrying receipt emitted for every `ASSET-1` batch evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AssetTransitionReceiptV1 {
    /// Receipt schema version (`1`).
    pub receipt_version: u16,
    /// Bound `PROFILE-D1` profile identifier.
    pub profile_id: [u8; 32],
    /// Block height at which the batch was evaluated.
    pub block_height: u64,
    /// Number of operations supplied in the batch.
    pub op_count: u16,
    /// Deterministic batch disposition (`Admitted` or specific fail-closed rejection).
    pub disposition: AssetDispositionV1,
    /// Total base execution fees burned by this batch (`0` on rollback).
    pub fees_burned_delta: u128,
    /// Total storage deposits newly locked by this batch (`0` on rollback).
    pub deposits_locked_delta: u128,
    /// Total storage deposits refunded by this batch (`0` on rollback).
    pub deposits_refunded_delta: u128,
    /// SHA-256 of canonical pre-batch [`AssetStateRootsV1`].
    pub pre_roots_sha256: [u8; 32],
    /// SHA-256 of canonical post-batch [`AssetStateRootsV1`] (equals `pre_roots_sha256` on rollback).
    pub post_roots_sha256: [u8; 32],
}

impl AssetTransitionReceiptV1 {
    /// Fixed canonical big-endian wire length (`157` bytes).
    pub const WIRE_LEN: usize = 2 + 32 + 8 + 2 + 1 + 16 + 16 + 16 + 32 + 32;

    /// Encodes this transition receipt into 157 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Asset1Error> {
        if self.receipt_version != 1 {
            return Err(Asset1Error::InvalidWire("receipt_version must equal 1"));
        }
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.receipt_version.to_be_bytes());
        out[2..34].copy_from_slice(&self.profile_id);
        out[34..42].copy_from_slice(&self.block_height.to_be_bytes());
        out[42..44].copy_from_slice(&self.op_count.to_be_bytes());
        out[44] = self.disposition.code();
        out[45..61].copy_from_slice(&self.fees_burned_delta.to_be_bytes());
        out[61..77].copy_from_slice(&self.deposits_locked_delta.to_be_bytes());
        out[77..93].copy_from_slice(&self.deposits_refunded_delta.to_be_bytes());
        out[93..125].copy_from_slice(&self.pre_roots_sha256);
        out[125..157].copy_from_slice(&self.post_roots_sha256);
        Ok(out)
    }

    /// Decodes a 157-byte canonical [`AssetTransitionReceiptV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Asset1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Asset1Error::InvalidWire(
                "AssetTransitionReceiptV1 must be 157 bytes",
            ));
        }
        let receipt_version = u16::from_be_bytes([bytes[0], bytes[1]]);
        if receipt_version != 1 {
            return Err(Asset1Error::InvalidWire("receipt_version must equal 1"));
        }
        Ok(Self {
            receipt_version,
            profile_id: bytes[2..34].try_into().expect("32"),
            block_height: u64::from_be_bytes(bytes[34..42].try_into().expect("8")),
            op_count: u16::from_be_bytes([bytes[42], bytes[43]]),
            disposition: AssetDispositionV1::from_code(bytes[44])?,
            fees_burned_delta: u128::from_be_bytes(bytes[45..61].try_into().expect("16")),
            deposits_locked_delta: u128::from_be_bytes(bytes[61..77].try_into().expect("16")),
            deposits_refunded_delta: u128::from_be_bytes(bytes[77..93].try_into().expect("16")),
            pre_roots_sha256: bytes[93..125].try_into().expect("32"),
            post_roots_sha256: bytes[125..157].try_into().expect("32"),
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` for this transition receipt.
    pub fn tag(&self) -> Result<ContentTag, Asset1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            ASSET_1_RECEIPT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Asset operation executed inside an `ASSET-1` atomic batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AssetOperationV1 {
    /// Mint a new mutable occupancy-episode token at `coord`, burning base fee and locking deposit.
    MintEpisode {
        /// Acting owner paying fee and deposit.
        actor: ActorIdV1,
        /// Target non-bedrock coordinate.
        coord: Coord,
        /// Immutable mint episode identifier from `LEDGER-1`.
        mint_episode_id: [u8; 32],
    },
    /// Transfer occupancy-episode ownership and its refundable deposit claim from `from` to `to`.
    TransferEpisode {
        /// Current episode owner paying `base_fee_burn`.
        from: ActorIdV1,
        /// New episode owner receiving transfer/burn and deposit-refund authority.
        to: ActorIdV1,
        /// Target occupied non-bedrock coordinate.
        coord: Coord,
    },
    /// Burn an owned occupancy-episode token, burning `base_fee_burn` and refunding `locked_deposit`.
    BurnEpisode {
        /// Current episode owner receiving the deposit refund.
        actor: ActorIdV1,
        /// Target occupied non-bedrock coordinate.
        coord: Coord,
    },
    /// Transfer fungible native balance from `from` to `to`, burning `base_fee_burn`.
    TransferNative {
        /// Sender paying `amount + base_fee_burn`.
        from: ActorIdV1,
        /// Recipient receiving `amount`.
        to: ActorIdV1,
        /// Positive native unit amount (`> 0`).
        amount: u128,
    },
}

impl AssetOperationV1 {
    /// Returns the acting principal required to authorize this operation.
    pub const fn acting_principal(&self) -> ActorIdV1 {
        match *self {
            Self::MintEpisode { actor, .. } | Self::BurnEpisode { actor, .. } => actor,
            Self::TransferEpisode { from, .. } | Self::TransferNative { from, .. } => from,
        }
    }

    /// Returns the coordinate targeted by this operation, if it touches `EpisodeOwnership`.
    pub const fn target_coord(&self) -> Option<Coord> {
        match *self {
            Self::MintEpisode { coord, .. }
            | Self::TransferEpisode { coord, .. }
            | Self::BurnEpisode { coord, .. } => Some(coord),
            Self::TransferNative { .. } => None,
        }
    }
}

/// Cryptographically authorized `ASSET-1` call binding an [`AssetOperationV1`] to `IDENTITY-1`.
#[derive(Clone, Debug)]
pub struct AuthorizedAssetCallV1 {
    /// Asset state operation to execute if authorization succeeds.
    pub operation: AssetOperationV1,
    /// `IDENTITY-1` transaction body (must match `operation.acting_principal()`).
    pub tx: TransactionBodyV1,
    /// Signer actor identity (principal or delegated actor).
    pub signer_actor_id: ActorIdV1,
    /// Domain-separated `CRYPTO-D1` signing preimage.
    pub preimage: SigningPreimageV1,
    /// Raw `(ed25519_pubkey, ed25519_sig)` witnesses.
    pub raw_signatures: Vec<([u8; 32], [u8; 64])>,
}

/// Authenticated dual-domain asset ledger (`NativeBalanceRootV1` + `EpisodeOwnershipRootV1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetLedgerStateV1 {
    policy: AssetPolicyDescriptorV1,
    profile_id: [u8; 32],
    block_height: u64,
    balances: BTreeMap<ActorIdV1, u128>,
    episodes: BTreeMap<Coord, EpisodeOwnershipValueV1>,
    balance_trie: DomainPatriciaTrieV1,
    episode_trie: DomainPatriciaTrieV1,
    genesis_supply: u128,
    total_burned_fees: u128,
}

impl AssetLedgerStateV1 {
    /// Initializes genesis balances under `policy` and commits both authenticated Patricia tries.
    pub fn from_genesis(
        policy: AssetPolicyDescriptorV1,
        profile_id: [u8; 32],
        genesis_balances: &[(ActorIdV1, u128)],
    ) -> Result<Self, Asset1Error> {
        policy.validate()?;
        let mut balances = BTreeMap::new();
        let mut bal_pairs = Vec::with_capacity(genesis_balances.len());
        let mut genesis_supply = 0u128;
        for &(actor, amount) in genesis_balances {
            if balances.insert(actor, amount).is_some() {
                return Err(Asset1Error::InvalidGenesis(
                    "duplicate actor in genesis_balances",
                ));
            }
            genesis_supply =
                genesis_supply
                    .checked_add(amount)
                    .ok_or(Asset1Error::ConservationViolation(
                        "genesis supply overflow",
                    ))?;
            bal_pairs.push((*actor.as_bytes(), amount.to_be_bytes().to_vec()));
        }
        if genesis_supply > policy.max_native_supply {
            return Err(Asset1Error::InvalidGenesis(
                "genesis_supply exceeds policy.max_native_supply",
            ));
        }
        let balance_trie =
            DomainPatriciaTrieV1::from_entries(DomainIdV1::NativeBalance, bal_pairs)?;
        let episode_trie = DomainPatriciaTrieV1::empty(DomainIdV1::EpisodeOwnership);
        let state = Self {
            policy,
            profile_id,
            block_height: 0,
            balances,
            episodes: BTreeMap::new(),
            balance_trie,
            episode_trie,
            genesis_supply,
            total_burned_fees: 0,
        };
        state.verify_conservation()?;
        Ok(state)
    }

    /// Returns the liquid native balance of `actor`.
    pub fn balance_of(&self, actor: &ActorIdV1) -> u128 {
        self.balances.get(actor).copied().unwrap_or(0)
    }

    /// Returns the live occupancy-episode ownership record at `coord`, if occupied.
    pub fn episode_at(&self, coord: Coord) -> Option<EpisodeOwnershipValueV1> {
        self.episodes.get(&coord).copied()
    }

    /// Returns the current committed [`AssetStateRootsV1`].
    pub fn roots(&self) -> Result<AssetStateRootsV1, Asset1Error> {
        let (total_liquid_supply, total_locked_deposits) = self.sum_supplies()?;
        Ok(AssetStateRootsV1 {
            roots_version: 1,
            profile_id: self.profile_id,
            block_height: self.block_height,
            account_count: self.balance_trie.len() as u32,
            episode_count: self.episode_trie.len() as u32,
            total_liquid_supply,
            total_locked_deposits,
            total_burned_fees: self.total_burned_fees,
            genesis_supply: self.genesis_supply,
            native_balance_root: self.balance_trie.root(),
            episode_ownership_root: self.episode_trie.root(),
        })
    }

    /// Generates an inclusion or absence proof in `NativeBalanceRootV1` (Domain 3).
    pub fn prove_balance(&self, actor: &ActorIdV1) -> Result<DomainPatriciaProofV1, Asset1Error> {
        Ok(self.balance_trie.prove(actor.as_bytes())?)
    }

    /// Generates an inclusion or absence proof in `EpisodeOwnershipRootV1` (Domain 4).
    pub fn prove_episode(&self, coord: Coord) -> Result<DomainPatriciaProofV1, Asset1Error> {
        let key = episode_trie_key(coord);
        Ok(self.episode_trie.prove(&key)?)
    }

    /// Verifies cold rebuild parity across both `NativeBalanceRootV1` and `EpisodeOwnershipRootV1`.
    pub fn verify_rebuild_parity(&self) -> Result<(), Asset1Error> {
        self.balance_trie.verify_rebuild_parity()?;
        self.episode_trie.verify_rebuild_parity()?;
        Ok(())
    }

    /// Verifies closed supply conservation and strict bedrock `(0, 0, 0)` exclusion.
    pub fn verify_conservation(&self) -> Result<(), Asset1Error> {
        if self.episodes.contains_key(&BEDROCK_COORD) {
            return Err(Asset1Error::ConservationViolation(
                "bedrock coordinate holds forbidden economic record",
            ));
        }
        let (liquid, locked) = self.sum_supplies()?;
        let accounted = liquid
            .checked_add(locked)
            .and_then(|v| v.checked_add(self.total_burned_fees))
            .ok_or(Asset1Error::ConservationViolation("supply sum overflow"))?;
        if accounted != self.genesis_supply {
            return Err(Asset1Error::ConservationViolation(
                "closed supply conservation broken",
            ));
        }
        Ok(())
    }

    fn sum_supplies(&self) -> Result<(u128, u128), Asset1Error> {
        let mut liquid = 0u128;
        for &bal in self.balances.values() {
            liquid = liquid
                .checked_add(bal)
                .ok_or(Asset1Error::ConservationViolation("liquid sum overflow"))?;
        }
        let mut locked = 0u128;
        for rec in self.episodes.values() {
            locked = locked
                .checked_add(rec.locked_deposit)
                .ok_or(Asset1Error::ConservationViolation("locked sum overflow"))?;
        }
        Ok((liquid, locked))
    }

    fn set_balance(&mut self, actor: ActorIdV1, new_balance: u128) {
        self.balances.insert(actor, new_balance);
        self.balance_trie
            .upsert(*actor.as_bytes(), new_balance.to_be_bytes().to_vec());
    }

    fn apply_single_op(
        &mut self,
        op: AssetOperationV1,
    ) -> Result<(u128, u128, u128), AssetDispositionV1> {
        match op {
            AssetOperationV1::MintEpisode {
                actor,
                coord,
                mint_episode_id,
            } => {
                if coord == BEDROCK_COORD {
                    return Err(AssetDispositionV1::RejectedBedrockForbidden);
                }
                if self.episodes.contains_key(&coord) {
                    return Err(AssetDispositionV1::RejectedCoordinateOccupied);
                }
                let fee = self.policy.base_fee_burn;
                let dep = self.policy.deposit_per_occupied_block;
                let required = fee
                    .checked_add(dep)
                    .ok_or(AssetDispositionV1::RejectedPolicyOrOverflow)?;
                let avail = self.balance_of(&actor);
                if avail < required {
                    return Err(AssetDispositionV1::RejectedInsufficientBalance);
                }
                self.set_balance(actor, avail - required);
                self.total_burned_fees = self
                    .total_burned_fees
                    .checked_add(fee)
                    .ok_or(AssetDispositionV1::RejectedPolicyOrOverflow)?;
                let rec = EpisodeOwnershipValueV1 {
                    owner: actor,
                    mint_episode_id,
                    locked_deposit: dep,
                };
                self.episodes.insert(coord, rec);
                self.episode_trie
                    .upsert(episode_trie_key(coord), rec.encode_canonical().to_vec());
                Ok((fee, dep, 0))
            }
            AssetOperationV1::TransferEpisode { from, to, coord } => {
                if coord == BEDROCK_COORD {
                    return Err(AssetDispositionV1::RejectedBedrockForbidden);
                }
                let Some(rec) = self.episodes.get(&coord).copied() else {
                    return Err(AssetDispositionV1::RejectedCoordinateEmpty);
                };
                if rec.owner != from {
                    return Err(AssetDispositionV1::RejectedNotEpisodeOwner);
                }
                let fee = self.policy.base_fee_burn;
                let avail = self.balance_of(&from);
                if avail < fee {
                    return Err(AssetDispositionV1::RejectedInsufficientBalance);
                }
                self.set_balance(from, avail - fee);
                self.total_burned_fees = self
                    .total_burned_fees
                    .checked_add(fee)
                    .ok_or(AssetDispositionV1::RejectedPolicyOrOverflow)?;
                let next = EpisodeOwnershipValueV1 {
                    owner: to,
                    mint_episode_id: rec.mint_episode_id,
                    locked_deposit: rec.locked_deposit,
                };
                self.episodes.insert(coord, next);
                self.episode_trie
                    .upsert(episode_trie_key(coord), next.encode_canonical().to_vec());
                Ok((fee, 0, 0))
            }
            AssetOperationV1::BurnEpisode { actor, coord } => {
                if coord == BEDROCK_COORD {
                    return Err(AssetDispositionV1::RejectedBedrockForbidden);
                }
                let Some(rec) = self.episodes.get(&coord).copied() else {
                    return Err(AssetDispositionV1::RejectedCoordinateEmpty);
                };
                if rec.owner != actor {
                    return Err(AssetDispositionV1::RejectedNotEpisodeOwner);
                }
                let fee = self.policy.base_fee_burn;
                let avail = self.balance_of(&actor);
                if avail < fee {
                    return Err(AssetDispositionV1::RejectedInsufficientBalance);
                }
                let next_bal = (avail - fee)
                    .checked_add(rec.locked_deposit)
                    .ok_or(AssetDispositionV1::RejectedPolicyOrOverflow)?;
                self.set_balance(actor, next_bal);
                self.total_burned_fees = self
                    .total_burned_fees
                    .checked_add(fee)
                    .ok_or(AssetDispositionV1::RejectedPolicyOrOverflow)?;
                self.episodes.remove(&coord);
                self.episode_trie.delete(&episode_trie_key(coord));
                Ok((fee, 0, rec.locked_deposit))
            }
            AssetOperationV1::TransferNative { from, to, amount } => {
                if amount == 0 {
                    return Err(AssetDispositionV1::RejectedPolicyOrOverflow);
                }
                let fee = self.policy.base_fee_burn;
                let required = amount
                    .checked_add(fee)
                    .ok_or(AssetDispositionV1::RejectedPolicyOrOverflow)?;
                let from_avail = self.balance_of(&from);
                if from_avail < required {
                    return Err(AssetDispositionV1::RejectedInsufficientBalance);
                }
                self.set_balance(from, from_avail - required);
                let to_avail = self.balance_of(&to);
                let to_next = to_avail
                    .checked_add(amount)
                    .ok_or(AssetDispositionV1::RejectedPolicyOrOverflow)?;
                self.set_balance(to, to_next);
                self.total_burned_fees = self
                    .total_burned_fees
                    .checked_add(fee)
                    .ok_or(AssetDispositionV1::RejectedPolicyOrOverflow)?;
                Ok((fee, 0, 0))
            }
        }
    }

    fn build_receipt(
        &self,
        block_height: u64,
        op_count: u16,
        disposition: AssetDispositionV1,
        deltas: (u128, u128, u128),
        pre_roots_sha256: [u8; 32],
    ) -> Result<AssetTransitionReceiptV1, Asset1Error> {
        let post_roots_sha256 = self.roots()?.canonical_sha256()?;
        Ok(AssetTransitionReceiptV1 {
            receipt_version: 1,
            profile_id: self.profile_id,
            block_height,
            op_count,
            disposition,
            fees_burned_delta: deltas.0,
            deposits_locked_delta: deltas.1,
            deposits_refunded_delta: deltas.2,
            pre_roots_sha256,
            post_roots_sha256,
        })
    }

    /// Atomically applies an already-authenticated batch of [`AssetOperationV1`] transitions,
    /// enforcing concurrent-coordinate conflict refusal, conservation, and full rollback on error.
    pub fn apply_local_batch(
        &mut self,
        ops: &[AssetOperationV1],
        block_height: u64,
    ) -> Result<AssetTransitionReceiptV1, Asset1Error> {
        let pre_sha = self.roots()?.canonical_sha256()?;
        let op_count = u16::try_from(ops.len()).unwrap_or(u16::MAX);
        if ops.is_empty() || ops.len() > usize::from(u16::MAX) {
            return self.build_receipt(
                block_height,
                op_count,
                AssetDispositionV1::RejectedPolicyOrOverflow,
                (0, 0, 0),
                pre_sha,
            );
        }
        let mut touched_coords = BTreeSet::new();
        for op in ops {
            if let Some(c) = op.target_coord()
                && !touched_coords.insert(c)
            {
                return self.build_receipt(
                    block_height,
                    op_count,
                    AssetDispositionV1::RejectedConcurrentWrite,
                    (0, 0, 0),
                    pre_sha,
                );
            }
        }
        let mut staged = self.clone();
        let (mut fees, mut locked, mut refunded) = (0u128, 0u128, 0u128);
        for &op in ops {
            match staged.apply_single_op(op) {
                Ok((f, l, r)) => {
                    fees += f;
                    locked += l;
                    refunded += r;
                }
                Err(disp) => {
                    return self.build_receipt(block_height, op_count, disp, (0, 0, 0), pre_sha);
                }
            }
        }
        staged.block_height = block_height;
        staged.verify_conservation()?;
        *self = staged;
        self.build_receipt(
            block_height,
            op_count,
            AssetDispositionV1::Admitted,
            (fees, locked, refunded),
            pre_sha,
        )
    }

    /// Atomically verifies `IDENTITY-1` + `AUTH-COMMIT-1` signatures/nonces and applies all calls
    /// across both `AssetLedgerStateV1` and `AuthCommitStateV1`, rolling back both on any failure.
    pub fn apply_authorized_batch(
        &mut self,
        auth_state: &mut AuthCommitStateV1,
        calls: &[AuthorizedAssetCallV1],
        block_height: u64,
    ) -> Result<AssetTransitionReceiptV1, Asset1Error> {
        let pre_sha = self.roots()?.canonical_sha256()?;
        let op_count = u16::try_from(calls.len()).unwrap_or(u16::MAX);
        if calls.is_empty() || calls.len() > usize::from(u16::MAX) {
            return self.build_receipt(
                block_height,
                op_count,
                AssetDispositionV1::RejectedPolicyOrOverflow,
                (0, 0, 0),
                pre_sha,
            );
        }
        let mut touched_coords = BTreeSet::new();
        for call in calls {
            if let Some(c) = call.operation.target_coord()
                && !touched_coords.insert(c)
            {
                return self.build_receipt(
                    block_height,
                    op_count,
                    AssetDispositionV1::RejectedConcurrentWrite,
                    (0, 0, 0),
                    pre_sha,
                );
            }
        }
        let mut staged_asset = self.clone();
        let mut staged_auth = auth_state.clone();
        let (mut fees, mut locked, mut refunded) = (0u128, 0u128, 0u128);
        for call in calls {
            if call.tx.actor_id != call.operation.acting_principal() {
                return self.build_receipt(
                    block_height,
                    op_count,
                    AssetDispositionV1::RejectedUnauthorized,
                    (0, 0, 0),
                    pre_sha,
                );
            }
            if let Err(err) = staged_auth.commit_transaction(
                &call.tx,
                call.signer_actor_id,
                &call.preimage,
                &call.raw_signatures,
                block_height,
            ) {
                let disp = match err {
                    AuthCommit1Error::AtomicCommitRejected(
                        IdentityDispositionV1::RejectedStaleOrFutureNonce,
                    ) => AssetDispositionV1::RejectedReplayOrNonce,
                    _ => AssetDispositionV1::RejectedUnauthorized,
                };
                return self.build_receipt(block_height, op_count, disp, (0, 0, 0), pre_sha);
            }
            match staged_asset.apply_single_op(call.operation) {
                Ok((f, l, r)) => {
                    fees += f;
                    locked += l;
                    refunded += r;
                }
                Err(disp) => {
                    return self.build_receipt(block_height, op_count, disp, (0, 0, 0), pre_sha);
                }
            }
        }
        staged_asset.block_height = block_height;
        staged_asset.verify_conservation()?;
        *self = staged_asset;
        *auth_state = staged_auth;
        self.build_receipt(
            block_height,
            op_count,
            AssetDispositionV1::Admitted,
            (fees, locked, refunded),
            pre_sha,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_1_codec_conservation_and_golden_tags() {
        let policy = AssetPolicyDescriptorV1::v1_dual_domain_default();
        let profile_id = [0x44u8; 32];
        let actor_a = ActorIdV1::from_bytes([0x11u8; 32]);
        let actor_b = ActorIdV1::from_bytes([0x22u8; 32]);
        let mut ledger = AssetLedgerStateV1::from_genesis(
            policy,
            profile_id,
            &[(actor_a, 6_000_000), (actor_b, 4_000_000)],
        )
        .expect("genesis");
        let gen_roots = ledger.roots().expect("roots");
        let gen_bytes = gen_roots.encode_canonical().expect("encode");
        assert_eq!(
            AssetStateRootsV1::decode_canonical(&gen_bytes).expect("decode"),
            gen_roots
        );

        let coord1 = Coord::new(1, 2, 3);
        let coord2 = Coord::new(-4, 5, -6);
        let rcpt1 = ledger
            .apply_local_batch(
                &[
                    AssetOperationV1::MintEpisode {
                        actor: actor_a,
                        coord: coord1,
                        mint_episode_id: [0xA1u8; 32],
                    },
                    AssetOperationV1::MintEpisode {
                        actor: actor_a,
                        coord: coord2,
                        mint_episode_id: [0xA2u8; 32],
                    },
                ],
                10,
            )
            .expect("mint batch");
        assert_eq!(rcpt1.disposition, AssetDispositionV1::Admitted);
        let rcpt1_bytes = rcpt1.encode_canonical().expect("rcpt encode");
        assert_eq!(
            AssetTransitionReceiptV1::decode_canonical(&rcpt1_bytes).expect("rcpt decode"),
            rcpt1
        );

        // Transfer coord1 A -> B (carries deposit claim), burn coord2 by A (refunds A).
        let rcpt2 = ledger
            .apply_local_batch(
                &[
                    AssetOperationV1::TransferEpisode {
                        from: actor_a,
                        to: actor_b,
                        coord: coord1,
                    },
                    AssetOperationV1::BurnEpisode {
                        actor: actor_a,
                        coord: coord2,
                    },
                ],
                20,
            )
            .expect("transfer+burn batch");
        assert_eq!(rcpt2.disposition, AssetDispositionV1::Admitted);
        assert_eq!(rcpt2.deposits_refunded_delta, 1_000_000);
        let post_roots = ledger.roots().expect("post roots");
        ledger.verify_rebuild_parity().expect("rebuild parity");
        ledger.verify_conservation().expect("conservation");

        let ep1_proof = ledger.prove_episode(coord1).expect("ep1 proof");
        let ep1_val = ep1_proof
            .verify(
                DomainIdV1::EpisodeOwnership,
                post_roots.episode_ownership_root,
                &episode_trie_key(coord1),
            )
            .expect("verify ep1")
            .expect("present");
        let decoded_ep1 = EpisodeOwnershipValueV1::decode_canonical(&ep1_val).expect("decode ep1");
        assert_eq!(decoded_ep1.owner, actor_b);
        assert_eq!(decoded_ep1.mint_episode_id, [0xA1u8; 32]);

        // Golden tags pinned against independent tools/asset_1_vectors.py
        assert_eq!(
            gen_roots.tag().expect("gen tag").to_string(),
            "#bb.receipt.sha256.d4c462nqb6gg3rzzrraufi4cjv752ys33gfmep44wkltiposlvkq"
        );
        assert_eq!(
            rcpt1.tag().expect("rcpt1 tag").to_string(),
            "#bb.receipt.sha256.vvsftdwi462ds43vitk3d5qx3d3jy2nfxvdk7mupjntewyutrlna"
        );
        assert_eq!(
            rcpt2.tag().expect("rcpt2 tag").to_string(),
            "#bb.receipt.sha256.6s7fa3n5ez24u455z7ossxtldebkmbia3svdwuwhdk4txiccss4q"
        );
        assert_eq!(
            post_roots.tag().expect("post tag").to_string(),
            "#bb.receipt.sha256.yehegtsuyrnm7w6qa3myuotil2egmf6eeagwc2wgmyonqeublfoa"
        );
    }
}
