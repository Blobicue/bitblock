//! `BITS-ECONOMY-1`: canonical Bits creator-economy protocol over Bitblock `ASSET-1`, `IDENTITY-1`,
//! `AUTH-COMMIT-1`, and `CRYPTO-D1` (`wasm32`-clean, zero new Cargo dependencies, testnet/devnet
//! `lab: true` boundary).
//!
//! Realizes the Phase 0 single-chain merge specification (`docs/CHAIN_MERGE_DESIGN.md`) and
//! Cross-Session Amendment 1 (`ActorIdV1` canonical binding + raw Ed25519 lookup alias):
//! - **Unit & closed conservation (`1 Bit = 1 base unit`):** zero floats, zero transfer tax, zero
//!   runtime minting. Every state transition checks exact closed conservation:
//!   `sum(liquid_user_balances) + grant_pool + platform_treasury + simulated_payout_escrow == genesis_supply`.
//! - **Atomic `80/20` creator purchase split (`BitsPurchaseSplitV1`):** integer floor
//!   `creator_bits = (amount_bits * creator_bps) / 10_000` (`creator_bps = 8_000`),
//!   `platform_bits = amount_bits - creator_bits` (`creator_bits + platform_bits == amount_bits`),
//!   replay-guarded on `order_id`.
//! - **Gameladder never mints (`BitsTestnetGrantV1`):** signup (`<= 1,000` Bits), faucet (`<= 50`
//!   Bits), and optional carry-over grants transfer strictly from the fixed genesis-funded
//!   `TestnetGrantPool` and structurally refuse whenever `config.lab == false`.
//! - **Simulated testnet cash-out (`BitsSimulatedPayoutV1`):** requires `config.lab == true` and
//!   `amount_bits >= 100`, transferring creator Bits into `simulated_payout_escrow` (zero real money).
//! - **Amendment 1 canonical `ActorIdV1` + key rotation (`BitsIdentityBindV1`):**
//!   [`derive_default_bits_actor_id`] deterministically maps any raw 32-byte Ed25519 K1 pubkey to
//!   its permanent `ActorIdV1` under namespace `"bits.gameladder.v1"`. Key rotation updates the
//!   active signing key while keeping `ActorIdV1` and its authenticated `DomainPatriciaTrieV1`
//!   balance/nonce history fixed.
//! - **Optional non-transitive depth-1 scoped delegation (`BitsDelegationGrantV1`):** lets a
//!   K1-keyed player authorize bounded `PurchaseSplit` submissions by a relayer key up to
//!   `(max_bits_per_tx, max_bits_total, expires_at_height)` without exposing user private keys.
//!
//! Non-claim: `lab: true` testnet/devnet Bits carry zero real-world monetary value; `L1-PRODUCTION`,
//! `VALUE-1`, mainnet genesis, and production key custody remain strictly excluded.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::auth_commit_1::{
    AuthCommit1Error, DomainIdV1, DomainPatriciaProofV1, DomainPatriciaTrieV1,
};
use crate::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, CryptoD1Error, HashSuiteIdV1,
    SignatureSuiteIdV1, SigningDomainTagV1, SigningPreimageV1, sign_preimage_deterministic,
    verify_preimage_strict, verify_raw_message_strict,
};
use crate::identity_d1::{
    ActorIdV1, AuthorizationDescriptorV1, CredentialRoleV1, IdentityD1Error, ThresholdPolicyV1,
};
use crate::protocol::{ContentTag, TagKind, hash_canonical};

/// Canonical schema string for [`BitsGenesisConfigV1`].
pub const BITS_1_GENESIS_CONFIG_SCHEMA: &[u8] = b"bitblock/bits-1/genesis-config/v1";
/// Canonical schema string for [`BitsTransferV1`] (opcode `0x01`).
pub const BITS_1_TRANSFER_SCHEMA: &[u8] = b"bitblock/bits-1/transfer/v1";
/// Canonical schema string for [`BitsPurchaseSplitV1`] (opcode `0x02`).
pub const BITS_1_PURCHASE_SPLIT_SCHEMA: &[u8] = b"bitblock/bits-1/purchase-split/v1";
/// Canonical schema string for [`BitsTestnetGrantV1`] (opcode `0x03`).
pub const BITS_1_TESTNET_GRANT_SCHEMA: &[u8] = b"bitblock/bits-1/testnet-grant/v1";
/// Canonical schema string for [`BitsSimulatedPayoutV1`] (opcode `0x04`).
pub const BITS_1_SIMULATED_PAYOUT_SCHEMA: &[u8] = b"bitblock/bits-1/simulated-payout/v1";
/// Canonical schema string for [`BitsIdentityBindV1`] (opcode `0x05`).
pub const BITS_1_IDENTITY_BIND_SCHEMA: &[u8] = b"bitblock/bits-1/identity-bind/v1";
/// Canonical schema string for [`BitsL2AnchorV1`] (opcode `0x06`).
pub const BITS_1_L2_ANCHOR_SCHEMA: &[u8] = b"bitblock/bits-1/l2-anchor/v1";
/// Canonical schema string for [`BitsStateReceiptV1`].
pub const BITS_1_STATE_RECEIPT_SCHEMA: &[u8] = b"bitblock/bits-1/state-receipt/v1";

/// Protocol schema version for all `BITS-ECONOMY-1` canonical objects.
pub const BITS_1_VERSION: u16 = 1;
/// Basis-point denominator (`10,000 = 100.00%`).
pub const BITS_BPS_DENOMINATOR: u128 = 10_000;
/// Default creator share in basis points (`8,000 = 80.00%`).
pub const BITS_DEFAULT_CREATOR_SPLIT_BPS: u16 = 8_000;
/// Default signup testnet grant ceiling (`1,000` Bits).
pub const BITS_DEFAULT_SIGNUP_GRANT: u128 = 1_000;
/// Default faucet testnet drip ceiling (`50` Bits).
pub const BITS_DEFAULT_FAUCET_GRANT: u128 = 50;
/// Default minimum marketplace/pass purchase price (`10` Bits).
pub const BITS_DEFAULT_MIN_PURCHASE: u128 = 10;
/// Default minimum simulated testnet payout amount (`100` Bits).
pub const BITS_DEFAULT_MIN_PAYOUT: u128 = 100;
/// Canonical identity namespace used by [`derive_default_bits_actor_id`] (Amendment 1).
pub const BITS_IDENTITY_NAMESPACE: &str = "bits.gameladder.v1";
/// Mandatory testnet custody label stamped on all devnet/testnet Bits configurations.
pub const BITS_TESTNET_CUSTODY_LABEL: &str = "DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY";
/// Mandatory disclaimer label for testnet-only simulated creator payouts.
pub const BITS_SIMULATED_PAYOUT_LABEL: &str = "SIMULATED-TEST-ONLY-NO-REAL-MONEY";

const IDENTITY_BIND_POP_PREFIX: &[u8] = b"bb.bits1.identity_bind.v1";
const SIGNED_TX_PAYLOAD_PREFIX: &[u8] = b"bb.bits1.signed_tx.v1";
const ORDER_MEMO_PREFIX: &[u8] = b"gameladder-order/v1\0";
const SYSTEM_ACTOR_PREFIX: &[u8] = b"bb.bits1.system_actor.v1";

/// Fail-closed error returned by `BITS-ECONOMY-1` codecs, verifiers, and state transitions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BitsEconomy1Error {
    /// Underlying `CRYPTO-D1` strict Ed25519 or preimage error.
    Crypto(CryptoD1Error),
    /// Underlying `IDENTITY-D1` actor derivation or descriptor error.
    Identity(IdentityD1Error),
    /// Underlying `AUTH-COMMIT-1` Patricia trie or proof error.
    AuthCommit(AuthCommit1Error),
    /// Canonical wire buffer had invalid version, length, discriminant, or trailing bytes.
    InvalidWire(&'static str),
    /// Genesis configuration violated supply, split, or distinct system-actor bounds.
    InvalidGenesis(&'static str),
    /// Exact closed supply conservation invariant failed or arithmetic overflowed.
    ConservationViolation(&'static str),
    /// Operation was structurally refused because `genesis.lab == false`.
    TestnetOnlyDisabled(&'static str),
    /// Transaction signature, signer authority, delegation scope, or PoP failed.
    Unauthorized(&'static str),
    /// Transaction nonce, validity window, or replay guard (`order_id`/`grant_id`) failed.
    ReplayOrNonce(&'static str),
    /// Account or `TestnetGrantPool` lacked sufficient liquid Bits.
    InsufficientBalance(&'static str),
    /// Business rule bound failed (price below minimum, wrong treasury, non-monotone epoch).
    PolicyViolation(&'static str),
}

impl From<CryptoD1Error> for BitsEconomy1Error {
    fn from(value: CryptoD1Error) -> Self {
        Self::Crypto(value)
    }
}

impl From<IdentityD1Error> for BitsEconomy1Error {
    fn from(value: IdentityD1Error) -> Self {
        Self::Identity(value)
    }
}

impl From<AuthCommit1Error> for BitsEconomy1Error {
    fn from(value: AuthCommit1Error) -> Self {
        Self::AuthCommit(value)
    }
}

impl fmt::Display for BitsEconomy1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for BitsEconomy1Error {}

/// Derives a deterministic protocol system [`ActorIdV1`] (treasury, grant pool, or payout escrow).
pub fn derive_system_actor_id(chain_id: u64, label: &str) -> ActorIdV1 {
    let mut h = Sha256::new();
    h.update(SYSTEM_ACTOR_PREFIX);
    h.update(chain_id.to_be_bytes());
    h.update((label.len() as u32).to_be_bytes());
    h.update(label.as_bytes());
    ActorIdV1::from_bytes(h.finalize().into())
}

/// Deterministically derives the canonical [`ActorIdV1`] for a raw 32-byte Ed25519 K1 public key
/// under [`BITS_IDENTITY_NAMESPACE`] (Cross-Session Amendment 1).
pub fn derive_default_bits_actor_id(
    chain_id: u64,
    pubkey: &CanonicalEd25519PublicKeyV1,
) -> Result<ActorIdV1, BitsEconomy1Error> {
    let desc = AuthorizationDescriptorV1 {
        descriptor_version: 0,
        action_policy: ThresholdPolicyV1 {
            role: CredentialRoleV1::ActorAction,
            threshold: 1,
            keys: vec![*pubkey],
        },
        admin_policy: ThresholdPolicyV1 {
            role: CredentialRoleV1::ActorAdmin,
            threshold: 1,
            keys: vec![*pubkey],
        },
        delegations: Vec::new(),
        revoked: false,
    };
    Ok(ActorIdV1::derive(chain_id, BITS_IDENTITY_NAMESPACE, &desc)?)
}

/// Computes the canonical `gameladder-order/v1` `memo_hash` bound inside [`BitsPurchaseSplitV1`].
pub fn compute_purchase_memo_hash(
    order_id: &[u8; 32],
    world_id: &str,
    item_id: &str,
    buyer: &ActorIdV1,
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(ORDER_MEMO_PREFIX);
    h.update(order_id);
    h.update((world_id.len() as u32).to_be_bytes());
    h.update(world_id.as_bytes());
    h.update((item_id.len() as u32).to_be_bytes());
    h.update(item_id.as_bytes());
    h.update(buyer.as_bytes());
    h.finalize().into()
}

/// Computes the exact integer `80/20` creator/platform split (`creator + platform == amount_bits`).
pub fn compute_purchase_split(
    amount_bits: u128,
    creator_bps: u16,
) -> Result<(u128, u128), BitsEconomy1Error> {
    if u128::from(creator_bps) > BITS_BPS_DENOMINATOR {
        return Err(BitsEconomy1Error::PolicyViolation(
            "creator_bps exceeds 10000",
        ));
    }
    let scaled = amount_bits.checked_mul(u128::from(creator_bps)).ok_or(
        BitsEconomy1Error::ConservationViolation("purchase split multiplication overflow"),
    )?;
    let creator_bits = scaled / BITS_BPS_DENOMINATOR;
    let platform_bits = amount_bits - creator_bits;
    Ok((creator_bits, platform_bits))
}

/// Computes the domain-separated proof-of-possession message signed by `ed25519_pk` in
/// [`BitsIdentityBindV1`].
pub fn identity_bind_pop_message(
    chain_id: u64,
    genesis_id: &[u8; 32],
    actor_id: &ActorIdV1,
    ed25519_pk: &CanonicalEd25519PublicKeyV1,
    provider: BitsIdentityProviderV1,
    external_id_hash: &[u8; 32],
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(IDENTITY_BIND_POP_PREFIX);
    h.update(chain_id.to_be_bytes());
    h.update(genesis_id);
    h.update(actor_id.as_bytes());
    h.update(ed25519_pk.as_bytes());
    h.push_byte_digest(provider.code());
    h.update(external_id_hash);
    h.finalize().into()
}

/// Signs the canonical [`identity_bind_pop_message`] using a 32-byte Ed25519 secret seed.
pub fn sign_identity_bind_pop_with_seed(
    sk_seed: &[u8; 32],
    chain_id: u64,
    genesis_id: &[u8; 32],
    actor_id: &ActorIdV1,
    provider: BitsIdentityProviderV1,
    external_id_hash: &[u8; 32],
) -> Result<CanonicalEd25519SignatureV1, BitsEconomy1Error> {
    use ed25519_dalek::{Signer, SigningKey};
    let pk = CanonicalEd25519PublicKeyV1::from_secret_seed(sk_seed)?;
    let msg = identity_bind_pop_message(
        chain_id,
        genesis_id,
        actor_id,
        &pk,
        provider,
        external_id_hash,
    );
    let sk = SigningKey::from_bytes(sk_seed);
    let sig = sk.sign(&msg);
    Ok(CanonicalEd25519SignatureV1::from_bytes(sig.to_bytes())?)
}

trait Sha256ByteExt {
    fn push_byte_digest(&mut self, b: u8);
}

impl Sha256ByteExt for Sha256 {
    fn push_byte_digest(&mut self, b: u8) {
        self.update([b]);
    }
}

/// Testnet grant classification carried in [`BitsTestnetGrantV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum BitsGrantKindV1 {
    /// One-time `1,000` Bits new-account testnet signup grant.
    Signup = 1,
    /// Rate-limited `<= 50` Bits `/faucet` developer test drip.
    Faucet = 2,
    /// Optional deterministic testnet cutover carry-over allocation from `TestnetGrantPool`.
    CarryOver = 3,
}

impl BitsGrantKindV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte grant-kind discriminant.
    pub const fn from_code(code: u8) -> Result<Self, BitsEconomy1Error> {
        match code {
            1 => Ok(Self::Signup),
            2 => Ok(Self::Faucet),
            3 => Ok(Self::CarryOver),
            _ => Err(BitsEconomy1Error::InvalidWire("unknown BitsGrantKindV1")),
        }
    }
}

/// External identity provider namespace bound in [`BitsIdentityBindV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum BitsIdentityProviderV1 {
    /// Gameladder `playerHash` (64-char SHA-256 hex).
    GameladderPlayerHash = 1,
    /// Bitblock Studio creator/player handle.
    StudioHandle = 2,
    /// Discord OAuth subject (`d:<snowflake>`).
    Discord = 3,
}

impl BitsIdentityProviderV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte provider discriminant.
    pub const fn from_code(code: u8) -> Result<Self, BitsEconomy1Error> {
        match code {
            1 => Ok(Self::GameladderPlayerHash),
            2 => Ok(Self::StudioHandle),
            3 => Ok(Self::Discord),
            _ => Err(BitsEconomy1Error::InvalidWire(
                "unknown BitsIdentityProviderV1",
            )),
        }
    }
}

/// Deterministic disposition recorded inside [`BitsStateReceiptV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BitsDispositionV1 {
    /// Transaction verified, applied atomically, and passed closed conservation.
    Admitted = 0,
    /// Transaction rejected because `genesis.lab == false`.
    RejectedTestnetOnly = 1,
    /// Transaction rejected due to invalid signature, delegation scope, or PoP.
    RejectedUnauthorized = 2,
    /// Transaction rejected due to nonce, expiry, or duplicate `order_id`/`grant_id`.
    RejectedReplayOrNonce = 3,
    /// Transaction rejected due to insufficient balance or empty `TestnetGrantPool`.
    RejectedInsufficientBalance = 4,
    /// Transaction rejected due to minimum price, wrong treasury, or monotone-epoch violation.
    RejectedPolicy = 5,
}

impl BitsDispositionV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte disposition discriminant.
    pub const fn from_code(code: u8) -> Result<Self, BitsEconomy1Error> {
        match code {
            0 => Ok(Self::Admitted),
            1 => Ok(Self::RejectedTestnetOnly),
            2 => Ok(Self::RejectedUnauthorized),
            3 => Ok(Self::RejectedReplayOrNonce),
            4 => Ok(Self::RejectedInsufficientBalance),
            5 => Ok(Self::RejectedPolicy),
            _ => Err(BitsEconomy1Error::InvalidWire("unknown BitsDispositionV1")),
        }
    }
}

/// Immutable Bits genesis configuration (`205` canonical big-endian wire bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BitsGenesisConfigV1 {
    /// Schema version (`1`).
    pub version: u16,
    /// Testnet/devnet lab mode flag (`true` unlocks `TestnetGrantPool` and simulated payouts).
    pub lab: bool,
    /// Unique chain identifier bound in every signing preimage and `ActorIdV1`.
    pub chain_id: u64,
    /// Fixed total Bits supply conserved across every block (`zero` runtime minting).
    pub genesis_supply: u128,
    /// Initial balance allocated to `grant_pool_actor` at height 0 (`<= genesis_supply`).
    pub initial_grant_pool: u128,
    /// Protocol treasury actor receiving the 20% platform split on [`BitsPurchaseSplitV1`].
    pub platform_treasury_actor: ActorIdV1,
    /// Fixed genesis-funded testnet grant pool actor debited by [`BitsTestnetGrantV1`].
    pub grant_pool_actor: ActorIdV1,
    /// Testnet-only simulated payout escrow actor credited by [`BitsSimulatedPayoutV1`].
    pub payout_escrow_actor: ActorIdV1,
    /// Maximum Bits per `Signup` grant (`1,000` default).
    pub max_signup_grant_bits: u128,
    /// Maximum Bits per `Faucet` drip (`50` default).
    pub max_faucet_grant_bits: u128,
    /// Minimum purchase price for [`BitsPurchaseSplitV1`] (`10` default).
    pub min_purchase_bits: u128,
    /// Creator share in basis points (`8,000 = 80.00%` default).
    pub creator_split_bps: u16,
    /// Minimum simulated creator payout (`100` default).
    pub min_payout_bits: u128,
}

impl BitsGenesisConfigV1 {
    /// Exact canonical big-endian wire byte length (`205` bytes).
    pub const WIRE_LEN: usize = 2 + 1 + 8 + 16 + 16 + 32 + 32 + 32 + 16 + 16 + 16 + 2 + 16;

    /// Returns the deterministic canonical `lab: true` Bitblock Bits testnet configuration.
    pub fn canonical_testnet_default() -> Self {
        let chain_id = 0x4249_5453_0000_0001u64; // "BITS" + testnet v1
        let genesis_supply = 1_000_000_000u128; // 1B testnet Bits
        Self {
            version: BITS_1_VERSION,
            lab: true,
            chain_id,
            genesis_supply,
            initial_grant_pool: genesis_supply,
            platform_treasury_actor: derive_system_actor_id(chain_id, "platform-treasury"),
            grant_pool_actor: derive_system_actor_id(chain_id, "testnet-grant-pool"),
            payout_escrow_actor: derive_system_actor_id(chain_id, "simulated-payout-escrow"),
            max_signup_grant_bits: BITS_DEFAULT_SIGNUP_GRANT,
            max_faucet_grant_bits: BITS_DEFAULT_FAUCET_GRANT,
            min_purchase_bits: BITS_DEFAULT_MIN_PURCHASE,
            creator_split_bps: BITS_DEFAULT_CREATOR_SPLIT_BPS,
            min_payout_bits: BITS_DEFAULT_MIN_PAYOUT,
        }
    }

    /// Validates all genesis bounds and distinct system actor identities.
    pub fn validate(&self) -> Result<(), BitsEconomy1Error> {
        if self.version != BITS_1_VERSION {
            return Err(BitsEconomy1Error::InvalidGenesis("version must equal 1"));
        }
        if self.chain_id == 0 {
            return Err(BitsEconomy1Error::InvalidGenesis("chain_id must be > 0"));
        }
        if self.genesis_supply == 0 {
            return Err(BitsEconomy1Error::InvalidGenesis(
                "genesis_supply must be > 0",
            ));
        }
        if self.initial_grant_pool > self.genesis_supply {
            return Err(BitsEconomy1Error::InvalidGenesis(
                "initial_grant_pool exceeds genesis_supply",
            ));
        }
        if self.platform_treasury_actor == self.grant_pool_actor
            || self.platform_treasury_actor == self.payout_escrow_actor
            || self.grant_pool_actor == self.payout_escrow_actor
        {
            return Err(BitsEconomy1Error::InvalidGenesis(
                "system actors must be pairwise distinct",
            ));
        }
        if self.min_purchase_bits == 0 || self.min_payout_bits == 0 {
            return Err(BitsEconomy1Error::InvalidGenesis(
                "min_purchase_bits and min_payout_bits must be > 0",
            ));
        }
        if u128::from(self.creator_split_bps) > BITS_BPS_DENOMINATOR {
            return Err(BitsEconomy1Error::InvalidGenesis(
                "creator_split_bps must be <= 10000",
            ));
        }
        Ok(())
    }

    /// Encodes this genesis config into exact 205-byte canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], BitsEconomy1Error> {
        self.validate()?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.version.to_be_bytes());
        out[2] = u8::from(self.lab);
        out[3..11].copy_from_slice(&self.chain_id.to_be_bytes());
        out[11..27].copy_from_slice(&self.genesis_supply.to_be_bytes());
        out[27..43].copy_from_slice(&self.initial_grant_pool.to_be_bytes());
        out[43..75].copy_from_slice(self.platform_treasury_actor.as_bytes());
        out[75..107].copy_from_slice(self.grant_pool_actor.as_bytes());
        out[107..139].copy_from_slice(self.payout_escrow_actor.as_bytes());
        out[139..155].copy_from_slice(&self.max_signup_grant_bits.to_be_bytes());
        out[155..171].copy_from_slice(&self.max_faucet_grant_bits.to_be_bytes());
        out[171..187].copy_from_slice(&self.min_purchase_bits.to_be_bytes());
        out[187..189].copy_from_slice(&self.creator_split_bps.to_be_bytes());
        out[189..205].copy_from_slice(&self.min_payout_bits.to_be_bytes());
        Ok(out)
    }

    /// Decodes and validates a 205-byte canonical [`BitsGenesisConfigV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, BitsEconomy1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(BitsEconomy1Error::InvalidWire(
                "BitsGenesisConfigV1 must be 205 bytes",
            ));
        }
        let lab = match bytes[2] {
            0 => false,
            1 => true,
            _ => return Err(BitsEconomy1Error::InvalidWire("lab must be 0 or 1")),
        };
        let cfg = Self {
            version: u16::from_be_bytes([bytes[0], bytes[1]]),
            lab,
            chain_id: u64::from_be_bytes(bytes[3..11].try_into().expect("8")),
            genesis_supply: u128::from_be_bytes(bytes[11..27].try_into().expect("16")),
            initial_grant_pool: u128::from_be_bytes(bytes[27..43].try_into().expect("16")),
            platform_treasury_actor: ActorIdV1::from_bytes(bytes[43..75].try_into().expect("32")),
            grant_pool_actor: ActorIdV1::from_bytes(bytes[75..107].try_into().expect("32")),
            payout_escrow_actor: ActorIdV1::from_bytes(bytes[107..139].try_into().expect("32")),
            max_signup_grant_bits: u128::from_be_bytes(bytes[139..155].try_into().expect("16")),
            max_faucet_grant_bits: u128::from_be_bytes(bytes[155..171].try_into().expect("16")),
            min_purchase_bits: u128::from_be_bytes(bytes[171..187].try_into().expect("16")),
            creator_split_bps: u16::from_be_bytes([bytes[187], bytes[188]]),
            min_payout_bits: u128::from_be_bytes(bytes[189..205].try_into().expect("16")),
        };
        cfg.validate()?;
        Ok(cfg)
    }

    /// Computes the canonical `Policy` [`ContentTag`] for this genesis configuration.
    pub fn tag(&self) -> Result<ContentTag, BitsEconomy1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            BITS_1_GENESIS_CONFIG_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }

    /// Returns the 32-byte genesis digest committed into every signed Bits transaction.
    pub fn genesis_id(&self) -> Result<[u8; 32], BitsEconomy1Error> {
        Ok(self.tag()?.digest())
    }
}

/// Opcode `0x01`: direct integer Bits transfer (`0%` transfer tax, `112` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BitsTransferV1 {
    /// Sender actor identifier.
    pub from: ActorIdV1,
    /// Recipient actor identifier (`!= from`).
    pub to: ActorIdV1,
    /// Positive integer Bits amount transferred.
    pub amount_bits: u128,
    /// Optional 32-byte memo digest (`[0; 32]` when unused).
    pub memo_hash: [u8; 32],
}

impl BitsTransferV1 {
    /// Exact canonical big-endian wire byte length (`112` bytes).
    pub const WIRE_LEN: usize = 32 + 32 + 16 + 32;

    /// Encodes this transfer into 112 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], BitsEconomy1Error> {
        if self.amount_bits == 0 || self.from == self.to {
            return Err(BitsEconomy1Error::PolicyViolation(
                "transfer requires amount_bits > 0 and from != to",
            ));
        }
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..32].copy_from_slice(self.from.as_bytes());
        out[32..64].copy_from_slice(self.to.as_bytes());
        out[64..80].copy_from_slice(&self.amount_bits.to_be_bytes());
        out[80..112].copy_from_slice(&self.memo_hash);
        Ok(out)
    }

    /// Decodes a 112-byte canonical [`BitsTransferV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, BitsEconomy1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(BitsEconomy1Error::InvalidWire(
                "BitsTransferV1 must be 112 bytes",
            ));
        }
        let tx = Self {
            from: ActorIdV1::from_bytes(bytes[0..32].try_into().expect("32")),
            to: ActorIdV1::from_bytes(bytes[32..64].try_into().expect("32")),
            amount_bits: u128::from_be_bytes(bytes[64..80].try_into().expect("16")),
            memo_hash: bytes[80..112].try_into().expect("32"),
        };
        if tx.amount_bits == 0 || tx.from == tx.to {
            return Err(BitsEconomy1Error::PolicyViolation(
                "transfer requires amount_bits > 0 and from != to",
            ));
        }
        Ok(tx)
    }

    /// Computes the canonical `Receipt` [`ContentTag`] for this transfer payload.
    pub fn tag(&self) -> Result<ContentTag, BitsEconomy1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            BITS_1_TRANSFER_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Opcode `0x02`: atomic `80/20` creator/platform purchase split (`178` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BitsPurchaseSplitV1 {
    /// Buyer actor identifier debited `amount_bits`.
    pub buyer: ActorIdV1,
    /// Creator actor identifier credited `floor(amount_bits * creator_bps / 10_000)`.
    pub creator: ActorIdV1,
    /// Platform treasury actor credited `amount_bits - creator_bits`.
    pub treasury: ActorIdV1,
    /// Gross integer purchase price in Bits (`>= min_purchase_bits`).
    pub amount_bits: u128,
    /// Creator split in basis points (`8,000 = 80.00%`).
    pub creator_bps: u16,
    /// Unique 32-byte order identifier preventing duplicate settlement replay.
    pub order_id: [u8; 32],
    /// Canonical `gameladder-order/v1` commitment digest (`world_id`, `item_id`, `buyer`).
    pub memo_hash: [u8; 32],
}

impl BitsPurchaseSplitV1 {
    /// Exact canonical big-endian wire byte length (`178` bytes).
    pub const WIRE_LEN: usize = 32 + 32 + 32 + 16 + 2 + 32 + 32;

    /// Encodes this purchase split into 178 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], BitsEconomy1Error> {
        if self.amount_bits == 0 || self.buyer == self.creator {
            return Err(BitsEconomy1Error::PolicyViolation(
                "purchase split requires amount_bits > 0 and buyer != creator",
            ));
        }
        let _ = compute_purchase_split(self.amount_bits, self.creator_bps)?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..32].copy_from_slice(self.buyer.as_bytes());
        out[32..64].copy_from_slice(self.creator.as_bytes());
        out[64..96].copy_from_slice(self.treasury.as_bytes());
        out[96..112].copy_from_slice(&self.amount_bits.to_be_bytes());
        out[112..114].copy_from_slice(&self.creator_bps.to_be_bytes());
        out[114..146].copy_from_slice(&self.order_id);
        out[146..178].copy_from_slice(&self.memo_hash);
        Ok(out)
    }

    /// Decodes a 178-byte canonical [`BitsPurchaseSplitV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, BitsEconomy1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(BitsEconomy1Error::InvalidWire(
                "BitsPurchaseSplitV1 must be 178 bytes",
            ));
        }
        let tx = Self {
            buyer: ActorIdV1::from_bytes(bytes[0..32].try_into().expect("32")),
            creator: ActorIdV1::from_bytes(bytes[32..64].try_into().expect("32")),
            treasury: ActorIdV1::from_bytes(bytes[64..96].try_into().expect("32")),
            amount_bits: u128::from_be_bytes(bytes[96..112].try_into().expect("16")),
            creator_bps: u16::from_be_bytes([bytes[112], bytes[113]]),
            order_id: bytes[114..146].try_into().expect("32"),
            memo_hash: bytes[146..178].try_into().expect("32"),
        };
        let _ = tx.encode_canonical()?;
        Ok(tx)
    }

    /// Computes the canonical `Receipt` [`ContentTag`] for this purchase-split payload.
    pub fn tag(&self) -> Result<ContentTag, BitsEconomy1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            BITS_1_PURCHASE_SPLIT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Opcode `0x03`: testnet-only (`lab == true`) grant debited from `TestnetGrantPool` (`113` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BitsTestnetGrantV1 {
    /// Recipient canonical [`ActorIdV1`].
    pub recipient: ActorIdV1,
    /// Recipient's canonical Ed25519 K1 public key registered if not yet bound.
    pub recipient_pubkey: CanonicalEd25519PublicKeyV1,
    /// Positive Bits amount debited strictly from `TestnetGrantPool`.
    pub amount_bits: u128,
    /// Grant category (`Signup`, `Faucet`, or `CarryOver`).
    pub grant_kind: BitsGrantKindV1,
    /// Unique idempotency key preventing duplicate grant replay.
    pub grant_id: [u8; 32],
}

impl BitsTestnetGrantV1 {
    /// Exact canonical big-endian wire byte length (`113` bytes).
    pub const WIRE_LEN: usize = 32 + 32 + 16 + 1 + 32;

    /// Encodes this testnet grant into 113 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], BitsEconomy1Error> {
        if self.amount_bits == 0 {
            return Err(BitsEconomy1Error::PolicyViolation(
                "testnet grant amount_bits must be > 0",
            ));
        }
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..32].copy_from_slice(self.recipient.as_bytes());
        out[32..64].copy_from_slice(self.recipient_pubkey.as_bytes());
        out[64..80].copy_from_slice(&self.amount_bits.to_be_bytes());
        out[80] = self.grant_kind.code();
        out[81..113].copy_from_slice(&self.grant_id);
        Ok(out)
    }

    /// Decodes a 113-byte canonical [`BitsTestnetGrantV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, BitsEconomy1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(BitsEconomy1Error::InvalidWire(
                "BitsTestnetGrantV1 must be 113 bytes",
            ));
        }
        let recipient = ActorIdV1::from_bytes(bytes[0..32].try_into().expect("32"));
        let recipient_pubkey = CanonicalEd25519PublicKeyV1::from_slice(&bytes[32..64])?;
        let amount_bits = u128::from_be_bytes(bytes[64..80].try_into().expect("16"));
        let grant_kind = BitsGrantKindV1::from_code(bytes[80])?;
        let grant_id: [u8; 32] = bytes[81..113].try_into().expect("32");
        if amount_bits == 0 {
            return Err(BitsEconomy1Error::PolicyViolation(
                "testnet grant amount_bits must be > 0",
            ));
        }
        Ok(Self {
            recipient,
            recipient_pubkey,
            amount_bits,
            grant_kind,
            grant_id,
        })
    }

    /// Computes the canonical `Receipt` [`ContentTag`] for this testnet grant payload.
    pub fn tag(&self) -> Result<ContentTag, BitsEconomy1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            BITS_1_TESTNET_GRANT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Opcode `0x04`: testnet-only (`lab == true`) simulated creator cash-out escrow (`80` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BitsSimulatedPayoutV1 {
    /// Creator actor identifier debited `amount_bits` into `simulated_payout_escrow`.
    pub creator: ActorIdV1,
    /// Simulated payout amount in Bits (`>= min_payout_bits`).
    pub amount_bits: u128,
    /// Unique 32-byte payout request identifier preventing replay.
    pub payout_request_id: [u8; 32],
}

impl BitsSimulatedPayoutV1 {
    /// Exact canonical big-endian wire byte length (`80` bytes).
    pub const WIRE_LEN: usize = 32 + 16 + 32;

    /// Encodes this simulated payout into 80 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], BitsEconomy1Error> {
        if self.amount_bits == 0 {
            return Err(BitsEconomy1Error::PolicyViolation(
                "simulated payout amount_bits must be > 0",
            ));
        }
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..32].copy_from_slice(self.creator.as_bytes());
        out[32..48].copy_from_slice(&self.amount_bits.to_be_bytes());
        out[48..80].copy_from_slice(&self.payout_request_id);
        Ok(out)
    }

    /// Decodes an 80-byte canonical [`BitsSimulatedPayoutV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, BitsEconomy1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(BitsEconomy1Error::InvalidWire(
                "BitsSimulatedPayoutV1 must be 80 bytes",
            ));
        }
        let amount_bits = u128::from_be_bytes(bytes[32..48].try_into().expect("16"));
        if amount_bits == 0 {
            return Err(BitsEconomy1Error::PolicyViolation(
                "simulated payout amount_bits must be > 0",
            ));
        }
        Ok(Self {
            creator: ActorIdV1::from_bytes(bytes[0..32].try_into().expect("32")),
            amount_bits,
            payout_request_id: bytes[48..80].try_into().expect("32"),
        })
    }

    /// Computes the canonical `Receipt` [`ContentTag`] for this simulated payout payload.
    pub fn tag(&self) -> Result<ContentTag, BitsEconomy1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            BITS_1_SIMULATED_PAYOUT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Opcode `0x05`: non-custodial identity binding & key rotation (`161` bytes, Amendment 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BitsIdentityBindV1 {
    /// Permanent canonical [`ActorIdV1`] preserved across key rotations.
    pub actor_id: ActorIdV1,
    /// New or initial canonical Ed25519 public key bound to `actor_id`.
    pub ed25519_pk: CanonicalEd25519PublicKeyV1,
    /// External platform identity namespace (`GameladderPlayerHash`, `StudioHandle`, `Discord`).
    pub provider: BitsIdentityProviderV1,
    /// SHA-256 hash of the external identity identifier.
    pub external_id_hash: [u8; 32],
    /// Strict RFC 8032 proof-of-possession signature made by `ed25519_pk`.
    pub pop_signature: CanonicalEd25519SignatureV1,
}

impl BitsIdentityBindV1 {
    /// Exact canonical big-endian wire byte length (`161` bytes).
    pub const WIRE_LEN: usize = 32 + 32 + 1 + 32 + 64;

    /// Encodes this identity binding into 161 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..32].copy_from_slice(self.actor_id.as_bytes());
        out[32..64].copy_from_slice(self.ed25519_pk.as_bytes());
        out[64] = self.provider.code();
        out[65..97].copy_from_slice(&self.external_id_hash);
        out[97..161].copy_from_slice(self.pop_signature.as_bytes());
        out
    }

    /// Decodes and structurally validates a 161-byte canonical [`BitsIdentityBindV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, BitsEconomy1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(BitsEconomy1Error::InvalidWire(
                "BitsIdentityBindV1 must be 161 bytes",
            ));
        }
        Ok(Self {
            actor_id: ActorIdV1::from_bytes(bytes[0..32].try_into().expect("32")),
            ed25519_pk: CanonicalEd25519PublicKeyV1::from_slice(&bytes[32..64])?,
            provider: BitsIdentityProviderV1::from_code(bytes[64])?,
            external_id_hash: bytes[65..97].try_into().expect("32"),
            pop_signature: CanonicalEd25519SignatureV1::from_slice(&bytes[97..161])?,
        })
    }

    /// Computes the canonical `Receipt` [`ContentTag`] for this identity-bind payload.
    pub fn tag(&self) -> ContentTag {
        let bytes = self.encode_canonical();
        hash_canonical(
            TagKind::Receipt,
            BITS_1_IDENTITY_BIND_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length")
    }
}

/// Opcode `0x06`: audit-only epoch commitment with zero minting authority (`76` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BitsL2AnchorV1 {
    /// SHA-256 digest of the anchored application identifier (e.g. `"gameladder"`).
    pub app_id_hash: [u8; 32],
    /// Strictly increasing epoch number for `app_id_hash`.
    pub epoch_id: u64,
    /// 32-byte L2 event archive Merkle root.
    pub l2_root: [u8; 32],
    /// Number of verified events committed in `l2_root`.
    pub event_count: u32,
}

impl BitsL2AnchorV1 {
    /// Exact canonical big-endian wire byte length (`76` bytes).
    pub const WIRE_LEN: usize = 32 + 8 + 32 + 4;

    /// Encodes this audit-only anchor into 76 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..32].copy_from_slice(&self.app_id_hash);
        out[32..40].copy_from_slice(&self.epoch_id.to_be_bytes());
        out[40..72].copy_from_slice(&self.l2_root);
        out[72..76].copy_from_slice(&self.event_count.to_be_bytes());
        out
    }

    /// Decodes a 76-byte canonical [`BitsL2AnchorV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, BitsEconomy1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(BitsEconomy1Error::InvalidWire(
                "BitsL2AnchorV1 must be 76 bytes",
            ));
        }
        Ok(Self {
            app_id_hash: bytes[0..32].try_into().expect("32"),
            epoch_id: u64::from_be_bytes(bytes[32..40].try_into().expect("8")),
            l2_root: bytes[40..72].try_into().expect("32"),
            event_count: u32::from_be_bytes(bytes[72..76].try_into().expect("4")),
        })
    }

    /// Computes the canonical `Receipt` [`ContentTag`] for this L2 anchor payload.
    pub fn tag(&self) -> ContentTag {
        let bytes = self.encode_canonical();
        hash_canonical(
            TagKind::Receipt,
            BITS_1_L2_ANCHOR_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length")
    }
}

/// Union of all six canonical Bits creator-economy transaction payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BitsTxPayloadV1 {
    /// `0x01`: direct zero-tax Bits transfer.
    Transfer(BitsTransferV1),
    /// `0x02`: atomic `80/20` creator/platform purchase split.
    PurchaseSplit(BitsPurchaseSplitV1),
    /// `0x03`: testnet-only (`lab == true`) grant from `TestnetGrantPool`.
    TestnetGrant(BitsTestnetGrantV1),
    /// `0x04`: testnet-only (`lab == true`) simulated creator cash-out into escrow.
    SimulatedPayout(BitsSimulatedPayoutV1),
    /// `0x05`: non-custodial identity binding & key rotation (Amendment 1).
    IdentityBind(BitsIdentityBindV1),
    /// `0x06`: audit-only L2 epoch commitment (zero minting authority).
    L2Anchor(BitsL2AnchorV1),
}

impl BitsTxPayloadV1 {
    /// Returns the canonical 1-byte opcode discriminant (`0x01..=0x06`).
    pub const fn opcode(&self) -> u8 {
        match self {
            Self::Transfer(_) => 0x01,
            Self::PurchaseSplit(_) => 0x02,
            Self::TestnetGrant(_) => 0x03,
            Self::SimulatedPayout(_) => 0x04,
            Self::IdentityBind(_) => 0x05,
            Self::L2Anchor(_) => 0x06,
        }
    }

    /// Encodes this payload variant into canonical big-endian bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, BitsEconomy1Error> {
        match self {
            Self::Transfer(v) => Ok(v.encode_canonical()?.to_vec()),
            Self::PurchaseSplit(v) => Ok(v.encode_canonical()?.to_vec()),
            Self::TestnetGrant(v) => Ok(v.encode_canonical()?.to_vec()),
            Self::SimulatedPayout(v) => Ok(v.encode_canonical()?.to_vec()),
            Self::IdentityBind(v) => Ok(v.encode_canonical().to_vec()),
            Self::L2Anchor(v) => Ok(v.encode_canonical().to_vec()),
        }
    }

    /// Decodes a canonical payload variant from `(opcode, bytes)`.
    pub fn decode_canonical(opcode: u8, bytes: &[u8]) -> Result<Self, BitsEconomy1Error> {
        match opcode {
            0x01 => Ok(Self::Transfer(BitsTransferV1::decode_canonical(bytes)?)),
            0x02 => Ok(Self::PurchaseSplit(BitsPurchaseSplitV1::decode_canonical(
                bytes,
            )?)),
            0x03 => Ok(Self::TestnetGrant(BitsTestnetGrantV1::decode_canonical(
                bytes,
            )?)),
            0x04 => Ok(Self::SimulatedPayout(
                BitsSimulatedPayoutV1::decode_canonical(bytes)?,
            )),
            0x05 => Ok(Self::IdentityBind(BitsIdentityBindV1::decode_canonical(
                bytes,
            )?)),
            0x06 => Ok(Self::L2Anchor(BitsL2AnchorV1::decode_canonical(bytes)?)),
            _ => Err(BitsEconomy1Error::InvalidWire(
                "unknown BitsTxPayloadV1 opcode",
            )),
        }
    }
}

/// Signed canonical Bits transaction envelope verified via strict RFC 8032 Ed25519.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BitsSignedTxV1 {
    /// Protocol version (`1`).
    pub version: u16,
    /// Chain identifier bound in signing preimage.
    pub chain_id: u64,
    /// 32-byte genesis digest from [`BitsGenesisConfigV1::genesis_id`].
    pub genesis_id: [u8; 32],
    /// Principal [`ActorIdV1`] whose balance/nonce is charged.
    pub sender_actor: ActorIdV1,
    /// Ed25519 public key that produced `signature` (principal's active key or depth-1 delegate).
    pub signer_pubkey: CanonicalEd25519PublicKeyV1,
    /// Strict sequential actor nonce (`expected_nonce + 1`, starting at `1`).
    pub seq: u64,
    /// Inclusive maximum block height at which this transaction may execute (`0` = unbounded).
    pub valid_until_height: u64,
    /// Canonical typed Bits operation payload.
    pub payload: BitsTxPayloadV1,
    /// Strict RFC 8032 Ed25519 signature over [`Self::signing_preimage`].
    pub signature: CanonicalEd25519SignatureV1,
}

impl BitsSignedTxV1 {
    /// Encodes the unsigned header + payload bytes hashed into [`Self::signing_preimage`].
    pub fn encode_unsigned_canonical(&self) -> Result<Vec<u8>, BitsEconomy1Error> {
        if self.version != BITS_1_VERSION {
            return Err(BitsEconomy1Error::InvalidWire("tx version must equal 1"));
        }
        let payload_bytes = self.payload.encode_canonical()?;
        let mut out =
            Vec::with_capacity(2 + 8 + 32 + 32 + 32 + 8 + 8 + 1 + 2 + payload_bytes.len());
        out.extend_from_slice(&self.version.to_be_bytes());
        out.extend_from_slice(&self.chain_id.to_be_bytes());
        out.extend_from_slice(&self.genesis_id);
        out.extend_from_slice(self.sender_actor.as_bytes());
        out.extend_from_slice(self.signer_pubkey.as_bytes());
        out.extend_from_slice(&self.seq.to_be_bytes());
        out.extend_from_slice(&self.valid_until_height.to_be_bytes());
        out.push(self.payload.opcode());
        out.extend_from_slice(&(payload_bytes.len() as u16).to_be_bytes());
        out.extend_from_slice(&payload_bytes);
        Ok(out)
    }

    /// Builds the canonical `CRYPTO-D1` [`SigningPreimageV1`] signed by `signer_pubkey`.
    pub fn signing_preimage(&self) -> Result<SigningPreimageV1, BitsEconomy1Error> {
        let unsigned = self.encode_unsigned_canonical()?;
        let mut h = Sha256::new();
        h.update(SIGNED_TX_PAYLOAD_PREFIX);
        h.update(&unsigned);
        let payload_digest: [u8; 32] = h.finalize().into();
        Ok(SigningPreimageV1 {
            protocol_version: self.version,
            chain_id: self.chain_id,
            genesis_id: self.genesis_id,
            launch_manifest_id: [0u8; 32],
            profile_id: self.genesis_id,
            domain_tag: SigningDomainTagV1::ActorTransaction,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            payload_digest,
        })
    }

    /// Returns the 32-byte digest signed directly by Ed25519 (Rust, Python, and browser WebCrypto).
    pub fn signing_digest(&self) -> Result<[u8; 32], BitsEconomy1Error> {
        Ok(self.signing_preimage()?.signing_digest())
    }

    /// Encodes the complete signed transaction into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, BitsEconomy1Error> {
        let mut out = self.encode_unsigned_canonical()?;
        out.extend_from_slice(self.signature.as_bytes());
        Ok(out)
    }

    /// Decodes a complete signed transaction from canonical big-endian wire bytes.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, BitsEconomy1Error> {
        const HEADER_LEN: usize = 2 + 8 + 32 + 32 + 32 + 8 + 8 + 1 + 2;
        if bytes.len() < HEADER_LEN + 64 {
            return Err(BitsEconomy1Error::InvalidWire("BitsSignedTxV1 truncated"));
        }
        let version = u16::from_be_bytes([bytes[0], bytes[1]]);
        let chain_id = u64::from_be_bytes(bytes[2..10].try_into().expect("8"));
        let genesis_id: [u8; 32] = bytes[10..42].try_into().expect("32");
        let sender_actor = ActorIdV1::from_bytes(bytes[42..74].try_into().expect("32"));
        let signer_pubkey = CanonicalEd25519PublicKeyV1::from_slice(&bytes[74..106])?;
        let seq = u64::from_be_bytes(bytes[106..114].try_into().expect("8"));
        let valid_until_height = u64::from_be_bytes(bytes[114..122].try_into().expect("8"));
        let opcode = bytes[122];
        let payload_len = u16::from_be_bytes([bytes[123], bytes[124]]) as usize;
        if bytes.len() != HEADER_LEN + payload_len + 64 {
            return Err(BitsEconomy1Error::InvalidWire(
                "BitsSignedTxV1 length mismatch or trailing bytes",
            ));
        }
        let payload = BitsTxPayloadV1::decode_canonical(
            opcode,
            &bytes[HEADER_LEN..HEADER_LEN + payload_len],
        )?;
        let signature = CanonicalEd25519SignatureV1::from_slice(
            &bytes[HEADER_LEN + payload_len..HEADER_LEN + payload_len + 64],
        )?;
        Ok(Self {
            version,
            chain_id,
            genesis_id,
            sender_actor,
            signer_pubkey,
            seq,
            valid_until_height,
            payload,
            signature,
        })
    }

    /// Returns the deterministic 32-byte canonical transaction hash.
    pub fn tx_hash(&self) -> Result<[u8; 32], BitsEconomy1Error> {
        let wire = self.encode_canonical()?;
        let mut h = Sha256::new();
        h.update(b"bb.bits1.tx_hash.v1");
        h.update(&wire);
        Ok(h.finalize().into())
    }

    /// Signs a transaction deterministically using a 32-byte Ed25519 secret seed (tests/relayer).
    pub fn sign_with_seed(
        chain_id: u64,
        genesis_id: [u8; 32],
        sender_actor: ActorIdV1,
        signer_seed: &[u8; 32],
        seq: u64,
        valid_until_height: u64,
        payload: BitsTxPayloadV1,
    ) -> Result<Self, BitsEconomy1Error> {
        let signer_pubkey = CanonicalEd25519PublicKeyV1::from_secret_seed(signer_seed)?;
        let mut tx = Self {
            version: BITS_1_VERSION,
            chain_id,
            genesis_id,
            sender_actor,
            signer_pubkey,
            seq,
            valid_until_height,
            payload,
            signature: CanonicalEd25519SignatureV1::from_bytes([0u8; 64]).unwrap_or_else(|_| {
                let dummy = SigningPreimageV1 {
                    protocol_version: BITS_1_VERSION,
                    chain_id,
                    genesis_id,
                    launch_manifest_id: [0u8; 32],
                    profile_id: genesis_id,
                    domain_tag: SigningDomainTagV1::ActorTransaction,
                    signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
                    hash_suite: HashSuiteIdV1::Sha256V1,
                    payload_digest: [0u8; 32],
                };
                sign_preimage_deterministic(signer_seed, &dummy).expect("valid seed")
            }),
        };
        let preimage = tx.signing_preimage()?;
        tx.signature = sign_preimage_deterministic(signer_seed, &preimage)?;
        Ok(tx)
    }

    /// Strictly verifies the Ed25519 signature against `self.signer_pubkey` and signing preimage.
    pub fn verify_signature(&self) -> Result<(), BitsEconomy1Error> {
        let preimage = self.signing_preimage()?;
        verify_preimage_strict(&self.signer_pubkey, &preimage, &self.signature)?;
        Ok(())
    }
}

/// Optional non-transitive depth-1 scoped delegation granting a relayer key bounded
/// `BitsPurchaseSplitV1` authority without holding the player's private key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BitsDelegationGrantV1 {
    /// Principal actor granting bounded checkout capability.
    pub principal_actor: ActorIdV1,
    /// Direct delegate Ed25519 public key (non-transitive; cannot sub-delegate).
    pub delegate_pubkey: CanonicalEd25519PublicKeyV1,
    /// Maximum Bits allowed in any single delegated [`BitsPurchaseSplitV1`].
    pub max_bits_per_tx: u128,
    /// Cumulative Bits cap across all delegated purchases under this grant.
    pub max_bits_total: u128,
    /// Cumulative Bits already spent under this grant.
    pub spent_bits: u128,
    /// Inclusive maximum block height at which this delegation remains active.
    pub expires_at_height: u64,
    /// Explicitly enables `BitsPurchaseSplitV1` delegation (never `Transfer` or `SimulatedPayout`).
    pub allow_purchase_split: bool,
    /// Set `true` on revocation or key rotation.
    pub revoked: bool,
}

/// Authenticated state transition receipt emitted for every committed Bits transaction (`236` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BitsStateReceiptV1 {
    /// Schema version (`1`).
    pub version: u16,
    /// Committed block height.
    pub block_height: u64,
    /// Canonical 32-byte transaction hash.
    pub tx_hash: [u8; 32],
    /// Executed operation opcode (`0x01..=0x06`).
    pub opcode: u8,
    /// Deterministic execution disposition.
    pub disposition: BitsDispositionV1,
    /// Principal [`ActorIdV1`] associated with the transaction.
    pub actor_id: ActorIdV1,
    /// Post-transition `DomainIdV1::NativeBalance` (`3`) Patricia root.
    pub balance_root: [u8; 32],
    /// Post-transition `DomainIdV1::Nonce` (`2`) Patricia root.
    pub nonce_root: [u8; 32],
    /// Remaining Bits in `TestnetGrantPool`.
    pub grant_pool_remaining: u128,
    /// Cumulative Bits held by `platform_treasury_actor`.
    pub treasury_balance: u128,
    /// Cumulative testnet-only Bits held in `payout_escrow_actor`.
    pub simulated_payout_escrow: u128,
    /// Fixed immutable `genesis_supply` conserved across every transition.
    pub genesis_supply: u128,
    /// Creator share credited on `PurchaseSplit` (`0` for non-split ops).
    pub creator_bits_paid: u128,
    /// Platform treasury share credited on `PurchaseSplit` (`0` for non-split ops).
    pub platform_bits_paid: u128,
}

impl BitsStateReceiptV1 {
    /// Exact canonical big-endian wire byte length (`236` bytes).
    pub const WIRE_LEN: usize = 2 + 8 + 32 + 1 + 1 + 32 + 32 + 32 + 16 + 16 + 16 + 16 + 16 + 16;

    /// Encodes this receipt into 236 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], BitsEconomy1Error> {
        if self.version != BITS_1_VERSION {
            return Err(BitsEconomy1Error::InvalidWire(
                "receipt version must equal 1",
            ));
        }
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.version.to_be_bytes());
        out[2..10].copy_from_slice(&self.block_height.to_be_bytes());
        out[10..42].copy_from_slice(&self.tx_hash);
        out[42] = self.opcode;
        out[43] = self.disposition.code();
        out[44..76].copy_from_slice(self.actor_id.as_bytes());
        out[76..108].copy_from_slice(&self.balance_root);
        out[108..140].copy_from_slice(&self.nonce_root);
        out[140..156].copy_from_slice(&self.grant_pool_remaining.to_be_bytes());
        out[156..172].copy_from_slice(&self.treasury_balance.to_be_bytes());
        out[172..188].copy_from_slice(&self.simulated_payout_escrow.to_be_bytes());
        out[188..204].copy_from_slice(&self.genesis_supply.to_be_bytes());
        out[204..220].copy_from_slice(&self.creator_bits_paid.to_be_bytes());
        out[220..236].copy_from_slice(&self.platform_bits_paid.to_be_bytes());
        Ok(out)
    }

    /// Decodes a 236-byte canonical [`BitsStateReceiptV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, BitsEconomy1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(BitsEconomy1Error::InvalidWire(
                "BitsStateReceiptV1 must be 236 bytes",
            ));
        }
        let version = u16::from_be_bytes([bytes[0], bytes[1]]);
        if version != BITS_1_VERSION {
            return Err(BitsEconomy1Error::InvalidWire(
                "receipt version must equal 1",
            ));
        }
        Ok(Self {
            version,
            block_height: u64::from_be_bytes(bytes[2..10].try_into().expect("8")),
            tx_hash: bytes[10..42].try_into().expect("32"),
            opcode: bytes[42],
            disposition: BitsDispositionV1::from_code(bytes[43])?,
            actor_id: ActorIdV1::from_bytes(bytes[44..76].try_into().expect("32")),
            balance_root: bytes[76..108].try_into().expect("32"),
            nonce_root: bytes[108..140].try_into().expect("32"),
            grant_pool_remaining: u128::from_be_bytes(bytes[140..156].try_into().expect("16")),
            treasury_balance: u128::from_be_bytes(bytes[156..172].try_into().expect("16")),
            simulated_payout_escrow: u128::from_be_bytes(bytes[172..188].try_into().expect("16")),
            genesis_supply: u128::from_be_bytes(bytes[188..204].try_into().expect("16")),
            creator_bits_paid: u128::from_be_bytes(bytes[204..220].try_into().expect("16")),
            platform_bits_paid: u128::from_be_bytes(bytes[220..236].try_into().expect("16")),
        })
    }

    /// Computes the canonical `Receipt` [`ContentTag`] for this state receipt.
    pub fn tag(&self) -> Result<ContentTag, BitsEconomy1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            BITS_1_STATE_RECEIPT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Verified wallet proof payload returned for `GET /proof/wallet/{addr}` (Amendment 1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BitsWalletProofV1 {
    /// Permanent canonical [`ActorIdV1`] (stays fixed across key rotations).
    pub actor_id: ActorIdV1,
    /// Current active Ed25519 public key bound to `actor_id`, if known.
    pub active_pubkey: Option<CanonicalEd25519PublicKeyV1>,
    /// Committed liquid Bits balance (`0` if absent from trie).
    pub balance_bits: u128,
    /// Committed sequential actor nonce (`0` if never transacted).
    pub seq: u64,
    /// Authenticated `DomainIdV1::NativeBalance` (`3`) root.
    pub balance_root: [u8; 32],
    /// Canonical `DomainPatriciaProofV1` inclusion or non-membership witness.
    pub patricia_proof: DomainPatriciaProofV1,
}

/// Deterministic, zero-float, authenticated Bits economy state engine over `AUTH-COMMIT-1` tries.
#[derive(Clone, Debug)]
pub struct BitsEconomyStateV1 {
    config: BitsGenesisConfigV1,
    genesis_id: [u8; 32],
    balance_trie: DomainPatriciaTrieV1,
    nonce_trie: DomainPatriciaTrieV1,
    balances: BTreeMap<ActorIdV1, u128>,
    nonces: BTreeMap<ActorIdV1, u64>,
    actor_active_key: BTreeMap<ActorIdV1, CanonicalEd25519PublicKeyV1>,
    pubkey_to_actor: BTreeMap<CanonicalEd25519PublicKeyV1, ActorIdV1>,
    external_to_actor: BTreeMap<(BitsIdentityProviderV1, [u8; 32]), ActorIdV1>,
    delegations: BTreeMap<(ActorIdV1, CanonicalEd25519PublicKeyV1), BitsDelegationGrantV1>,
    grant_relayers: BTreeSet<CanonicalEd25519PublicKeyV1>,
    used_order_ids: BTreeSet<[u8; 32]>,
    used_grant_ids: BTreeSet<[u8; 32]>,
    used_payout_ids: BTreeSet<[u8; 32]>,
    l2_anchors: BTreeMap<[u8; 32], (u64, [u8; 32], u32)>,
}

impl BitsEconomyStateV1 {
    /// Initializes a clean-slate Bits ledger state from `config` and authorized testnet grant
    /// relayer keys (funded strictly from `config.initial_grant_pool`; zero runtime minting).
    pub fn from_genesis(
        config: BitsGenesisConfigV1,
        initial_allocations: &[(ActorIdV1, CanonicalEd25519PublicKeyV1, u128)],
        grant_relayers: &[CanonicalEd25519PublicKeyV1],
    ) -> Result<Self, BitsEconomy1Error> {
        config.validate()?;
        let genesis_id = config.genesis_id()?;
        let mut st = Self {
            config,
            genesis_id,
            balance_trie: DomainPatriciaTrieV1::empty(DomainIdV1::NativeBalance),
            nonce_trie: DomainPatriciaTrieV1::empty(DomainIdV1::Nonce),
            balances: BTreeMap::new(),
            nonces: BTreeMap::new(),
            actor_active_key: BTreeMap::new(),
            pubkey_to_actor: BTreeMap::new(),
            external_to_actor: BTreeMap::new(),
            delegations: BTreeMap::new(),
            grant_relayers: grant_relayers.iter().copied().collect(),
            used_order_ids: BTreeSet::new(),
            used_grant_ids: BTreeSet::new(),
            used_payout_ids: BTreeSet::new(),
            l2_anchors: BTreeMap::new(),
        };

        let mut allocated_sum = config.initial_grant_pool;
        if config.initial_grant_pool > 0 {
            st.set_balance(config.grant_pool_actor, config.initial_grant_pool);
        }
        for &(actor, pk, amt) in initial_allocations {
            if actor == config.grant_pool_actor
                || actor == config.platform_treasury_actor
                || actor == config.payout_escrow_actor
            {
                return Err(BitsEconomy1Error::InvalidGenesis(
                    "initial allocation cannot target a protocol system actor",
                ));
            }
            if st.actor_active_key.contains_key(&actor) || st.pubkey_to_actor.contains_key(&pk) {
                return Err(BitsEconomy1Error::InvalidGenesis(
                    "duplicate actor or pubkey in initial allocations",
                ));
            }
            allocated_sum =
                allocated_sum
                    .checked_add(amt)
                    .ok_or(BitsEconomy1Error::ConservationViolation(
                        "genesis supply sum overflow",
                    ))?;
            st.actor_active_key.insert(actor, pk);
            st.pubkey_to_actor.insert(pk, actor);
            if amt > 0 {
                st.set_balance(actor, amt);
            }
        }
        if allocated_sum != config.genesis_supply {
            return Err(BitsEconomy1Error::ConservationViolation(
                "initial_grant_pool + initial_allocations != genesis_supply",
            ));
        }
        st.verify_conservation()?;
        Ok(st)
    }

    /// Returns the immutable [`BitsGenesisConfigV1`].
    pub const fn config(&self) -> &BitsGenesisConfigV1 {
        &self.config
    }

    /// Returns the 32-byte genesis digest.
    pub const fn genesis_id(&self) -> [u8; 32] {
        self.genesis_id
    }

    /// Returns the current `DomainIdV1::NativeBalance` (`3`) Patricia root.
    pub fn balance_root(&self) -> [u8; 32] {
        self.balance_trie.root()
    }

    /// Returns the current `DomainIdV1::Nonce` (`2`) Patricia root.
    pub fn nonce_root(&self) -> [u8; 32] {
        self.nonce_trie.root()
    }

    /// Returns the liquid Bits balance of `actor_id`.
    pub fn balance_of(&self, actor_id: &ActorIdV1) -> u128 {
        self.balances.get(actor_id).copied().unwrap_or(0)
    }

    /// Returns the current sequential nonce of `actor_id`.
    pub fn nonce_of(&self, actor_id: &ActorIdV1) -> u64 {
        self.nonces.get(actor_id).copied().unwrap_or(0)
    }

    /// Returns the remaining balance of `TestnetGrantPool`.
    pub fn grant_pool_remaining(&self) -> u128 {
        self.balance_of(&self.config.grant_pool_actor)
    }

    /// Returns the cumulative platform treasury balance.
    pub fn treasury_balance(&self) -> u128 {
        self.balance_of(&self.config.platform_treasury_actor)
    }

    /// Returns the cumulative testnet-only simulated payout escrow balance.
    pub fn simulated_payout_escrow(&self) -> u128 {
        self.balance_of(&self.config.payout_escrow_actor)
    }

    /// Returns the number of committed non-zero balance accounts in the Patricia trie.
    pub fn account_count(&self) -> usize {
        self.balance_trie.len()
    }

    /// Resolves either a canonical [`ActorIdV1`] or a raw Ed25519 K1 public key alias to its
    /// canonical `(ActorIdV1, Option<active_pubkey>)` pair (Amendment 1).
    pub fn resolve_address_or_pubkey(
        &self,
        raw_32: [u8; 32],
    ) -> Result<(ActorIdV1, Option<CanonicalEd25519PublicKeyV1>), BitsEconomy1Error> {
        let as_actor = ActorIdV1::from_bytes(raw_32);
        if self.actor_active_key.contains_key(&as_actor) || self.balances.contains_key(&as_actor) {
            return Ok((as_actor, self.actor_active_key.get(&as_actor).copied()));
        }
        if let Ok(pk) = CanonicalEd25519PublicKeyV1::from_bytes(raw_32) {
            if let Some(&bound_actor) = self.pubkey_to_actor.get(&pk) {
                return Ok((bound_actor, Some(pk)));
            }
            let derived = derive_default_bits_actor_id(self.config.chain_id, &pk)?;
            return Ok((derived, Some(pk)));
        }
        Ok((as_actor, None))
    }

    /// Generates an authenticated [`BitsWalletProofV1`] (`DomainPatriciaProofV1` inclusion or
    /// non-membership witness) accepting either `ActorIdV1` or raw Ed25519 K1 pubkey (Amendment 1).
    pub fn prove_wallet(&self, raw_32: [u8; 32]) -> Result<BitsWalletProofV1, BitsEconomy1Error> {
        let (actor_id, active_pubkey) = self.resolve_address_or_pubkey(raw_32)?;
        let proof = self.balance_trie.prove(actor_id.as_bytes())?;
        Ok(BitsWalletProofV1 {
            actor_id,
            active_pubkey,
            balance_bits: self.balance_of(&actor_id),
            seq: self.nonce_of(&actor_id),
            balance_root: self.balance_root(),
            patricia_proof: proof,
        })
    }

    /// Registers a direct non-transitive depth-1 scoped delegation (`PurchaseSplit` only).
    pub fn register_purchase_delegation(
        &mut self,
        grant: BitsDelegationGrantV1,
    ) -> Result<(), BitsEconomy1Error> {
        if !grant.allow_purchase_split
            || grant.revoked
            || grant.max_bits_per_tx == 0
            || grant.max_bits_total == 0
            || grant.max_bits_per_tx > grant.max_bits_total
            || grant.spent_bits != 0
        {
            return Err(BitsEconomy1Error::PolicyViolation(
                "invalid purchase delegation grant bounds",
            ));
        }
        // Reject depth-2 transitive delegation: a delegate key that is itself a delegate of another
        // actor cannot grant sub-delegations as a principal.
        if let Some(&principal_pk) = self.actor_active_key.get(&grant.principal_actor)
            && self
                .delegations
                .keys()
                .any(|(_, del_pk)| *del_pk == principal_pk)
        {
            return Err(BitsEconomy1Error::Unauthorized(
                "transitive depth-2 delegation is forbidden",
            ));
        }
        self.delegations
            .insert((grant.principal_actor, grant.delegate_pubkey), grant);
        Ok(())
    }

    /// Verifies exact closed Bits conservation across every account and system pool.
    pub fn verify_conservation(&self) -> Result<(), BitsEconomy1Error> {
        let mut total = 0u128;
        for &bal in self.balances.values() {
            total = total
                .checked_add(bal)
                .ok_or(BitsEconomy1Error::ConservationViolation(
                    "balance sum overflow",
                ))?;
        }
        if total != self.config.genesis_supply {
            return Err(BitsEconomy1Error::ConservationViolation(
                "sum(balances) != genesis_supply",
            ));
        }
        self.balance_trie.verify_rebuild_parity()?;
        self.nonce_trie.verify_rebuild_parity()?;
        Ok(())
    }

    /// Applies one signed Bits transaction atomically at `block_height`, rolling back completely on
    /// any verification, authorization, replay, balance, or conservation error.
    pub fn apply_signed_tx(
        &mut self,
        tx: &BitsSignedTxV1,
        block_height: u64,
    ) -> Result<BitsStateReceiptV1, BitsEconomy1Error> {
        if tx.chain_id != self.config.chain_id || tx.genesis_id != self.genesis_id {
            return Err(BitsEconomy1Error::Unauthorized(
                "chain_id or genesis_id mismatch",
            ));
        }
        if tx.valid_until_height != 0 && block_height > tx.valid_until_height {
            return Err(BitsEconomy1Error::ReplayOrNonce(
                "transaction valid_until_height expired",
            ));
        }
        tx.verify_signature()?;
        let expected_seq = self
            .nonce_of(&tx.sender_actor)
            .checked_add(1)
            .ok_or(BitsEconomy1Error::ReplayOrNonce("nonce overflow"))?;
        if tx.seq != expected_seq {
            return Err(BitsEconomy1Error::ReplayOrNonce(
                "sequential actor nonce mismatch",
            ));
        }

        let tx_hash = tx.tx_hash()?;
        let mut next = self.clone();
        let (creator_bits_paid, platform_bits_paid) =
            next.execute_payload_inner(tx, block_height)?;
        next.set_nonce(tx.sender_actor, tx.seq);
        next.verify_conservation()?;
        *self = next;

        Ok(BitsStateReceiptV1 {
            version: BITS_1_VERSION,
            block_height,
            tx_hash,
            opcode: tx.payload.opcode(),
            disposition: BitsDispositionV1::Admitted,
            actor_id: tx.sender_actor,
            balance_root: self.balance_root(),
            nonce_root: self.nonce_root(),
            grant_pool_remaining: self.grant_pool_remaining(),
            treasury_balance: self.treasury_balance(),
            simulated_payout_escrow: self.simulated_payout_escrow(),
            genesis_supply: self.config.genesis_supply,
            creator_bits_paid,
            platform_bits_paid,
        })
    }

    fn execute_payload_inner(
        &mut self,
        tx: &BitsSignedTxV1,
        block_height: u64,
    ) -> Result<(u128, u128), BitsEconomy1Error> {
        match tx.payload {
            BitsTxPayloadV1::Transfer(ref op) => {
                if op.from != tx.sender_actor {
                    return Err(BitsEconomy1Error::Unauthorized(
                        "transfer sender_actor must match from",
                    ));
                }
                self.require_principal_key(&op.from, &tx.signer_pubkey)?;
                self.transfer_exact(op.from, op.to, op.amount_bits)?;
                Ok((0, 0))
            }
            BitsTxPayloadV1::PurchaseSplit(ref op) => {
                if op.buyer != tx.sender_actor {
                    return Err(BitsEconomy1Error::Unauthorized(
                        "purchase buyer must match sender_actor",
                    ));
                }
                if op.treasury != self.config.platform_treasury_actor {
                    return Err(BitsEconomy1Error::PolicyViolation(
                        "purchase treasury does not match genesis platform_treasury_actor",
                    ));
                }
                if op.amount_bits < self.config.min_purchase_bits {
                    return Err(BitsEconomy1Error::PolicyViolation(
                        "purchase amount_bits below min_purchase_bits",
                    ));
                }
                if op.creator_bps != self.config.creator_split_bps {
                    return Err(BitsEconomy1Error::PolicyViolation(
                        "purchase creator_bps does not match genesis creator_split_bps",
                    ));
                }
                if self.used_order_ids.contains(&op.order_id) {
                    return Err(BitsEconomy1Error::ReplayOrNonce(
                        "duplicate purchase order_id",
                    ));
                }
                self.authorize_purchase_signer(
                    &op.buyer,
                    &tx.signer_pubkey,
                    op.amount_bits,
                    block_height,
                )?;
                let (creator_bits, platform_bits) =
                    compute_purchase_split(op.amount_bits, op.creator_bps)?;
                let buyer_bal = self.balance_of(&op.buyer);
                if buyer_bal < op.amount_bits {
                    return Err(BitsEconomy1Error::InsufficientBalance(
                        "buyer lacks sufficient Bits for purchase split",
                    ));
                }
                self.set_balance(op.buyer, buyer_bal - op.amount_bits);
                self.credit_balance(op.creator, creator_bits)?;
                self.credit_balance(op.treasury, platform_bits)?;
                self.used_order_ids.insert(op.order_id);
                Ok((creator_bits, platform_bits))
            }
            BitsTxPayloadV1::TestnetGrant(ref op) => {
                if !self.config.lab {
                    return Err(BitsEconomy1Error::TestnetOnlyDisabled(
                        "BitsTestnetGrantV1 is structurally disabled when lab == false",
                    ));
                }
                if tx.sender_actor != self.config.grant_pool_actor {
                    return Err(BitsEconomy1Error::Unauthorized(
                        "testnet grant sender_actor must be grant_pool_actor",
                    ));
                }
                if !self.grant_relayers.contains(&tx.signer_pubkey) {
                    return Err(BitsEconomy1Error::Unauthorized(
                        "signer is not an authorized testnet grant relayer",
                    ));
                }
                if self.used_grant_ids.contains(&op.grant_id) {
                    return Err(BitsEconomy1Error::ReplayOrNonce("duplicate grant_id"));
                }
                match op.grant_kind {
                    BitsGrantKindV1::Signup => {
                        if op.amount_bits > self.config.max_signup_grant_bits {
                            return Err(BitsEconomy1Error::PolicyViolation(
                                "signup grant exceeds max_signup_grant_bits",
                            ));
                        }
                    }
                    BitsGrantKindV1::Faucet => {
                        if op.amount_bits > self.config.max_faucet_grant_bits {
                            return Err(BitsEconomy1Error::PolicyViolation(
                                "faucet grant exceeds max_faucet_grant_bits",
                            ));
                        }
                    }
                    BitsGrantKindV1::CarryOver => {}
                }
                // Bind default ActorIdV1 -> pubkey if this is a brand-new account.
                if !self.actor_active_key.contains_key(&op.recipient) {
                    let expected_actor =
                        derive_default_bits_actor_id(self.config.chain_id, &op.recipient_pubkey)?;
                    if expected_actor != op.recipient {
                        return Err(BitsEconomy1Error::PolicyViolation(
                            "unbound recipient actor_id must equal derive_default_bits_actor_id",
                        ));
                    }
                    if self.pubkey_to_actor.contains_key(&op.recipient_pubkey) {
                        return Err(BitsEconomy1Error::PolicyViolation(
                            "recipient_pubkey already bound to a different actor",
                        ));
                    }
                    self.actor_active_key
                        .insert(op.recipient, op.recipient_pubkey);
                    self.pubkey_to_actor
                        .insert(op.recipient_pubkey, op.recipient);
                }
                self.transfer_exact(self.config.grant_pool_actor, op.recipient, op.amount_bits)?;
                self.used_grant_ids.insert(op.grant_id);
                Ok((0, 0))
            }
            BitsTxPayloadV1::SimulatedPayout(ref op) => {
                if !self.config.lab {
                    return Err(BitsEconomy1Error::TestnetOnlyDisabled(
                        "BitsSimulatedPayoutV1 is structurally disabled when lab == false",
                    ));
                }
                if op.creator != tx.sender_actor {
                    return Err(BitsEconomy1Error::Unauthorized(
                        "simulated payout creator must match sender_actor",
                    ));
                }
                if op.amount_bits < self.config.min_payout_bits {
                    return Err(BitsEconomy1Error::PolicyViolation(
                        "simulated payout below min_payout_bits",
                    ));
                }
                if self.used_payout_ids.contains(&op.payout_request_id) {
                    return Err(BitsEconomy1Error::ReplayOrNonce(
                        "duplicate payout_request_id",
                    ));
                }
                self.require_principal_key(&op.creator, &tx.signer_pubkey)?;
                self.transfer_exact(op.creator, self.config.payout_escrow_actor, op.amount_bits)?;
                self.used_payout_ids.insert(op.payout_request_id);
                Ok((0, 0))
            }
            BitsTxPayloadV1::IdentityBind(ref op) => {
                if op.actor_id != tx.sender_actor {
                    return Err(BitsEconomy1Error::Unauthorized(
                        "identity bind actor_id must match sender_actor",
                    ));
                }
                // Verify strict proof-of-possession by the new/bound Ed25519 key.
                let pop_msg = identity_bind_pop_message(
                    self.config.chain_id,
                    &self.genesis_id,
                    &op.actor_id,
                    &op.ed25519_pk,
                    op.provider,
                    &op.external_id_hash,
                );
                verify_raw_message_strict(&op.ed25519_pk, &pop_msg, &op.pop_signature)?;

                // Either initial self-bind for derived ActorIdV1, or key rotation signed by current active key.
                if let Some(&current_pk) = self.actor_active_key.get(&op.actor_id) {
                    if tx.signer_pubkey != current_pk {
                        return Err(BitsEconomy1Error::Unauthorized(
                            "identity rotation must be signed by actor's current active key",
                        ));
                    }
                    if op.ed25519_pk != current_pk {
                        if let Some(&other_actor) = self.pubkey_to_actor.get(&op.ed25519_pk)
                            && other_actor != op.actor_id
                        {
                            return Err(BitsEconomy1Error::PolicyViolation(
                                "new ed25519_pk is already bound to another actor",
                            ));
                        }
                        self.pubkey_to_actor.remove(&current_pk);
                        self.actor_active_key.insert(op.actor_id, op.ed25519_pk);
                        self.pubkey_to_actor.insert(op.ed25519_pk, op.actor_id);
                        // Revoke all stale delegations on key rotation.
                        for ((principal, _), grant) in self.delegations.iter_mut() {
                            if *principal == op.actor_id {
                                grant.revoked = true;
                            }
                        }
                    }
                } else {
                    let expected =
                        derive_default_bits_actor_id(self.config.chain_id, &op.ed25519_pk)?;
                    if expected != op.actor_id || tx.signer_pubkey != op.ed25519_pk {
                        return Err(BitsEconomy1Error::Unauthorized(
                            "initial identity bind must match derived ActorIdV1 and be self-signed",
                        ));
                    }
                    if self.pubkey_to_actor.contains_key(&op.ed25519_pk) {
                        return Err(BitsEconomy1Error::PolicyViolation(
                            "ed25519_pk already bound to another actor",
                        ));
                    }
                    self.actor_active_key.insert(op.actor_id, op.ed25519_pk);
                    self.pubkey_to_actor.insert(op.ed25519_pk, op.actor_id);
                }

                if let Some(&existing_owner) = self
                    .external_to_actor
                    .get(&(op.provider, op.external_id_hash))
                    && existing_owner != op.actor_id
                {
                    return Err(BitsEconomy1Error::PolicyViolation(
                        "external identity already bound to another actor",
                    ));
                }
                self.external_to_actor
                    .insert((op.provider, op.external_id_hash), op.actor_id);
                Ok((0, 0))
            }
            BitsTxPayloadV1::L2Anchor(ref op) => {
                if !self.grant_relayers.contains(&tx.signer_pubkey) {
                    return Err(BitsEconomy1Error::Unauthorized(
                        "L2Anchor requires authorized relayer signer",
                    ));
                }
                if let Some(&(prev_epoch, _, _)) = self.l2_anchors.get(&op.app_id_hash)
                    && op.epoch_id <= prev_epoch
                {
                    return Err(BitsEconomy1Error::ReplayOrNonce(
                        "L2Anchor epoch_id must strictly increase",
                    ));
                }
                self.l2_anchors
                    .insert(op.app_id_hash, (op.epoch_id, op.l2_root, op.event_count));
                Ok((0, 0))
            }
        }
    }

    fn require_principal_key(
        &self,
        actor: &ActorIdV1,
        signer_pk: &CanonicalEd25519PublicKeyV1,
    ) -> Result<(), BitsEconomy1Error> {
        let Some(&active_pk) = self.actor_active_key.get(actor) else {
            return Err(BitsEconomy1Error::Unauthorized(
                "actor has no bound active Ed25519 key",
            ));
        };
        if active_pk != *signer_pk {
            return Err(BitsEconomy1Error::Unauthorized(
                "signer key does not match actor's active key",
            ));
        }
        Ok(())
    }

    fn authorize_purchase_signer(
        &mut self,
        buyer: &ActorIdV1,
        signer_pk: &CanonicalEd25519PublicKeyV1,
        amount_bits: u128,
        block_height: u64,
    ) -> Result<(), BitsEconomy1Error> {
        if self.actor_active_key.get(buyer) == Some(signer_pk) {
            return Ok(());
        }
        let Some(grant) = self.delegations.get_mut(&(*buyer, *signer_pk)) else {
            return Err(BitsEconomy1Error::Unauthorized(
                "purchase signer is neither buyer active key nor authorized delegate",
            ));
        };
        if grant.revoked || !grant.allow_purchase_split {
            return Err(BitsEconomy1Error::Unauthorized(
                "delegation grant revoked or lacks purchase scope",
            ));
        }
        if block_height > grant.expires_at_height {
            return Err(BitsEconomy1Error::Unauthorized("delegation grant expired"));
        }
        if amount_bits > grant.max_bits_per_tx {
            return Err(BitsEconomy1Error::Unauthorized(
                "purchase exceeds delegation max_bits_per_tx",
            ));
        }
        let next_spent = grant.spent_bits.checked_add(amount_bits).ok_or(
            BitsEconomy1Error::ConservationViolation("delegation spent_bits overflow"),
        )?;
        if next_spent > grant.max_bits_total {
            return Err(BitsEconomy1Error::Unauthorized(
                "purchase exceeds delegation max_bits_total",
            ));
        }
        grant.spent_bits = next_spent;
        Ok(())
    }

    fn transfer_exact(
        &mut self,
        from: ActorIdV1,
        to: ActorIdV1,
        amount_bits: u128,
    ) -> Result<(), BitsEconomy1Error> {
        let from_bal = self.balance_of(&from);
        if from_bal < amount_bits {
            return Err(BitsEconomy1Error::InsufficientBalance(
                "insufficient liquid Bits balance",
            ));
        }
        self.set_balance(from, from_bal - amount_bits);
        self.credit_balance(to, amount_bits)?;
        Ok(())
    }

    fn credit_balance(
        &mut self,
        actor: ActorIdV1,
        amount_bits: u128,
    ) -> Result<(), BitsEconomy1Error> {
        if amount_bits == 0 {
            return Ok(());
        }
        let cur = self.balance_of(&actor);
        let next = cur
            .checked_add(amount_bits)
            .ok_or(BitsEconomy1Error::ConservationViolation(
                "credit balance overflow",
            ))?;
        self.set_balance(actor, next);
        Ok(())
    }

    fn set_balance(&mut self, actor: ActorIdV1, amount: u128) {
        if amount == 0 {
            self.balances.remove(&actor);
            let _ = self.balance_trie.delete(actor.as_bytes());
        } else {
            self.balances.insert(actor, amount);
            self.balance_trie
                .upsert(*actor.as_bytes(), amount.to_be_bytes().to_vec());
        }
    }

    fn set_nonce(&mut self, actor: ActorIdV1, seq: u64) {
        self.nonces.insert(actor, seq);
        self.nonce_trie
            .upsert(*actor.as_bytes(), seq.to_be_bytes().to_vec());
    }
}

/// Verifies a [`BitsWalletProofV1`] against an expected `balance_root` (`DomainIdV1::NativeBalance`),
/// returning the proven liquid Bits balance (`0` for proven non-membership).
pub fn verify_bits_wallet_proof(
    expected_balance_root: [u8; 32],
    proof: &BitsWalletProofV1,
) -> Result<u128, BitsEconomy1Error> {
    if proof.balance_root != expected_balance_root {
        return Err(BitsEconomy1Error::PolicyViolation(
            "wallet proof balance_root mismatch",
        ));
    }
    let val_opt = proof.patricia_proof.verify(
        DomainIdV1::NativeBalance,
        expected_balance_root,
        proof.actor_id.as_bytes(),
    )?;
    let proven_balance = match val_opt {
        None => 0u128,
        Some(bytes) => {
            let arr: [u8; 16] = bytes.as_slice().try_into().map_err(|_| {
                BitsEconomy1Error::InvalidWire("balance Patricia leaf must be 16 bytes")
            })?;
            u128::from_be_bytes(arr)
        }
    };
    if proven_balance != proof.balance_bits {
        return Err(BitsEconomy1Error::PolicyViolation(
            "proven Patricia balance disagrees with wallet proof balance_bits",
        ));
    }
    Ok(proven_balance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn seed(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn sign_raw_pop(sk_seed: &[u8; 32], msg: &[u8; 32]) -> CanonicalEd25519SignatureV1 {
        let sk = SigningKey::from_bytes(sk_seed);
        let sig = sk.sign(msg);
        CanonicalEd25519SignatureV1::from_bytes(sig.to_bytes()).expect("canonical sig")
    }

    #[test]
    fn bits_economy_1_canonical_wire_and_split_conservation() {
        let cfg = BitsGenesisConfigV1::canonical_testnet_default();
        let cfg_wire = cfg.encode_canonical().unwrap();
        assert_eq!(
            BitsGenesisConfigV1::decode_canonical(&cfg_wire).unwrap(),
            cfg
        );

        for price in [10u128, 11, 13, 99, 100, 250, 999, 100_000] {
            let (creator, platform) = compute_purchase_split(price, 8_000).unwrap();
            assert_eq!(creator + platform, price);
            assert_eq!(creator, (price * 8_000) / 10_000);
        }

        let pk1 = CanonicalEd25519PublicKeyV1::from_secret_seed(&seed(0x11)).unwrap();
        let pk2 = CanonicalEd25519PublicKeyV1::from_secret_seed(&seed(0x22)).unwrap();
        let relayer_pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&seed(0x99)).unwrap();
        let a1 = derive_default_bits_actor_id(cfg.chain_id, &pk1).unwrap();
        let a2 = derive_default_bits_actor_id(cfg.chain_id, &pk2).unwrap();

        let mut st = BitsEconomyStateV1::from_genesis(cfg, &[], &[relayer_pk]).unwrap();
        let gid = st.genesis_id();

        let grant_tx = BitsSignedTxV1::sign_with_seed(
            cfg.chain_id,
            gid,
            cfg.grant_pool_actor,
            &seed(0x99),
            1,
            10,
            BitsTxPayloadV1::TestnetGrant(BitsTestnetGrantV1 {
                recipient: a1,
                recipient_pubkey: pk1,
                amount_bits: 1_000,
                grant_kind: BitsGrantKindV1::Signup,
                grant_id: [0xA1; 32],
            }),
        )
        .unwrap();
        let rcpt = st.apply_signed_tx(&grant_tx, 1).unwrap();
        assert_eq!(rcpt.disposition, BitsDispositionV1::Admitted);
        assert_eq!(st.balance_of(&a1), 1_000);
        assert_eq!(st.grant_pool_remaining(), cfg.genesis_supply - 1_000);

        let order_id = [0xB2; 32];
        let memo = compute_purchase_memo_hash(&order_id, "aurora_forge_plaza", "pass_vip", &a1);
        let split_tx = BitsSignedTxV1::sign_with_seed(
            cfg.chain_id,
            gid,
            a1,
            &seed(0x11),
            1,
            10,
            BitsTxPayloadV1::PurchaseSplit(BitsPurchaseSplitV1 {
                buyer: a1,
                creator: a2,
                treasury: cfg.platform_treasury_actor,
                amount_bits: 250,
                creator_bps: 8_000,
                order_id,
                memo_hash: memo,
            }),
        )
        .unwrap();
        let rcpt2 = st.apply_signed_tx(&split_tx, 2).unwrap();
        assert_eq!(rcpt2.creator_bits_paid, 200);
        assert_eq!(rcpt2.platform_bits_paid, 50);
        assert_eq!(st.balance_of(&a1), 750);
        assert_eq!(st.balance_of(&a2), 200);
        assert_eq!(st.treasury_balance(), 50);

        // Verify Amendment 1 wallet proof by BOTH ActorIdV1 and raw Ed25519 pubkey!
        let wp_by_actor = st.prove_wallet(*a1.as_bytes()).unwrap();
        let wp_by_pk = st.prove_wallet(*pk1.as_bytes()).unwrap();
        assert_eq!(wp_by_actor.actor_id, a1);
        assert_eq!(wp_by_pk.actor_id, a1);
        assert_eq!(
            verify_bits_wallet_proof(st.balance_root(), &wp_by_pk).unwrap(),
            750
        );

        // Verify IdentityBind PoP roundtrip
        let pop_msg = identity_bind_pop_message(
            cfg.chain_id,
            &gid,
            &a1,
            &pk1,
            BitsIdentityProviderV1::Discord,
            &[0xCC; 32],
        );
        let bind_payload = BitsIdentityBindV1 {
            actor_id: a1,
            ed25519_pk: pk1,
            provider: BitsIdentityProviderV1::Discord,
            external_id_hash: [0xCC; 32],
            pop_signature: sign_raw_pop(&seed(0x11), &pop_msg),
        };
        assert_eq!(
            BitsIdentityBindV1::decode_canonical(&bind_payload.encode_canonical()).unwrap(),
            bind_payload
        );
    }
}
