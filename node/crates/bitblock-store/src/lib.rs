//! Crash-safe ACID `redb` storage and `SIGNER-OPS-1` kernel-locked validator signer boundary
//! for the merged Bitblock L1 Bits testnet daemon.
//!
//! Guarantees:
//! - Single atomic `redb` write transaction per finalized block (`blocks`, `tx_index`,
//!   `order_index`, `meta`) so block headers, QCs, and indexes never drift on crash.
//! - Cold reopen deterministically replays `BitsEconomyStateV1` from genesis through every
//!   persisted block and verifies `balance_root`, `nonce_root`, `tx_root`, and exact supply
//!   conservation at every height.
//! - Every validator proposal and HotStuff commit vote passes through Bitblock's real
//!   `SIGNER-OPS-1` [`PersistentValidatorSignerV1`] (`flock` / `share_mode(0)` exclusive OS lock
//!   plus atomic `watermark.tmp -> sync_all() -> rename(watermark.bin)` BEFORE releasing any
//!   Ed25519 signature bytes).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bitblock_light_wasm::{
    BitsBlockHeaderJson, BitsCommitteeMemberJson, BitsQuorumCertJson, compute_bits_header_hash,
};
use bitblock_research::bits_economy_1::{
    BITS_1_VERSION, BITS_DEFAULT_CREATOR_SPLIT_BPS, BITS_DEFAULT_FAUCET_GRANT,
    BITS_DEFAULT_MIN_PAYOUT, BITS_DEFAULT_MIN_PURCHASE, BITS_DEFAULT_SIGNUP_GRANT,
    BitsEconomyStateV1, BitsGenesisConfigV1, BitsSignedTxV1, BitsStateReceiptV1, BitsTxPayloadV1,
    derive_system_actor_id,
};
use bitblock_research::crypto_1::KeyLifecycleRegistryV1;
use bitblock_research::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, SigningDomainTagV1,
};
use bitblock_research::executor_1::ExecutorForkContextV1;
use bitblock_research::identity_d1::{ActorIdV1, CredentialRoleV1};
use bitblock_research::signer_ops_1::signer_io::PersistentValidatorSignerV1;
use bitblock_research::signer_ops_1::{
    SignerAuditReceiptV1, SignerPhaseCodeV1, SignerWatermarkV1, ValidatorKeyCeremonyBundleV1,
};
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// JS IEEE-754 safe chain ID (`0x4249_5453` = ASCII `"BITS"`, `< 2^53`) for browser/relayer JSON interop.
pub const BITS_TESTNET_CHAIN_ID: u64 = 1_112_101_971;
/// Default testnet genesis supply (`1,000,000,000` testnet Bits; strictly `lab: true`).
pub const BITS_TESTNET_GENESIS_SUPPLY: u64 = 1_000_000_000;
/// Explicit devnet/testnet custody boundary marker.
pub const BITS_TESTNET_CUSTODY_LABEL: &str = "DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY";

const BLOCKS_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("blocks");
const META_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");
const TX_INDEX_TABLE: TableDefinition<&[u8], u64> = TableDefinition::new("tx_index");
const ORDER_INDEX_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("order_index");

const VALIDATOR_SCHEDULE_PREFIX: &[u8] = b"bb.bits1.validator_schedule.v1";
const TX_ROOT_PREFIX: &[u8] = b"bb.bits1.tx_root.v1";
const QC_COMMIT_PAYLOAD_PREFIX: &[u8] = b"bb.bits1.qc_commit.v1";
const PROPOSAL_PAYLOAD_PREFIX: &[u8] = b"bb.bits1.proposal.v1";

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("redb error: {0}")]
    Db(String),
    #[error("json codec error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("genesis or state error: {0}")]
    State(String),
    #[error("signer boundary error: {0}")]
    Signer(String),
}

impl From<redb::Error> for StoreError {
    fn from(e: redb::Error) -> Self {
        Self::Db(e.to_string())
    }
}
impl From<redb::DatabaseError> for StoreError {
    fn from(e: redb::DatabaseError) -> Self {
        Self::Db(e.to_string())
    }
}
impl From<redb::TransactionError> for StoreError {
    fn from(e: redb::TransactionError) -> Self {
        Self::Db(e.to_string())
    }
}
impl From<redb::TableError> for StoreError {
    fn from(e: redb::TableError) -> Self {
        Self::Db(e.to_string())
    }
}
impl From<redb::StorageError> for StoreError {
    fn from(e: redb::StorageError) -> Self {
        Self::Db(e.to_string())
    }
}
impl From<redb::CommitError> for StoreError {
    fn from(e: redb::CommitError) -> Self {
        Self::Db(e.to_string())
    }
}

/// One validator entry committed into the genesis file and active committee schedule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatorGenesisEntry {
    /// 64-char hex canonical validator `ActorIdV1`.
    pub validator_id_hex: String,
    /// 64-char hex Ed25519 `ValidatorProposal` public key.
    pub proposal_pubkey_hex: String,
    /// 64-char hex Ed25519 `ValidatorVote` public key (distinct from `proposal_pubkey_hex`).
    pub vote_pubkey_hex: String,
    /// Positive voting weight.
    pub power: u64,
}

/// Serializable JSON genesis file shared across all validators in a shadow testnet cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BitsGenesisFile {
    pub network_id: String,
    pub custody_label: String,
    pub lab: bool,
    pub chain_id: u64,
    pub genesis_supply: u64,
    pub initial_grant_pool: u64,
    pub max_signup_grant_bits: u64,
    pub max_faucet_grant_bits: u64,
    pub min_purchase_bits: u64,
    pub creator_split_bps: u16,
    pub min_payout_bits: u64,
    pub epoch_blocks: u64,
    pub block_ms: u64,
    pub platform_treasury_actor_hex: String,
    pub grant_pool_actor_hex: String,
    pub payout_escrow_actor_hex: String,
    pub grant_relayers_hex: Vec<String>,
    pub validators: Vec<ValidatorGenesisEntry>,
}

impl BitsGenesisFile {
    /// Constructs a canonical `lab: true` Bitblock Bits shadow testnet genesis record.
    pub fn testnet_default(
        validators: Vec<ValidatorGenesisEntry>,
        grant_relayers_hex: Vec<String>,
        block_ms: u64,
    ) -> Self {
        let chain_id = BITS_TESTNET_CHAIN_ID;
        let treasury = derive_system_actor_id(chain_id, "platform-treasury");
        let grant_pool = derive_system_actor_id(chain_id, "testnet-grant-pool");
        let payout_escrow = derive_system_actor_id(chain_id, "simulated-payout-escrow");
        Self {
            network_id: "bitblock-bits-testnet-1".into(),
            custody_label: BITS_TESTNET_CUSTODY_LABEL.into(),
            lab: true,
            chain_id,
            genesis_supply: BITS_TESTNET_GENESIS_SUPPLY,
            initial_grant_pool: BITS_TESTNET_GENESIS_SUPPLY,
            max_signup_grant_bits: BITS_DEFAULT_SIGNUP_GRANT as u64,
            max_faucet_grant_bits: BITS_DEFAULT_FAUCET_GRANT as u64,
            min_purchase_bits: BITS_DEFAULT_MIN_PURCHASE as u64,
            creator_split_bps: BITS_DEFAULT_CREATOR_SPLIT_BPS,
            min_payout_bits: BITS_DEFAULT_MIN_PAYOUT as u64,
            epoch_blocks: 200,
            block_ms,
            platform_treasury_actor_hex: hex::encode(treasury.as_bytes()),
            grant_pool_actor_hex: hex::encode(grant_pool.as_bytes()),
            payout_escrow_actor_hex: hex::encode(payout_escrow.as_bytes()),
            grant_relayers_hex,
            validators,
        }
    }

    /// Converts this JSON genesis file into the canonical validated [`BitsGenesisConfigV1`].
    pub fn to_bits_genesis_config(&self) -> Result<BitsGenesisConfigV1, StoreError> {
        let cfg = BitsGenesisConfigV1 {
            version: BITS_1_VERSION,
            lab: self.lab,
            chain_id: self.chain_id,
            genesis_supply: u128::from(self.genesis_supply),
            initial_grant_pool: u128::from(self.initial_grant_pool),
            platform_treasury_actor: ActorIdV1::from_bytes(decode_hex_32(
                &self.platform_treasury_actor_hex,
            )?),
            grant_pool_actor: ActorIdV1::from_bytes(decode_hex_32(&self.grant_pool_actor_hex)?),
            payout_escrow_actor: ActorIdV1::from_bytes(decode_hex_32(
                &self.payout_escrow_actor_hex,
            )?),
            max_signup_grant_bits: u128::from(self.max_signup_grant_bits),
            max_faucet_grant_bits: u128::from(self.max_faucet_grant_bits),
            min_purchase_bits: u128::from(self.min_purchase_bits),
            creator_split_bps: self.creator_split_bps,
            min_payout_bits: u128::from(self.min_payout_bits),
        };
        cfg.validate()
            .map_err(|e| StoreError::State(e.to_string()))?;
        Ok(cfg)
    }

    /// Returns the canonical 32-byte genesis digest committed into every signed Bits tx and QC.
    pub fn genesis_id(&self) -> Result<[u8; 32], StoreError> {
        self.to_bits_genesis_config()?
            .genesis_id()
            .map_err(|e| StoreError::State(e.to_string()))
    }

    /// Returns the authorized testnet grant relayer Ed25519 public keys.
    pub fn grant_relayers(&self) -> Result<Vec<CanonicalEd25519PublicKeyV1>, StoreError> {
        let mut out = Vec::with_capacity(self.grant_relayers_hex.len());
        for h in &self.grant_relayers_hex {
            let bytes = decode_hex_32(h)?;
            let pk = CanonicalEd25519PublicKeyV1::from_bytes(bytes)
                .map_err(|e| StoreError::State(e.to_string()))?;
            out.push(pk);
        }
        Ok(out)
    }

    /// Computes the deterministic 32-byte `VALIDATOR-SCHEDULE-1` commitment root for this committee.
    pub fn validator_schedule_root(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(VALIDATOR_SCHEDULE_PREFIX);
        h.update((self.validators.len() as u32).to_be_bytes());
        for v in &self.validators {
            h.update(v.validator_id_hex.to_lowercase().as_bytes());
            h.update(b":");
            h.update(v.proposal_pubkey_hex.to_lowercase().as_bytes());
            h.update(b":");
            h.update(v.vote_pubkey_hex.to_lowercase().as_bytes());
            h.update(v.power.to_be_bytes());
        }
        h.finalize().into()
    }

    /// Returns the active committee formatted directly for `bitblock-light-wasm::verify_header_qc_json`.
    pub fn committee_json(&self) -> Vec<BitsCommitteeMemberJson> {
        self.validators
            .iter()
            .map(|v| BitsCommitteeMemberJson {
                validator_id: v.validator_id_hex.to_lowercase(),
                vote_pubkey_hex: v.vote_pubkey_hex.to_lowercase(),
                power: v.power,
            })
            .collect()
    }
}

/// Deterministic Merkle-style SHA-256 root over canonical block transaction hashes.
pub fn compute_tx_root(tx_hashes: &[[u8; 32]]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(TX_ROOT_PREFIX);
    h.update((tx_hashes.len() as u32).to_be_bytes());
    for th in tx_hashes {
        h.update(th);
    }
    h.finalize().into()
}

/// Computes the 32-byte payload digest signed by validators for a HotStuff commit QC.
/// Matches `bitblock-light-wasm::build_qc_commit_preimage` byte-for-byte.
pub fn qc_commit_payload_digest(
    genesis_id: [u8; 32],
    height: u64,
    round: u32,
    epoch: u64,
    block_hash: [u8; 32],
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(QC_COMMIT_PAYLOAD_PREFIX);
    h.update(genesis_id);
    h.update(height.to_be_bytes());
    h.update(round.to_be_bytes());
    h.update(epoch.to_be_bytes());
    h.update(block_hash);
    h.finalize().into()
}

/// Computes the 32-byte payload digest signed by the round leader for a block proposal.
pub fn proposal_payload_digest(
    genesis_id: [u8; 32],
    height: u64,
    round: u32,
    epoch: u64,
    block_hash: [u8; 32],
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(PROPOSAL_PAYLOAD_PREFIX);
    h.update(genesis_id);
    h.update(height.to_be_bytes());
    h.update(round.to_be_bytes());
    h.update(epoch.to_be_bytes());
    h.update(block_hash);
    h.finalize().into()
}

/// JSON-serializable committed Bits transaction receipt served by `GET /proof/tx/{hash}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxReceiptRecord {
    pub block_height: u64,
    pub tx_hash: String,
    pub opcode: u8,
    pub kind: String,
    pub actor_id: String,
    pub balance_root: String,
    pub nonce_root: String,
    pub grant_pool_remaining: u64,
    pub treasury_balance: u64,
    pub simulated_payout_escrow: u64,
    pub genesis_supply: u64,
    pub creator_bits_paid: u64,
    pub platform_bits_paid: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buyer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_bits: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo_hash: Option<String>,
}

impl TxReceiptRecord {
    /// Builds a JSON receipt record from a committed [`BitsSignedTxV1`] and [`BitsStateReceiptV1`].
    pub fn from_committed(tx: &BitsSignedTxV1, rcpt: &BitsStateReceiptV1) -> Self {
        let (kind, order_id, buyer, creator, recipient, amount_bits, memo_hash) = match &tx.payload
        {
            BitsTxPayloadV1::Transfer(op) => (
                "transfer",
                None,
                None,
                None,
                Some(hex::encode(op.to.as_bytes())),
                Some(op.amount_bits as u64),
                Some(hex::encode(op.memo_hash)),
            ),
            BitsTxPayloadV1::PurchaseSplit(op) => (
                "purchase_split",
                Some(hex::encode(op.order_id)),
                Some(hex::encode(op.buyer.as_bytes())),
                Some(hex::encode(op.creator.as_bytes())),
                None,
                Some(op.amount_bits as u64),
                Some(hex::encode(op.memo_hash)),
            ),
            BitsTxPayloadV1::TestnetGrant(op) => (
                "testnet_grant",
                None,
                None,
                None,
                Some(hex::encode(op.recipient.as_bytes())),
                Some(op.amount_bits as u64),
                Some(hex::encode(op.grant_id)),
            ),
            BitsTxPayloadV1::SimulatedPayout(op) => (
                "simulated_payout",
                None,
                None,
                Some(hex::encode(op.creator.as_bytes())),
                None,
                Some(op.amount_bits as u64),
                Some(hex::encode(op.payout_request_id)),
            ),
            BitsTxPayloadV1::IdentityBind(op) => (
                "identity_bind",
                None,
                None,
                None,
                Some(hex::encode(op.actor_id.as_bytes())),
                None,
                Some(hex::encode(op.external_id_hash)),
            ),
            BitsTxPayloadV1::L2Anchor(op) => (
                "l2_anchor",
                None,
                None,
                None,
                None,
                None,
                Some(hex::encode(op.l2_root)),
            ),
        };
        Self {
            block_height: rcpt.block_height,
            tx_hash: hex::encode(rcpt.tx_hash),
            opcode: rcpt.opcode,
            kind: kind.to_string(),
            actor_id: hex::encode(rcpt.actor_id.as_bytes()),
            balance_root: hex::encode(rcpt.balance_root),
            nonce_root: hex::encode(rcpt.nonce_root),
            grant_pool_remaining: rcpt.grant_pool_remaining as u64,
            treasury_balance: rcpt.treasury_balance as u64,
            simulated_payout_escrow: rcpt.simulated_payout_escrow as u64,
            genesis_supply: rcpt.genesis_supply as u64,
            creator_bits_paid: rcpt.creator_bits_paid as u64,
            platform_bits_paid: rcpt.platform_bits_paid as u64,
            order_id,
            buyer,
            creator,
            recipient,
            amount_bits,
            memo_hash,
        }
    }
}

/// One finalized Bits L1 block persisted atomically in `redb` and served over `/proof/block/{k}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalizedBitsBlock {
    pub header: BitsBlockHeaderJson,
    pub qc: BitsQuorumCertJson,
    pub txs_hex: Vec<String>,
    pub receipts: Vec<TxReceiptRecord>,
}

/// Crash-safe ACID `redb` store backing finalized blocks, transaction receipts, and order lookups.
#[derive(Clone)]
pub struct ChainStore {
    db: Arc<Database>,
}

impl ChainStore {
    /// Opens or creates the `redb` database at `path` and ensures all tables exist.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        if let Some(parent) = path.as_ref().parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| StoreError::Db(e.to_string()))?;
        }
        let db = Database::create(path)?;
        let wtx = db.begin_write()?;
        {
            let _ = wtx.open_table(BLOCKS_TABLE)?;
            let _ = wtx.open_table(META_TABLE)?;
            let _ = wtx.open_table(TX_INDEX_TABLE)?;
            let _ = wtx.open_table(ORDER_INDEX_TABLE)?;
        }
        wtx.commit()?;
        Ok(Self { db: Arc::new(db) })
    }

    /// Initializes genesis metadata if empty, then deterministically replays all finalized blocks
    /// into [`BitsEconomyStateV1`], verifying every header root and exact Bits supply conservation.
    pub fn init_or_replay(
        &self,
        genesis: &BitsGenesisFile,
    ) -> Result<
        (
            BitsEconomyStateV1,
            u64,
            [u8; 32],
            Option<FinalizedBitsBlock>,
        ),
        StoreError,
    > {
        let genesis_id = genesis.genesis_id()?;
        let cfg = genesis.to_bits_genesis_config()?;
        let relayers = genesis.grant_relayers()?;

        let stored_genesis_id = {
            let rtx = self.db.begin_read()?;
            let meta = rtx.open_table(META_TABLE)?;
            meta.get("genesis_id")?.map(|v| v.value().to_vec())
        };
        if let Some(existing) = stored_genesis_id {
            if existing.as_slice() != genesis_id.as_slice() {
                return Err(StoreError::State(
                    "stored genesis_id does not match provided genesis.json".into(),
                ));
            }
        } else {
            let genesis_bytes = serde_json::to_vec(genesis)?;
            let wtx = self.db.begin_write()?;
            {
                let mut meta = wtx.open_table(META_TABLE)?;
                meta.insert("genesis", genesis_bytes.as_slice())?;
                meta.insert("genesis_id", genesis_id.as_slice())?;
                meta.insert("head_height", 0u64.to_be_bytes().as_slice())?;
                meta.insert("head_hash", genesis_id.as_slice())?;
            }
            wtx.commit()?;
        }

        let mut state = BitsEconomyStateV1::from_genesis(cfg, &[], &relayers)
            .map_err(|e| StoreError::State(e.to_string()))?;
        let mut head_height = 0u64;
        let mut head_hash = genesis_id;
        let mut tail: Option<FinalizedBitsBlock> = None;

        let all_blocks = self.load_all_blocks()?;
        for fb in all_blocks {
            let expected_h = head_height + 1;
            if fb.header.height != expected_h {
                return Err(StoreError::State(format!(
                    "non-contiguous persisted block height {} (expected {})",
                    fb.header.height, expected_h
                )));
            }
            if fb.header.parent_hash != hex::encode(head_hash) {
                return Err(StoreError::State(format!(
                    "parent hash mismatch replaying height {}",
                    fb.header.height
                )));
            }
            let mut tx_hashes = Vec::with_capacity(fb.txs_hex.len());
            for tx_hex in &fb.txs_hex {
                let raw = hex::decode(tx_hex).map_err(|e| StoreError::State(e.to_string()))?;
                let tx = BitsSignedTxV1::decode_canonical(&raw)
                    .map_err(|e| StoreError::State(e.to_string()))?;
                let rcpt = state
                    .apply_signed_tx(&tx, fb.header.height)
                    .map_err(|e| StoreError::State(e.to_string()))?;
                tx_hashes.push(rcpt.tx_hash);
            }
            let tx_root = compute_tx_root(&tx_hashes);
            if fb.header.balance_root != hex::encode(state.balance_root())
                || fb.header.nonce_root != hex::encode(state.nonce_root())
                || fb.header.tx_root != hex::encode(tx_root)
            {
                return Err(StoreError::State(format!(
                    "replayed state root mismatch at height {}",
                    fb.header.height
                )));
            }
            let bh = compute_bits_header_hash(&fb.header)
                .map_err(|e| StoreError::State(e.to_string()))?;
            head_height = fb.header.height;
            head_hash = bh;
            tail = Some(fb);
        }
        state
            .verify_conservation()
            .map_err(|e| StoreError::State(e.to_string()))?;
        Ok((state, head_height, head_hash, tail))
    }

    /// Atomically persists one finalized block plus its tx and order lookup indexes.
    pub fn commit_block(&self, finalized: &FinalizedBitsBlock) -> Result<[u8; 32], StoreError> {
        let h = finalized.header.height;
        let bh = compute_bits_header_hash(&finalized.header)
            .map_err(|e| StoreError::State(e.to_string()))?;
        let block_bytes = serde_json::to_vec(finalized)?;
        let wtx = self.db.begin_write()?;
        {
            let mut blocks = wtx.open_table(BLOCKS_TABLE)?;
            blocks.insert(h, block_bytes.as_slice())?;

            let mut tx_idx = wtx.open_table(TX_INDEX_TABLE)?;
            let mut ord_idx = wtx.open_table(ORDER_INDEX_TABLE)?;
            for rcpt in &finalized.receipts {
                if let Ok(th) = decode_hex_32(&rcpt.tx_hash) {
                    tx_idx.insert(th.as_slice(), h)?;
                    if let Some(ref oid_hex) = rcpt.order_id
                        && let Ok(oid) = decode_hex_32(oid_hex)
                    {
                        ord_idx.insert(oid.as_slice(), th.as_slice())?;
                    }
                }
            }

            let mut meta = wtx.open_table(META_TABLE)?;
            meta.insert("head_height", h.to_be_bytes().as_slice())?;
            meta.insert("head_hash", bh.as_slice())?;
        }
        wtx.commit()?;
        Ok(bh)
    }

    /// Returns the highest persisted block height (`0` at genesis).
    pub fn head_height(&self) -> Result<u64, StoreError> {
        let rtx = self.db.begin_read()?;
        let meta = rtx.open_table(META_TABLE)?;
        let Some(val) = meta.get("head_height")? else {
            return Ok(0);
        };
        let arr: [u8; 8] = val
            .value()
            .try_into()
            .map_err(|_| StoreError::Db("corrupt head_height".into()))?;
        Ok(u64::from_be_bytes(arr))
    }

    /// Loads a single finalized block by height.
    pub fn get_block(&self, height: u64) -> Result<Option<FinalizedBitsBlock>, StoreError> {
        let rtx = self.db.begin_read()?;
        let blocks = rtx.open_table(BLOCKS_TABLE)?;
        let Some(val) = blocks.get(height)? else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_slice(val.value())?))
    }

    /// Lists up to `limit` contiguous finalized blocks starting at `from`.
    pub fn list_blocks(
        &self,
        from: u64,
        limit: usize,
    ) -> Result<Vec<FinalizedBitsBlock>, StoreError> {
        let rtx = self.db.begin_read()?;
        let blocks = rtx.open_table(BLOCKS_TABLE)?;
        let mut out = Vec::new();
        for entry in blocks.range(from..)? {
            let (_, val) = entry?;
            out.push(serde_json::from_slice(val.value())?);
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    fn load_all_blocks(&self) -> Result<Vec<FinalizedBitsBlock>, StoreError> {
        let rtx = self.db.begin_read()?;
        let blocks = rtx.open_table(BLOCKS_TABLE)?;
        let mut out = Vec::new();
        for entry in blocks.iter()? {
            let (_, val) = entry?;
            out.push(serde_json::from_slice(val.value())?);
        }
        Ok(out)
    }

    /// Looks up a committed transaction and its receipt by canonical 32-byte `tx_hash`.
    pub fn get_tx(&self, tx_hash: &[u8; 32]) -> Result<Option<(u64, TxReceiptRecord)>, StoreError> {
        let h = {
            let rtx = self.db.begin_read()?;
            let tx_idx = rtx.open_table(TX_INDEX_TABLE)?;
            tx_idx.get(tx_hash.as_slice())?.map(|v| v.value())
        };
        let Some(height) = h else {
            return Ok(None);
        };
        let Some(fb) = self.get_block(height)? else {
            return Ok(None);
        };
        let target_hex = hex::encode(tx_hash);
        for rcpt in fb.receipts {
            if rcpt.tx_hash == target_hex {
                return Ok(Some((height, rcpt)));
            }
        }
        Ok(None)
    }

    /// Looks up a committed `BitsPurchaseSplitV1` receipt by its 32-byte `order_id`.
    pub fn get_order(
        &self,
        order_id: &[u8; 32],
    ) -> Result<Option<(u64, TxReceiptRecord)>, StoreError> {
        let th_bytes = {
            let rtx = self.db.begin_read()?;
            let ord_idx = rtx.open_table(ORDER_INDEX_TABLE)?;
            ord_idx
                .get(order_id.as_slice())?
                .map(|v| v.value().to_vec())
        };
        let Some(raw) = th_bytes else {
            return Ok(None);
        };
        let Ok(th) = <[u8; 32]>::try_from(raw.as_slice()) else {
            return Ok(None);
        };
        self.get_tx(&th)
    }
}

/// Real `SIGNER-OPS-1` kernel-locked validator signer boundary wrapping [`PersistentValidatorSignerV1`].
///
/// Acquires an exclusive OS lock (`flock` / Windows `share_mode(0)`) on `<signer_dir>/signer.lock`
/// and fsyncs `<signer_dir>/watermark.bin` atomically BEFORE releasing any proposal or commit vote
/// Ed25519 signature bytes.
pub struct ValidatorSignerBoundary {
    signer: PersistentValidatorSignerV1,
    registry: KeyLifecycleRegistryV1,
    genesis_id: [u8; 32],
    validator_id: ActorIdV1,
    proposal_pubkey_hex: String,
    vote_pubkey_hex: String,
    failover_epoch: u64,
}

impl ValidatorSignerBoundary {
    /// Opens or initializes the crash-safe `SIGNER-OPS-1` signer in `signer_dir`.
    /// Fails closed if another process holds the exclusive lock or if `watermark.bin` is corrupt.
    pub fn open_or_create(
        signer_dir: PathBuf,
        chain_id: u64,
        genesis_id: [u8; 32],
        validator_id: ActorIdV1,
        proposal_seed: [u8; 32],
        vote_seed: [u8; 32],
    ) -> Result<Self, StoreError> {
        let fork = ExecutorForkContextV1 {
            chain_id,
            world_id: 1,
            genesis_id,
            profile_id: genesis_id,
            transition_spec_digest: genesis_id,
        };
        let mut registry = KeyLifecycleRegistryV1::new();
        let mut salt_h = Sha256::new();
        salt_h.update(b"bb.bits1.signer.ceremony.v1");
        salt_h.update(genesis_id);
        salt_h.update(validator_id.as_bytes());
        let salt_dig: [u8; 32] = salt_h.finalize().into();
        let proposal_salt: [u8; 16] = salt_dig[0..16].try_into().expect("16");
        let vote_salt: [u8; 16] = salt_dig[16..32].try_into().expect("16");

        let ceremony = ValidatorKeyCeremonyBundleV1::execute(
            &fork,
            validator_id,
            &proposal_seed,
            &vote_seed,
            b"bitblock-testnet-signer-ceremony",
            proposal_salt,
            [0x11; 16],
            vote_salt,
            [0x22; 16],
            1,
            &mut registry,
        )
        .map_err(|e| StoreError::Signer(e.to_string()))?;

        let proposal_pubkey_hex = hex::encode(ceremony.policy.proposal_key.as_bytes());
        let vote_pubkey_hex = hex::encode(ceremony.policy.vote_key.as_bytes());
        let failover_epoch = ceremony.policy.failover_epoch;

        let signer = PersistentValidatorSignerV1::open_or_create(
            signer_dir,
            fork,
            &ceremony,
            proposal_seed,
            vote_seed,
        )
        .map_err(|e| StoreError::Signer(e.to_string()))?;

        Ok(Self {
            signer,
            registry,
            genesis_id,
            validator_id,
            proposal_pubkey_hex,
            vote_pubkey_hex,
            failover_epoch,
        })
    }

    pub fn validator_id_hex(&self) -> String {
        hex::encode(self.validator_id.as_bytes())
    }

    pub fn proposal_pubkey_hex(&self) -> &str {
        &self.proposal_pubkey_hex
    }

    pub fn vote_pubkey_hex(&self) -> &str {
        &self.vote_pubkey_hex
    }

    pub fn last_watermark(&self) -> SignerWatermarkV1 {
        self.signer.core().persisted_state().last_watermark
    }

    /// Durably advances the `SIGNER-OPS-1` watermark to `(epoch, height, round, Proposal)` and
    /// signs the proposal digest using the isolated `ValidatorProposal` key.
    pub fn sign_proposal_durable(
        &mut self,
        epoch: u64,
        height: u64,
        round: u32,
        block_hash: [u8; 32],
    ) -> Result<(CanonicalEd25519SignatureV1, SignerAuditReceiptV1), StoreError> {
        let wm = SignerWatermarkV1 {
            epoch,
            height,
            round,
            phase: SignerPhaseCodeV1::Proposal,
        };
        let digest = proposal_payload_digest(self.genesis_id, height, round, epoch, block_hash);
        self.signer
            .sign_digest_durable(
                CredentialRoleV1::ValidatorProposal,
                SigningDomainTagV1::BlockProposal,
                self.failover_epoch,
                wm,
                block_hash,
                digest,
                &self.registry,
            )
            .map_err(|e| StoreError::Signer(e.to_string()))
    }

    /// Durably advances the `SIGNER-OPS-1` watermark to `(epoch, height, round, CommitVote)` and
    /// signs the QC commit digest using the isolated `ValidatorVote` key.
    /// The returned signature verifies directly under `bitblock-light-wasm::verify_header_qc_json`.
    pub fn sign_qc_commit_durable(
        &mut self,
        epoch: u64,
        height: u64,
        round: u32,
        block_hash: [u8; 32],
    ) -> Result<(CanonicalEd25519SignatureV1, SignerAuditReceiptV1), StoreError> {
        let wm = SignerWatermarkV1 {
            epoch,
            height,
            round,
            phase: SignerPhaseCodeV1::CommitVote,
        };
        let digest = qc_commit_payload_digest(self.genesis_id, height, round, epoch, block_hash);
        self.signer
            .sign_digest_durable(
                CredentialRoleV1::ValidatorVote,
                SigningDomainTagV1::ConsensusVote,
                self.failover_epoch,
                wm,
                block_hash,
                digest,
                &self.registry,
            )
            .map_err(|e| StoreError::Signer(e.to_string()))
    }
}

pub fn decode_hex_32(s: &str) -> Result<[u8; 32], StoreError> {
    let raw = hex::decode(s.trim()).map_err(|_| StoreError::State("invalid 32-byte hex".into()))?;
    raw.try_into()
        .map_err(|_| StoreError::State("expected 32 bytes".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitblock_light_wasm::{BitsQcSignatureJson, check_header_qc_slice};
    use bitblock_research::bits_economy_1::{
        BitsGrantKindV1, BitsPurchaseSplitV1, BitsTestnetGrantV1, derive_default_bits_actor_id,
    };
    use serde_json::json;

    #[test]
    fn acid_commit_reopen_replay_and_signer_ops_1_safety() {
        let base = std::env::temp_dir().join(format!(
            "bb_store_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();

        let prop_seed = [0x11; 32];
        let vote_seed = [0x22; 32];
        let relayer_seed = [0x33; 32];
        let buyer_seed = [0x44; 32];
        let creator_seed = [0x55; 32];

        let val_id = ActorIdV1::from_bytes([0x01; 32]);
        let prop_pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&prop_seed).unwrap();
        let vote_pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&vote_seed).unwrap();
        let relayer_pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&relayer_seed).unwrap();
        let buyer_pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&buyer_seed).unwrap();
        let creator_pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&creator_seed).unwrap();

        let genesis = BitsGenesisFile::testnet_default(
            vec![ValidatorGenesisEntry {
                validator_id_hex: hex::encode(val_id.as_bytes()),
                proposal_pubkey_hex: hex::encode(prop_pk.as_bytes()),
                vote_pubkey_hex: hex::encode(vote_pk.as_bytes()),
                power: 100,
            }],
            vec![hex::encode(relayer_pk.as_bytes())],
            1_000,
        );
        let genesis_id = genesis.genesis_id().unwrap();
        let db_path = base.join("chain.redb");
        let signer_dir = base.join("signer");

        let buyer_actor = derive_default_bits_actor_id(genesis.chain_id, &buyer_pk).unwrap();
        let creator_actor = derive_default_bits_actor_id(genesis.chain_id, &creator_pk).unwrap();
        let treasury_actor = genesis
            .to_bits_genesis_config()
            .unwrap()
            .platform_treasury_actor;

        let order_id = [0x99; 32];
        let purchase_tx_hash;
        {
            let store = ChainStore::open(&db_path).unwrap();
            let (mut state, h0, hash0, tail0) = store.init_or_replay(&genesis).unwrap();
            assert_eq!(h0, 0);
            assert_eq!(hash0, genesis_id);
            assert!(tail0.is_none());

            let mut signer = ValidatorSignerBoundary::open_or_create(
                signer_dir.clone(),
                genesis.chain_id,
                genesis_id,
                val_id,
                prop_seed,
                vote_seed,
            )
            .unwrap();

            // Concurrent second signer handle on same directory must fail closed (`flock`).
            assert!(
                ValidatorSignerBoundary::open_or_create(
                    signer_dir.clone(),
                    genesis.chain_id,
                    genesis_id,
                    val_id,
                    prop_seed,
                    vote_seed,
                )
                .is_err()
            );

            // Block 1: signup grant (1,000 Bits) + atomic 80/20 purchase (250 Bits)
            let grant_tx = BitsSignedTxV1::sign_with_seed(
                genesis.chain_id,
                genesis_id,
                genesis.to_bits_genesis_config().unwrap().grant_pool_actor,
                &relayer_seed,
                1,
                100,
                BitsTxPayloadV1::TestnetGrant(BitsTestnetGrantV1 {
                    recipient: buyer_actor,
                    recipient_pubkey: buyer_pk,
                    amount_bits: 1_000,
                    grant_kind: BitsGrantKindV1::Signup,
                    grant_id: [0x77; 32],
                }),
            )
            .unwrap();
            let r1 = state.apply_signed_tx(&grant_tx, 1).unwrap();

            let buy_tx = BitsSignedTxV1::sign_with_seed(
                genesis.chain_id,
                genesis_id,
                buyer_actor,
                &buyer_seed,
                1,
                100,
                BitsTxPayloadV1::PurchaseSplit(BitsPurchaseSplitV1 {
                    buyer: buyer_actor,
                    creator: creator_actor,
                    treasury: treasury_actor,
                    amount_bits: 250,
                    creator_bps: 8_000,
                    order_id,
                    memo_hash: [0x88; 32],
                }),
            )
            .unwrap();
            let r2 = state.apply_signed_tx(&buy_tx, 1).unwrap();
            purchase_tx_hash = r2.tx_hash;

            let tx_root = compute_tx_root(&[r1.tx_hash, r2.tx_hash]);
            let header = BitsBlockHeaderJson {
                chain_id: genesis.chain_id,
                genesis_id: hex::encode(genesis_id),
                height: 1,
                round: 0,
                epoch: 0,
                parent_hash: hex::encode(genesis_id),
                balance_root: hex::encode(state.balance_root()),
                nonce_root: hex::encode(state.nonce_root()),
                tx_root: hex::encode(tx_root),
                validator_schedule_root: hex::encode(genesis.validator_schedule_root()),
            };
            let bh = compute_bits_header_hash(&header).unwrap();

            let _prop_sig = signer.sign_proposal_durable(0, 1, 0, bh).unwrap();
            let (vote_sig, _) = signer.sign_qc_commit_durable(0, 1, 0, bh).unwrap();
            // Equivocation at same (epoch=0, height=1, round=0, CommitVote) with distinct block_hash MUST fail!
            assert!(signer.sign_qc_commit_durable(0, 1, 0, [0xEE; 32]).is_err());

            let qc = BitsQuorumCertJson {
                genesis_id: hex::encode(genesis_id),
                height: 1,
                round: 0,
                epoch: 0,
                block_hash: hex::encode(bh),
                signatures: vec![BitsQcSignatureJson {
                    validator_id: signer.validator_id_hex(),
                    vote_pubkey_hex: signer.vote_pubkey_hex().to_string(),
                    sig_hex: hex::encode(vote_sig.as_bytes()),
                }],
            };

            // Browser light-wasm verifier MUST accept the exact SIGNER-OPS-1 emitted QC signature!
            let qc_payload = serde_json::to_vec(&json!({
                "header": header,
                "qc": qc,
                "committee": genesis.committee_json(),
            }))
            .unwrap();
            assert_eq!(check_header_qc_slice(&qc_payload), 1);

            let fb = FinalizedBitsBlock {
                header,
                qc,
                txs_hex: vec![
                    hex::encode(grant_tx.encode_canonical().unwrap()),
                    hex::encode(buy_tx.encode_canonical().unwrap()),
                ],
                receipts: vec![
                    TxReceiptRecord::from_committed(&grant_tx, &r1),
                    TxReceiptRecord::from_committed(&buy_tx, &r2),
                ],
            };
            store.commit_block(&fb).unwrap();
        }

        // Cold reopen: verify full ACID replay parity + order index + watermark rollback refusal!
        let store2 = ChainStore::open(&db_path).unwrap();
        assert_eq!(store2.head_height().unwrap(), 1);
        let (replayed, h1, _, tail1) = store2.init_or_replay(&genesis).unwrap();
        assert_eq!(h1, 1);
        assert!(tail1.is_some());
        assert_eq!(replayed.balance_of(&buyer_actor), 750);
        assert_eq!(replayed.balance_of(&creator_actor), 200);
        assert_eq!(replayed.treasury_balance(), 50);

        let (ord_h, ord_rcpt) = store2.get_order(&order_id).unwrap().unwrap();
        assert_eq!(ord_h, 1);
        assert_eq!(ord_rcpt.tx_hash, hex::encode(purchase_tx_hash));
        assert_eq!(ord_rcpt.creator_bits_paid, 200);
        assert_eq!(ord_rcpt.platform_bits_paid, 50);

        let mut signer2 = ValidatorSignerBoundary::open_or_create(
            signer_dir,
            genesis.chain_id,
            genesis_id,
            val_id,
            prop_seed,
            vote_seed,
        )
        .unwrap();
        // Watermark rollback to height 1 Proposal after height 1 CommitVote MUST be refused across restart!
        assert!(signer2.sign_proposal_durable(0, 1, 0, [0xAA; 32]).is_err());

        let _ = std::fs::remove_dir_all(&base);
    }
}
