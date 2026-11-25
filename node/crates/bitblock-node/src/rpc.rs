//! Hardened `axum` CORS HTTP proof-door server for the merged Bitblock L1 Bits testnet daemon.
//!
//! Endpoints:
//! - `GET /health`
//! - `GET /metrics` (Prometheus text format)
//! - `GET /proof` (head + Bits unit metadata + Ed25519 QC + header + system actors)
//! - `GET /proof/blocks?from=&limit=` (capped at `<= 100`, served lock-free from RAM ring buffer)
//! - `GET /proof/block/{k}`
//! - `GET /proof/tx/{hash}` (committed receipt including `80/20` split fields and `order_id`)
//! - `GET /proof/wallet/{addr}` (Cross-Session Amendment 1: accepts `ActorIdV1` OR raw Ed25519
//!   `pubkeyHex`; always returns canonical `actorId`, active `ed25519Hex`/`pubkeyHex`, `balance`,
//!   `seq`, and `AUTH-COMMIT-1` Domain 3 `DomainPatriciaProofV1` inclusion or non-membership proof)
//! - `GET /proof/validators` (committee formatted for `bitblock-light-wasm::verify_header_qc_json`)
//! - `POST /tx` (accepts `{ tx_hex }` or transitional `{ tlv_hex }`; returns `202 Accepted`)
//! - `POST /faucet` (strictly `lab == true`, else `403 Forbidden`; per-key + global rate limits)

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::{Duration, Instant};

use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{get, options, post};
use axum::{Json, Router};
use bitblock_light_wasm::compute_bits_header_hash;
use bitblock_net::NetMetrics;
use bitblock_research::bits_economy_1::{BitsEconomyStateV1, BitsSignedTxV1};
use bitblock_store::{
    BITS_TESTNET_CUSTODY_LABEL, BitsGenesisFile, ChainStore, FinalizedBitsBlock, decode_hex_32,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{RwLock, Semaphore, mpsc, oneshot};

pub const MAX_HTTP_BODY_BYTES: usize = 64 * 1024;
pub const MAX_INFLIGHT_HTTP: usize = 256;
pub const RPC_ACTION_QUEUE_CAP: usize = 512;
pub const HOT_BLOCK_SUMMARY_CAP: usize = 512;
pub const MAX_BLOCKS_PAGE_LIMIT: usize = 100;
const FAUCET_GLOBAL_LIMIT: usize = 20;
const FAUCET_GLOBAL_WINDOW: Duration = Duration::from_secs(10);
const FAUCET_PER_KEY_COOLDOWN: Duration = Duration::from_secs(2);
const FAUCET_COOLDOWN_MAX_ENTRIES: usize = 1_024;
const RPC_ACTION_TIMEOUT: Duration = Duration::from_secs(5);

#[allow(clippy::large_enum_variant)]
pub enum RpcAction {
    SubmitTx {
        tx: BitsSignedTxV1,
        raw_bytes: Vec<u8>,
        reply: oneshot::Sender<Result<String, String>>,
    },
    RequestFaucet {
        recipient_pubkey: [u8; 32],
        amount_bits: u64,
        reply: oneshot::Sender<Result<String, String>>,
    },
}

/// Immutable per-block RAM snapshot served lock-free by HTTP proof doors.
pub struct CachedReadView {
    pub state: BitsEconomyStateV1,
    pub height: u64,
    pub head_hash: [u8; 32],
    pub tail: Option<FinalizedBitsBlock>,
}

#[derive(Clone)]
pub struct CachedBlockSummary {
    pub height: u64,
    pub hash: Arc<str>,
    pub json: Arc<str>,
}

pub fn build_block_summary(fb: &FinalizedBitsBlock) -> CachedBlockSummary {
    let hash_str = compute_bits_header_hash(&fb.header)
        .map(hex::encode)
        .unwrap_or_else(|_| fb.qc.block_hash.clone());
    let val = json!({
        "k": fb.header.height,
        "height": fb.header.height,
        "round": fb.header.round,
        "epoch": fb.header.epoch,
        "hash": hash_str,
        "parent": fb.header.parent_hash,
        "balanceRoot": fb.header.balance_root,
        "nonceRoot": fb.header.nonce_root,
        "txRoot": fb.header.tx_root,
        "txCount": fb.txs_hex.len(),
        "sealed": true,
    });
    CachedBlockSummary {
        height: fb.header.height,
        hash: Arc::from(hash_str),
        json: Arc::from(val.to_string()),
    }
}

pub struct SharedRpcState {
    pub genesis: BitsGenesisFile,
    pub genesis_id: [u8; 32],
    pub store: ChainStore,
    pub cached_view: StdRwLock<Arc<CachedReadView>>,
    pub recent_blocks: StdRwLock<VecDeque<CachedBlockSummary>>,
    pub http_permits: Arc<Semaphore>,
    pub peer_count: RwLock<usize>,
    pub mempool_len: AtomicU64,
    pub bft_round: AtomicU64,
    pub wal_writes_total: AtomicU64,
    pub http_rate_limited_total: AtomicU64,
    pub http_shed_total: AtomicU64,
    pub net_metrics: Arc<NetMetrics>,
    pub action_tx: mpsc::Sender<RpcAction>,
    pub validator_id_hex: String,
    pub vote_pubkey_hex: String,
    pub faucet_cooldowns: RwLock<HashMap<String, Instant>>,
    pub faucet_global_window: RwLock<VecDeque<Instant>>,
}

impl SharedRpcState {
    pub fn read_view(&self) -> Arc<CachedReadView> {
        self.cached_view.read().expect("rwlock").clone()
    }

    pub fn update_view(
        &self,
        state: BitsEconomyStateV1,
        height: u64,
        head_hash: [u8; 32],
        tail: Option<FinalizedBitsBlock>,
    ) {
        if let Some(ref fb) = tail {
            let summary = build_block_summary(fb);
            let mut ring = self.recent_blocks.write().expect("rwlock");
            if ring.back().map(|e| e.height) != Some(fb.header.height) {
                ring.push_back(summary);
                while ring.len() > HOT_BLOCK_SUMMARY_CAP {
                    ring.pop_front();
                }
            }
        }
        let next = Arc::new(CachedReadView {
            state,
            height,
            head_hash,
            tail,
        });
        *self.cached_view.write().expect("rwlock") = next;
    }

    fn try_http_permit(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        match self.http_permits.clone().try_acquire_owned() {
            Ok(p) => Some(p),
            Err(_) => {
                self.http_shed_total.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }
}

pub fn build_router(shared: Arc<SharedRpcState>) -> Router {
    Router::new()
        .route("/health", get(handle_health).options(handle_cors_preflight))
        .route("/metrics", get(handle_metrics))
        .route(
            "/proof",
            get(handle_proof_head).options(handle_cors_preflight),
        )
        .route(
            "/proof/blocks",
            get(handle_proof_blocks).options(handle_cors_preflight),
        )
        .route(
            "/proof/block/{k}",
            get(handle_proof_block).options(handle_cors_preflight),
        )
        .route(
            "/proof/tx/{hash}",
            get(handle_proof_tx).options(handle_cors_preflight),
        )
        .route(
            "/proof/wallet/{addr}",
            get(handle_proof_wallet).options(handle_cors_preflight),
        )
        .route(
            "/proof/validators",
            get(handle_proof_validators).options(handle_cors_preflight),
        )
        .route("/tx", post(handle_submit_tx).options(handle_cors_preflight))
        .route(
            "/faucet",
            post(handle_faucet).options(handle_cors_preflight),
        )
        .route("/{*any}", options(handle_cors_preflight))
        .layer(DefaultBodyLimit::max(MAX_HTTP_BODY_BYTES))
        .with_state(shared)
}

fn cors_json(status: StatusCode, body: Value) -> axum::response::Response {
    (
        status,
        [
            (
                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                HeaderValue::from_static("*"),
            ),
            (
                header::ACCESS_CONTROL_ALLOW_METHODS,
                HeaderValue::from_static("GET, POST, OPTIONS"),
            ),
            (
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static("Content-Type"),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
        Json(body),
    )
        .into_response()
}

fn cors_raw_json(status: StatusCode, body: String) -> axum::response::Response {
    (
        status,
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            ),
            (
                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                HeaderValue::from_static("*"),
            ),
            (
                header::ACCESS_CONTROL_ALLOW_METHODS,
                HeaderValue::from_static("GET, POST, OPTIONS"),
            ),
            (
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static("Content-Type"),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
        body,
    )
        .into_response()
}

fn shed_503(reason: &str) -> axum::response::Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [
            (
                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                HeaderValue::from_static("*"),
            ),
            (
                header::ACCESS_CONTROL_ALLOW_METHODS,
                HeaderValue::from_static("GET, POST, OPTIONS"),
            ),
            (
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static("Content-Type"),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            (header::RETRY_AFTER, HeaderValue::from_static("1")),
        ],
        Json(json!({ "ok": false, "reason": reason })),
    )
        .into_response()
}

async fn handle_cors_preflight() -> impl IntoResponse {
    (
        StatusCode::NO_CONTENT,
        [
            (
                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                HeaderValue::from_static("*"),
            ),
            (
                header::ACCESS_CONTROL_ALLOW_METHODS,
                HeaderValue::from_static("GET, POST, OPTIONS"),
            ),
            (
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static("Content-Type"),
            ),
        ],
    )
}

async fn handle_health(State(st): State<Arc<SharedRpcState>>) -> axum::response::Response {
    let view = st.read_view();
    let peers = *st.peer_count.read().await;
    cors_json(
        StatusCode::OK,
        json!({
            "ok": true,
            "protocol": "bitblock-l1-v1",
            "chainId": st.genesis.chain_id,
            "height": view.height,
            "round": st.bft_round.load(Ordering::Relaxed),
            "peers": peers,
            "mempoolLen": st.mempool_len.load(Ordering::Relaxed),
            "walWrites": st.wal_writes_total.load(Ordering::Relaxed),
            "validatorId": st.validator_id_hex,
            "votePubkey": st.vote_pubkey_hex,
            "headHash": hex::encode(view.head_hash),
            "grantPoolRemaining": view.state.grant_pool_remaining() as u64,
            "treasuryBalance": view.state.treasury_balance() as u64,
            "simulatedPayoutEscrow": view.state.simulated_payout_escrow() as u64,
            "lab": st.genesis.lab,
            "custodyLabel": BITS_TESTNET_CUSTODY_LABEL,
        }),
    )
}

async fn handle_metrics(State(st): State<Arc<SharedRpcState>>) -> impl IntoResponse {
    let view = st.read_view();
    let h = view.height;
    let grant_pool = view.state.grant_pool_remaining() as u64;
    let treasury = view.state.treasury_balance() as u64;
    let escrow = view.state.simulated_payout_escrow() as u64;
    let mempool = st.mempool_len.load(Ordering::Relaxed);
    let bft_round = st.bft_round.load(Ordering::Relaxed);
    let wal_writes = st.wal_writes_total.load(Ordering::Relaxed);
    let http_limited = st.http_rate_limited_total.load(Ordering::Relaxed);
    let http_shed = st.http_shed_total.load(Ordering::Relaxed);
    let peers = st.net_metrics.peers_connected.load(Ordering::Relaxed);
    let gossip_rx = st.net_metrics.gossip_rx_total.load(Ordering::Relaxed);
    let gossip_rej = st.net_metrics.gossip_rejected_total.load(Ordering::Relaxed);
    let sync_srv = st
        .net_metrics
        .sync_requests_served_total
        .load(Ordering::Relaxed);

    let body = format!(
        "# HELP bitblock_chain_height Current finalized Bits L1 block height\n\
         # TYPE bitblock_chain_height gauge\n\
         bitblock_chain_height {h}\n\
         # HELP bitblock_grant_pool_remaining Remaining Bits in TestnetGrantPool\n\
         # TYPE bitblock_grant_pool_remaining gauge\n\
         bitblock_grant_pool_remaining {grant_pool}\n\
         # HELP bitblock_treasury_balance Cumulative Bits held in platform treasury\n\
         # TYPE bitblock_treasury_balance gauge\n\
         bitblock_treasury_balance {treasury}\n\
         # HELP bitblock_simulated_payout_escrow Testnet-only simulated creator payout escrow\n\
         # TYPE bitblock_simulated_payout_escrow gauge\n\
         bitblock_simulated_payout_escrow {escrow}\n\
         # HELP bitblock_mempool_len Pending canonical Bits transactions in mempool\n\
         # TYPE bitblock_mempool_len gauge\n\
         bitblock_mempool_len {mempool}\n\
         # HELP bitblock_bft_round Current HotStuff consensus round\n\
         # TYPE bitblock_bft_round gauge\n\
         bitblock_bft_round {bft_round}\n\
         # HELP bitblock_peers_connected Connected libp2p peers\n\
         # TYPE bitblock_peers_connected gauge\n\
         bitblock_peers_connected {peers}\n\
         # HELP bitblock_wal_writes_total Durable SIGNER-OPS-1 watermark fsync commits\n\
         # TYPE bitblock_wal_writes_total counter\n\
         bitblock_wal_writes_total {wal_writes}\n\
         # HELP bitblock_gossip_rx_total Gossipsub packets received\n\
         # TYPE bitblock_gossip_rx_total counter\n\
         bitblock_gossip_rx_total {gossip_rx}\n\
         # HELP bitblock_gossip_rejected_total Invalid or rate-limited gossip packets rejected\n\
         # TYPE bitblock_gossip_rejected_total counter\n\
         bitblock_gossip_rejected_total {gossip_rej}\n\
         # HELP bitblock_sync_requests_served_total Unicast sync responses served\n\
         # TYPE bitblock_sync_requests_served_total counter\n\
         bitblock_sync_requests_served_total {sync_srv}\n\
         # HELP bitblock_http_rate_limited_total HTTP requests rejected with 429\n\
         # TYPE bitblock_http_rate_limited_total counter\n\
         bitblock_http_rate_limited_total {http_limited}\n\
         # HELP bitblock_http_shed_total HTTP requests shed with 503\n\
         # TYPE bitblock_http_shed_total counter\n\
         bitblock_http_shed_total {http_shed}\n"
    );
    (
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; version=0.0.4"),
            ),
            (
                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                HeaderValue::from_static("*"),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
        body,
    )
}

async fn handle_proof_head(State(st): State<Arc<SharedRpcState>>) -> axum::response::Response {
    let Some(_permit) = st.try_http_permit() else {
        return shed_503("rpc concurrency limit reached — retry shortly");
    };
    let view = st.read_view();
    let balance_root_hex = hex::encode(view.state.balance_root());
    let nonce_root_hex = hex::encode(view.state.nonce_root());
    let head_hash_hex = hex::encode(view.head_hash);
    let epoch = view.height / st.genesis.epoch_blocks.max(1);
    let treasury_bps = 10_000u16.saturating_sub(st.genesis.creator_split_bps);

    cors_json(
        StatusCode::OK,
        json!({
            "ok": true,
            "protocol": "bitblock-l1-v1",
            "chainId": st.genesis.chain_id,
            "genesis": hex::encode(st.genesis_id),
            "network": st.genesis.network_id,
            "lab": st.genesis.lab,
            "custodyLabel": BITS_TESTNET_CUSTODY_LABEL,
            "epochBlocks": st.genesis.epoch_blocks,
            "unit": {
                "name": "Bits",
                "symbol": "BITS",
                "baseUnit": 1,
                "creatorSplitBps": st.genesis.creator_split_bps,
                "treasurySplitBps": treasury_bps,
                "continuity": "bitblock-hotstuff-ed25519",
            },
            "systemActors": {
                "platformTreasury": st.genesis.platform_treasury_actor_hex,
                "grantPool": st.genesis.grant_pool_actor_hex,
                "payoutEscrow": st.genesis.payout_escrow_actor_hex,
            },
            "head": {
                "height": view.height,
                "hash": head_hash_hex,
                "root": balance_root_hex,
                "balanceRoot": balance_root_hex,
                "walletRoot": balance_root_hex,
                "nonceRoot": nonce_root_hex,
                "walletCount": view.state.account_count(),
                "grantPoolRemaining": view.state.grant_pool_remaining() as u64,
                "treasuryBalance": view.state.treasury_balance() as u64,
                "simulatedPayoutEscrow": view.state.simulated_payout_escrow() as u64,
                "genesisSupply": st.genesis.genesis_supply,
                "committeeEpoch": epoch,
                "l1": {
                    "height": view.height,
                    "head": head_hash_hex,
                },
                "qc": view.tail.as_ref().map(|b| &b.qc),
                "header": view.tail.as_ref().map(|b| &b.header),
            },
        }),
    )
}

#[derive(Deserialize)]
struct BlocksQuery {
    from: Option<u64>,
    limit: Option<usize>,
}

async fn handle_proof_blocks(
    State(st): State<Arc<SharedRpcState>>,
    Query(q): Query<BlocksQuery>,
) -> axum::response::Response {
    let Some(_permit) = st.try_http_permit() else {
        return shed_503("rpc concurrency limit reached — retry shortly");
    };
    let from = q.from.unwrap_or(1).max(1);
    let limit = q
        .limit
        .unwrap_or(MAX_BLOCKS_PAGE_LIMIT)
        .min(MAX_BLOCKS_PAGE_LIMIT);
    let height = st.read_view().height;

    let hot_summaries: Option<Vec<CachedBlockSummary>> = {
        let ring = st.recent_blocks.read().expect("rwlock");
        let oldest = ring.front().map(|e| e.height);
        if height == 0 {
            Some(Vec::new())
        } else if let Some(first_k) = oldest {
            if from >= first_k || first_k == 1 {
                let start_idx = ring.partition_point(|e| e.height < from);
                Some(ring.iter().skip(start_idx).take(limit).cloned().collect())
            } else {
                None
            }
        } else {
            None
        }
    };

    let summaries: Vec<CachedBlockSummary> = match hot_summaries {
        Some(v) => v,
        None => {
            let store = st.store.clone();
            let blocks = tokio::task::spawn_blocking(move || {
                store.list_blocks(from, limit).unwrap_or_default()
            })
            .await
            .unwrap_or_default();
            blocks.iter().map(build_block_summary).collect()
        }
    };

    let mut out = String::with_capacity(96 + summaries.len() * 256);
    out.push_str("{\"ok\":true,\"height\":");
    out.push_str(&height.to_string());
    out.push_str(",\"head\":");
    match summaries.last() {
        Some(last) => {
            out.push('"');
            out.push_str(&last.hash);
            out.push('"');
        }
        None => out.push_str("null"),
    }
    out.push_str(",\"blocks\":[");
    for (idx, s) in summaries.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push_str(&s.json);
    }
    out.push_str("]}");
    cors_raw_json(StatusCode::OK, out)
}

async fn handle_proof_block(
    State(st): State<Arc<SharedRpcState>>,
    Path(k): Path<u64>,
) -> axum::response::Response {
    let Some(_permit) = st.try_http_permit() else {
        return shed_503("rpc concurrency limit reached — retry shortly");
    };
    let view = st.read_view();
    let maybe_fb = if let Some(ref tail) = view.tail
        && tail.header.height == k
    {
        Some(tail.clone())
    } else {
        let store = st.store.clone();
        tokio::task::spawn_blocking(move || store.get_block(k).ok().flatten())
            .await
            .ok()
            .flatten()
    };
    match maybe_fb {
        Some(fb) => cors_json(
            StatusCode::OK,
            json!({
                "ok": true,
                "header": fb.header,
                "qc": fb.qc,
                "txsHex": fb.txs_hex,
                "receipts": fb.receipts,
            }),
        ),
        None => cors_json(
            StatusCode::NOT_FOUND,
            json!({ "ok": false, "reason": "block not found" }),
        ),
    }
}

async fn handle_proof_tx(
    State(st): State<Arc<SharedRpcState>>,
    Path(hash_hex): Path<String>,
) -> axum::response::Response {
    let Some(_permit) = st.try_http_permit() else {
        return shed_503("rpc concurrency limit reached — retry shortly");
    };
    let Ok(tx_hash) = decode_hex_32(&hash_hex) else {
        return cors_json(
            StatusCode::BAD_REQUEST,
            json!({ "ok": false, "reason": "tx hash must be 32-byte hex" }),
        );
    };
    let store = st.store.clone();
    let found = tokio::task::spawn_blocking(move || store.get_tx(&tx_hash).ok().flatten())
        .await
        .ok()
        .flatten();
    match found {
        Some((height, receipt)) => cors_json(
            StatusCode::OK,
            json!({
                "ok": true,
                "height": height,
                "txHash": hex::encode(tx_hash),
                "receipt": receipt,
            }),
        ),
        None => cors_json(
            StatusCode::NOT_FOUND,
            json!({ "ok": false, "reason": "transaction not found" }),
        ),
    }
}

/// Cross-Session Amendment 1: accepts either a canonical 64-char hex `ActorIdV1` OR a raw 64-char
/// hex Ed25519 K1 `pubkeyHex` and always returns canonical `actorId` + `AUTH-COMMIT-1` Domain 3
/// `DomainPatriciaProofV1` inclusion or non-membership proof.
async fn handle_proof_wallet(
    State(st): State<Arc<SharedRpcState>>,
    Path(addr): Path<String>,
) -> axum::response::Response {
    let Some(_permit) = st.try_http_permit() else {
        return shed_503("rpc concurrency limit reached — retry shortly");
    };
    let Ok(raw_32) = decode_hex_32(&addr) else {
        return cors_json(
            StatusCode::BAD_REQUEST,
            json!({ "ok": false, "reason": "address must be 64-char hex ActorIdV1 or Ed25519 pubkey" }),
        );
    };
    let view = st.read_view();
    let Ok(proof) = view.state.prove_wallet(raw_32) else {
        return cors_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({ "ok": false, "reason": "failed to build wallet Patricia proof" }),
        );
    };
    let Ok(proof_bytes) = proof.patricia_proof.encode_canonical() else {
        return cors_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({ "ok": false, "reason": "failed to encode wallet Patricia proof" }),
        );
    };
    let proof_hex = hex::encode(proof_bytes);
    let pk_hex = proof.active_pubkey.map(|pk| hex::encode(pk.as_bytes()));
    cors_json(
        StatusCode::OK,
        json!({
            "ok": true,
            "actorId": hex::encode(proof.actor_id.as_bytes()),
            "ed25519Hex": pk_hex,
            "pubkeyHex": pk_hex,
            "balance": proof.balance_bits as u64,
            "seq": proof.seq,
            "root": hex::encode(proof.balance_root),
            "proofHex": proof_hex,
            "domainId": 3,
            "height": view.height,
        }),
    )
}

async fn handle_proof_validators(
    State(st): State<Arc<SharedRpcState>>,
) -> axum::response::Response {
    let Some(_permit) = st.try_http_permit() else {
        return shed_503("rpc concurrency limit reached — retry shortly");
    };
    let view = st.read_view();
    let epoch = view.height / st.genesis.epoch_blocks.max(1);
    let committee = st.genesis.committee_json();
    let validators: Vec<Value> = st
        .genesis
        .validators
        .iter()
        .map(|v| {
            json!({
                "validator_id": v.validator_id_hex.to_lowercase(),
                "proposal_pubkey_hex": v.proposal_pubkey_hex.to_lowercase(),
                "vote_pubkey_hex": v.vote_pubkey_hex.to_lowercase(),
                "ed25519_pk_hex": v.vote_pubkey_hex.to_lowercase(),
                "power": v.power,
            })
        })
        .collect();
    cors_json(
        StatusCode::OK,
        json!({
            "ok": true,
            "protocol": "bitblock-l1-v1",
            "chainId": st.genesis.chain_id,
            "committeeEpoch": epoch,
            "validatorScheduleRoot": hex::encode(st.genesis.validator_schedule_root()),
            "committee": committee,
            "allValidators": validators,
        }),
    )
}

#[derive(Deserialize)]
struct TxSubmitBody {
    #[serde(alias = "tlv_hex")]
    tx_hex: String,
}

async fn handle_submit_tx(
    State(st): State<Arc<SharedRpcState>>,
    Json(body): Json<TxSubmitBody>,
) -> axum::response::Response {
    let Some(_permit) = st.try_http_permit() else {
        return shed_503("rpc concurrency limit reached — retry shortly");
    };
    let Ok(raw_bytes) = hex::decode(body.tx_hex.trim()) else {
        return cors_json(
            StatusCode::BAD_REQUEST,
            json!({ "ok": false, "reason": "invalid tx_hex" }),
        );
    };
    let tx = match BitsSignedTxV1::decode_canonical(&raw_bytes) {
        Ok(t) => t,
        Err(e) => {
            return cors_json(
                StatusCode::BAD_REQUEST,
                json!({ "ok": false, "reason": e.to_string() }),
            );
        }
    };
    if let Err(e) = tx.verify_signature() {
        return cors_json(
            StatusCode::BAD_REQUEST,
            json!({ "ok": false, "reason": format!("signature verification failed: {e}") }),
        );
    }

    let (reply_tx, reply_rx) = oneshot::channel();
    match st.action_tx.try_send(RpcAction::SubmitTx {
        tx,
        raw_bytes,
        reply: reply_tx,
    }) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_)) => {
            st.http_shed_total.fetch_add(1, Ordering::Relaxed);
            return shed_503("mempool ingress queue full — retry shortly");
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            return cors_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "ok": false, "reason": "mempool closed" }),
            );
        }
    }
    match tokio::time::timeout(RPC_ACTION_TIMEOUT, reply_rx).await {
        Ok(Ok(Ok(tx_hash))) => cors_json(
            StatusCode::ACCEPTED,
            json!({ "ok": true, "txHash": tx_hash }),
        ),
        Ok(Ok(Err(reason))) => cors_json(
            StatusCode::BAD_REQUEST,
            json!({ "ok": false, "reason": reason }),
        ),
        Ok(Err(_)) => cors_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({ "ok": false, "reason": "node busy" }),
        ),
        Err(_) => {
            st.http_shed_total.fetch_add(1, Ordering::Relaxed);
            shed_503("node overloaded — retry shortly")
        }
    }
}

#[derive(Deserialize)]
struct FaucetBody {
    #[serde(alias = "pubkeyHex", alias = "ed25519Hex")]
    pubkey_hex: String,
    #[serde(default = "default_faucet_amount")]
    amount: u64,
}

fn default_faucet_amount() -> u64 {
    50
}

async fn handle_faucet(
    State(st): State<Arc<SharedRpcState>>,
    Json(body): Json<FaucetBody>,
) -> axum::response::Response {
    let Some(_permit) = st.try_http_permit() else {
        return shed_503("rpc concurrency limit reached — retry shortly");
    };
    if !st.genesis.lab {
        return cors_json(
            StatusCode::FORBIDDEN,
            json!({ "ok": false, "reason": "testnet faucet structurally disabled when genesis.lab == false" }),
        );
    }
    let pk_hex = body.pubkey_hex.trim().to_lowercase();
    let Ok(recipient_pubkey) = decode_hex_32(&pk_hex) else {
        return cors_json(
            StatusCode::BAD_REQUEST,
            json!({ "ok": false, "reason": "pubkey_hex must be 32-byte Ed25519 hex" }),
        );
    };
    let max_drip = st.genesis.max_faucet_grant_bits.max(1);
    let amt = body.amount.clamp(1, max_drip);

    let now = Instant::now();
    {
        let mut global = st.faucet_global_window.write().await;
        while let Some(&front) = global.front() {
            if now.duration_since(front) >= FAUCET_GLOBAL_WINDOW {
                global.pop_front();
            } else {
                break;
            }
        }
        if global.len() >= FAUCET_GLOBAL_LIMIT {
            st.http_rate_limited_total.fetch_add(1, Ordering::Relaxed);
            return cors_json(
                StatusCode::TOO_MANY_REQUESTS,
                json!({ "ok": false, "reason": "global faucet rate limit active — retry shortly" }),
            );
        }
        let mut cd = st.faucet_cooldowns.write().await;
        if let Some(prev) = cd.get(&pk_hex)
            && now.duration_since(*prev) < FAUCET_PER_KEY_COOLDOWN
        {
            st.http_rate_limited_total.fetch_add(1, Ordering::Relaxed);
            return cors_json(
                StatusCode::TOO_MANY_REQUESTS,
                json!({ "ok": false, "reason": "faucet cooldown active — retry shortly" }),
            );
        }
        if cd.len() >= FAUCET_COOLDOWN_MAX_ENTRIES {
            cd.retain(|_, ts| now.duration_since(*ts) < FAUCET_PER_KEY_COOLDOWN);
            if cd.len() >= FAUCET_COOLDOWN_MAX_ENTRIES {
                cd.clear();
            }
        }
        cd.insert(pk_hex, now);
        global.push_back(now);
    }

    let (reply_tx, reply_rx) = oneshot::channel();
    match st.action_tx.try_send(RpcAction::RequestFaucet {
        recipient_pubkey,
        amount_bits: amt,
        reply: reply_tx,
    }) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_)) => {
            st.http_shed_total.fetch_add(1, Ordering::Relaxed);
            return shed_503("node action queue full — retry shortly");
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            return cors_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "ok": false, "reason": "node loop closed" }),
            );
        }
    }
    match tokio::time::timeout(RPC_ACTION_TIMEOUT, reply_rx).await {
        Ok(Ok(Ok(tx_hash))) => cors_json(
            StatusCode::ACCEPTED,
            json!({
                "ok": true,
                "txHash": tx_hash,
                "requestedAmount": amt,
                "source": "TestnetGrantPool (zero runtime minting)",
            }),
        ),
        Ok(Ok(Err(reason))) => cors_json(
            StatusCode::BAD_REQUEST,
            json!({ "ok": false, "reason": reason }),
        ),
        Ok(Err(_)) => cors_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({ "ok": false, "reason": "node busy" }),
        ),
        Err(_) => {
            st.http_shed_total.fetch_add(1, Ordering::Relaxed);
            shed_503("node overloaded — retry shortly")
        }
    }
}
