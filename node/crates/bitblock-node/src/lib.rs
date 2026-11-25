//! Standalone Bitblock L1 Bits testnet validator & full-node daemon library.
//!
//! Architecture:
//! - Isolated inside `node/` (root `Cargo.lock` stays frozen)
//! - Deterministic `BitsEconomyStateV1` (`bits_economy_1.rs`) over `AUTH-COMMIT-1` Patricia tries
//! - 2-phase Ed25519 HotStuff BFT consensus producing `BitsBlockHeaderJson` + `BitsQuorumCertJson`
//!   verified byte-for-byte by `bitblock-light-wasm`
//! - `SIGNER-OPS-1` kernel-locked (`flock` / `share_mode(0)`) monotonic anti-double-sign boundary
//! - Crash-safe ACID `redb` persistence (`bitblock-store`) + hardened `libp2p` mesh (`bitblock-net`)
//! - Hardened `axum` CORS HTTP proof doors (`rpc.rs`) with `503` shedding, `413` 64 KiB body cap,
//!   `<= 100` block page cap, Amendment 1 dual `ActorIdV1`/`pubkeyHex` wallet proofs, and `lab == false`
//!   faucet refusal.

pub mod keystore;
pub mod rpc;

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::{Duration, Instant};

use bitblock_light_wasm::{
    BitsBlockHeaderJson, BitsQcSignatureJson, BitsQuorumCertJson, build_qc_commit_preimage,
    check_header_qc_slice, compute_bits_header_hash,
};
use bitblock_net::{
    BftWireMsg, MAX_SYNC_BLOCKS, NetCommand, NetEvent, P2pConfig, WirePacket, spawn_p2p,
};
use bitblock_research::bits_economy_1::{
    BITS_1_VERSION, BitsEconomyStateV1, BitsGrantKindV1, BitsSignedTxV1, BitsTestnetGrantV1,
    BitsTxPayloadV1, derive_default_bits_actor_id,
};
use bitblock_research::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, HashSuiteIdV1, SignatureSuiteIdV1,
    SigningDomainTagV1, SigningPreimageV1, verify_preimage_strict,
};
use bitblock_research::identity_d1::ActorIdV1;
use bitblock_store::{
    BitsGenesisFile, ChainStore, FinalizedBitsBlock, TxReceiptRecord, ValidatorGenesisEntry,
    ValidatorSignerBoundary, compute_tx_root, decode_hex_32, proposal_payload_digest,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::{RwLock, Semaphore, mpsc, oneshot};
use tracing::{info, warn};

use crate::keystore::{
    EncryptedBitblockKeystore, ValidatorSecretMaterial, decode_seed_32, decrypt_validator_keystore,
    encrypt_validator_keystore,
};
use crate::rpc::{
    CachedReadView, HOT_BLOCK_SUMMARY_CAP, MAX_INFLIGHT_HTTP, RPC_ACTION_QUEUE_CAP, RpcAction,
    SharedRpcState, build_block_summary, build_router,
};

const MAX_MEMPOOL_TXS: usize = 2_048;
const MAX_BLOCK_TXS: usize = 64;

/// Per-node JSON configuration file written by `init-testnet` and loaded by `run`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub node_id: usize,
    pub data_dir: PathBuf,
    pub genesis_path: PathBuf,
    #[serde(default)]
    pub validator_keystore_path: Option<PathBuf>,
    /// Legacy plaintext dev fields (refused unless `--unsafe-dev-keys` is passed).
    #[serde(default)]
    pub validator_id_hex: String,
    #[serde(default)]
    pub proposal_seed_hex: String,
    #[serde(default)]
    pub vote_seed_hex: String,
    #[serde(default)]
    pub p2p_seed_hex: String,
    #[serde(default)]
    pub faucet_relayer_seed_hex: Option<String>,
    pub p2p_listen: Vec<String>,
    pub bootstrap_peers: Vec<String>,
    pub rpc_listen: String,
    pub block_ms: u64,
}

/// Summary returned by [`init_testnet`] for programmatic tests and CLI output.
#[derive(Debug, Clone)]
pub struct InitTestnetResult {
    pub genesis_path: PathBuf,
    pub genesis_id_hex: String,
    pub faucet_relayer_pubkey_hex: String,
    pub faucet_relayer_seed_hex: String,
    pub node_configs: Vec<PathBuf>,
    pub password_files: Vec<PathBuf>,
}

/// Generates a reproducible multi-validator localhost shadow testnet with role-separated
/// Ed25519 keys, Argon2id/AES-256-GCM encrypted keystores, and a shared `genesis.json`.
#[allow(clippy::too_many_arguments)]
pub fn init_testnet(
    dir: PathBuf,
    validators: usize,
    base_p2p_port: u16,
    base_rpc_port: u16,
    block_ms: u64,
    epoch_blocks: Option<u64>,
    keystore_password: String,
    lab: bool,
    extra_grant_relayers_hex: Vec<String>,
) -> Result<InitTestnetResult, String> {
    if validators == 0 {
        return Err("validators must be >= 1".into());
    }
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    // Shared testnet faucet relayer key (lab == true only).
    let mut faucet_seed = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut faucet_seed);
    let faucet_pk =
        CanonicalEd25519PublicKeyV1::from_secret_seed(&faucet_seed).map_err(|e| e.to_string())?;
    let faucet_pk_hex = hex::encode(faucet_pk.as_bytes());
    let faucet_seed_hex = hex::encode(faucet_seed);

    let mut grant_relayers = vec![faucet_pk_hex.clone()];
    for extra in extra_grant_relayers_hex {
        let trimmed = extra.trim().to_lowercase();
        if !trimmed.is_empty() && !grant_relayers.contains(&trimmed) {
            grant_relayers.push(trimmed);
        }
    }

    let mut val_entries = Vec::with_capacity(validators);
    let mut secrets = Vec::with_capacity(validators);
    for i in 0..validators {
        let mut prop_seed = [0u8; 32];
        let mut vote_seed = [0u8; 32];
        let mut p2p_seed = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut prop_seed);
        rand::rngs::OsRng.fill_bytes(&mut vote_seed);
        rand::rngs::OsRng.fill_bytes(&mut p2p_seed);

        let prop_pk =
            CanonicalEd25519PublicKeyV1::from_secret_seed(&prop_seed).map_err(|e| e.to_string())?;
        let vote_pk =
            CanonicalEd25519PublicKeyV1::from_secret_seed(&vote_seed).map_err(|e| e.to_string())?;
        let val_id_bytes: [u8; 32] = {
            let mut h = Sha256::new();
            h.update(b"bb.bits1.validator_id.v1");
            h.update((i as u32).to_be_bytes());
            h.update(vote_pk.as_bytes());
            h.finalize().into()
        };
        let val_id_hex = hex::encode(val_id_bytes);

        val_entries.push(ValidatorGenesisEntry {
            validator_id_hex: val_id_hex.clone(),
            proposal_pubkey_hex: hex::encode(prop_pk.as_bytes()),
            vote_pubkey_hex: hex::encode(vote_pk.as_bytes()),
            power: 100,
        });
        secrets.push(ValidatorSecretMaterial {
            validator_id_hex: val_id_hex,
            proposal_seed_hex: hex::encode(prop_seed),
            vote_seed_hex: hex::encode(vote_seed),
            p2p_seed_hex: hex::encode(p2p_seed),
            faucet_relayer_seed_hex: if lab {
                Some(faucet_seed_hex.clone())
            } else {
                None
            },
        });
    }

    let mut genesis = BitsGenesisFile::testnet_default(val_entries, grant_relayers, block_ms);
    genesis.lab = lab;
    if let Some(eb) = epoch_blocks {
        genesis.epoch_blocks = eb.max(1);
    }
    let genesis_id = genesis.genesis_id().map_err(|e| e.to_string())?;
    let genesis_path = dir.join("genesis.json");
    fs::write(
        &genesis_path,
        serde_json::to_string_pretty(&genesis).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let mut node_configs = Vec::with_capacity(validators);
    let mut password_files = Vec::with_capacity(validators);

    for (i, mat) in secrets.iter().enumerate() {
        let node_dir = dir.join(format!("node-{i}"));
        fs::create_dir_all(&node_dir).map_err(|e| e.to_string())?;

        let ks = encrypt_validator_keystore(mat, &keystore_password).map_err(|e| e.to_string())?;
        let ks_path = node_dir.join("validator-keystore.json");
        fs::write(
            &ks_path,
            serde_json::to_string_pretty(&ks).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

        let pass_path = node_dir.join("keystore.pass");
        fs::write(&pass_path, format!("{keystore_password}\n")).map_err(|e| e.to_string())?;

        let p2p_port = base_p2p_port + (i as u16);
        let rpc_port = base_rpc_port + (i as u16);
        let bootstrap_peers: Vec<String> = (0..validators)
            .filter(|&j| j != i)
            .map(|j| format!("/ip4/127.0.0.1/tcp/{}", base_p2p_port + (j as u16)))
            .collect();

        let cfg = NodeConfig {
            node_id: i,
            data_dir: node_dir.clone(),
            genesis_path: genesis_path.clone(),
            validator_keystore_path: Some(ks_path),
            validator_id_hex: String::new(),
            proposal_seed_hex: String::new(),
            vote_seed_hex: String::new(),
            p2p_seed_hex: String::new(),
            faucet_relayer_seed_hex: None,
            p2p_listen: vec![format!("/ip4/127.0.0.1/tcp/{p2p_port}")],
            bootstrap_peers,
            rpc_listen: format!("127.0.0.1:{rpc_port}"),
            block_ms,
        };
        let cfg_path = node_dir.join("node.json");
        fs::write(
            &cfg_path,
            serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

        node_configs.push(cfg_path);
        password_files.push(pass_path);
    }

    Ok(InitTestnetResult {
        genesis_path,
        genesis_id_hex: hex::encode(genesis_id),
        faucet_relayer_pubkey_hex: faucet_pk_hex,
        faucet_relayer_seed_hex: faucet_seed_hex,
        node_configs,
        password_files,
    })
}

fn load_validator_secrets(
    cfg: &NodeConfig,
    password_file: Option<&Path>,
    unsafe_dev_keys: bool,
) -> Result<ValidatorSecretMaterial, String> {
    if let Some(ref ks_path) = cfg.validator_keystore_path {
        let raw = fs::read_to_string(ks_path).map_err(|e| {
            format!(
                "failed reading validator keystore {}: {e}",
                ks_path.display()
            )
        })?;
        let ks: EncryptedBitblockKeystore =
            serde_json::from_str(&raw).map_err(|e| format!("invalid keystore json: {e}"))?;
        let pass = if let Some(pf) = password_file {
            fs::read_to_string(pf)
                .map_err(|e| format!("failed reading password file {}: {e}", pf.display()))?
                .trim()
                .to_string()
        } else if let Ok(env_pass) = std::env::var("BB_KEYSTORE_PASSWORD") {
            env_pass
        } else {
            return Err(
                "encrypted validator keystore requires --password-file or BB_KEYSTORE_PASSWORD"
                    .into(),
            );
        };
        return decrypt_validator_keystore(&ks, &pass).map_err(|e| e.to_string());
    }
    if !unsafe_dev_keys {
        return Err(
            "plaintext seeds in node.json are refused unless --unsafe-dev-keys is passed".into(),
        );
    }
    Ok(ValidatorSecretMaterial {
        validator_id_hex: cfg.validator_id_hex.clone(),
        proposal_seed_hex: cfg.proposal_seed_hex.clone(),
        vote_seed_hex: cfg.vote_seed_hex.clone(),
        p2p_seed_hex: cfg.p2p_seed_hex.clone(),
        faucet_relayer_seed_hex: cfg.faucet_relayer_seed_hex.clone(),
    })
}

struct PendingProposal {
    header: BitsBlockHeaderJson,
    txs_hex: Vec<String>,
    receipts: Vec<TxReceiptRecord>,
    next_state: BitsEconomyStateV1,
    block_hash: [u8; 32],
}

/// Runs a live Bitblock L1 validator node until `shutdown_rx` fires (or forever if `None`).
pub async fn run_node_with_shutdown(
    config_path: PathBuf,
    password_file: Option<PathBuf>,
    unsafe_dev_keys: bool,
    mut shutdown_rx: Option<oneshot::Receiver<()>>,
) -> Result<(), String> {
    let cfg_raw = fs::read_to_string(&config_path)
        .map_err(|e| format!("failed reading config {}: {e}", config_path.display()))?;
    let cfg: NodeConfig =
        serde_json::from_str(&cfg_raw).map_err(|e| format!("invalid node.json: {e}"))?;

    let genesis_raw = fs::read_to_string(&cfg.genesis_path)
        .map_err(|e| format!("failed reading genesis {}: {e}", cfg.genesis_path.display()))?;
    let genesis: BitsGenesisFile =
        serde_json::from_str(&genesis_raw).map_err(|e| format!("invalid genesis.json: {e}"))?;
    let genesis_id = genesis.genesis_id().map_err(|e| e.to_string())?;
    let genesis_hex = hex::encode(genesis_id);
    let schedule_root = genesis.validator_schedule_root();
    let schedule_root_hex = hex::encode(schedule_root);
    let committee_json = genesis.committee_json();
    let total_power: u128 = genesis.validators.iter().map(|v| u128::from(v.power)).sum();
    if total_power == 0 {
        return Err("genesis committee total power must be > 0".into());
    }

    let secrets = load_validator_secrets(&cfg, password_file.as_deref(), unsafe_dev_keys)?;
    let val_id =
        ActorIdV1::from_bytes(decode_hex_32(&secrets.validator_id_hex).map_err(|e| e.to_string())?);
    let prop_seed = decode_seed_32(&secrets.proposal_seed_hex).map_err(|e| e.to_string())?;
    let vote_seed = decode_seed_32(&secrets.vote_seed_hex).map_err(|e| e.to_string())?;
    let p2p_seed = decode_seed_32(&secrets.p2p_seed_hex).map_err(|e| e.to_string())?;
    let faucet_seed: Option<[u8; 32]> = match &secrets.faucet_relayer_seed_hex {
        Some(h) if !h.trim().is_empty() => Some(decode_seed_32(h).map_err(|e| e.to_string())?),
        _ => None,
    };

    fs::create_dir_all(&cfg.data_dir).map_err(|e| e.to_string())?;
    let store = ChainStore::open(cfg.data_dir.join("chain.redb")).map_err(|e| e.to_string())?;
    let (mut state, mut head_height, mut head_hash, tail) =
        store.init_or_replay(&genesis).map_err(|e| e.to_string())?;

    // Open kernel-locked SIGNER-OPS-1 anti-double-sign boundary BEFORE joining consensus!
    let mut signer = ValidatorSignerBoundary::open_or_create(
        cfg.data_dir.join("signer"),
        genesis.chain_id,
        genesis_id,
        val_id,
        prop_seed,
        vote_seed,
    )
    .map_err(|e| e.to_string())?;

    let local_val_idx = genesis.validators.iter().position(|v| {
        v.validator_id_hex
            .eq_ignore_ascii_case(&signer.validator_id_hex())
    });

    let p2p = spawn_p2p(P2pConfig {
        listen_addrs: cfg.p2p_listen.clone(),
        bootstrap_peers: cfg.bootstrap_peers.clone(),
        genesis_hex: genesis_hex.clone(),
        key_seed: p2p_seed,
    })
    .map_err(|e| e.to_string())?;

    // Pre-populate hot block summary ring from persisted store.
    let mut recent_ring = VecDeque::with_capacity(HOT_BLOCK_SUMMARY_CAP);
    if head_height > 0 {
        let start = head_height
            .saturating_sub(HOT_BLOCK_SUMMARY_CAP as u64 - 1)
            .max(1);
        if let Ok(blocks) = store.list_blocks(start, HOT_BLOCK_SUMMARY_CAP) {
            for fb in &blocks {
                recent_ring.push_back(build_block_summary(fb));
            }
        }
    }

    let (action_tx, mut action_rx) = mpsc::channel::<RpcAction>(RPC_ACTION_QUEUE_CAP);
    let shared = Arc::new(SharedRpcState {
        genesis: genesis.clone(),
        genesis_id,
        store: store.clone(),
        cached_view: StdRwLock::new(Arc::new(CachedReadView {
            state: state.clone(),
            height: head_height,
            head_hash,
            tail,
        })),
        recent_blocks: StdRwLock::new(recent_ring),
        http_permits: Arc::new(Semaphore::new(MAX_INFLIGHT_HTTP)),
        peer_count: RwLock::new(0),
        mempool_len: AtomicU64::new(0),
        bft_round: AtomicU64::new(0),
        wal_writes_total: AtomicU64::new(0),
        http_rate_limited_total: AtomicU64::new(0),
        http_shed_total: AtomicU64::new(0),
        net_metrics: p2p.metrics.clone(),
        action_tx,
        validator_id_hex: signer.validator_id_hex(),
        vote_pubkey_hex: signer.vote_pubkey_hex().to_string(),
        faucet_cooldowns: RwLock::new(HashMap::new()),
        faucet_global_window: RwLock::new(VecDeque::new()),
    });

    // Spawn HTTP proof-door server.
    let rpc_addr: SocketAddr = cfg
        .rpc_listen
        .parse()
        .map_err(|e| format!("invalid rpc_listen {}: {e}", cfg.rpc_listen))?;
    let listener = tokio::net::TcpListener::bind(rpc_addr)
        .await
        .map_err(|e| format!("failed binding rpc {rpc_addr}: {e}"))?;
    let router = build_router(shared.clone());
    let (http_stop_tx, http_stop_rx) = oneshot::channel::<()>();
    let http_task = tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = http_stop_rx.await;
            })
            .await;
    });

    info!(
        node_id = cfg.node_id,
        height = head_height,
        rpc = %cfg.rpc_listen,
        peer_id = %p2p.local_peer_id,
        "bitblock-node started (Testnet/Devnet lab={})",
        genesis.lab
    );

    let mut evt_rx = p2p.evt_rx;
    let cmd_tx = p2p.cmd_tx;

    let mut mempool: Vec<(BitsSignedTxV1, Vec<u8>, [u8; 32])> = Vec::new();
    let mut mempool_hashes: HashSet<[u8; 32]> = HashSet::new();

    let mut round: u32 = 0;
    let mut round_started = Instant::now();
    let mut proposed_rounds: HashSet<(u64, u32)> = HashSet::new();
    let mut voted_rounds: HashSet<(u64, u32)> = HashSet::new();
    let mut pending_proposals: HashMap<(u64, u32, [u8; 32]), PendingProposal> = HashMap::new();
    let mut votes: HashMap<(u64, u32, [u8; 32]), BTreeMap<String, BitsQcSignatureJson>> =
        HashMap::new();

    let block_interval = Duration::from_millis(cfg.block_ms.max(150));
    let round_timeout = Duration::from_millis(cfg.block_ms.max(150).saturating_mul(3).max(900));
    let mut tick = tokio::time::interval(Duration::from_millis((cfg.block_ms / 4).clamp(60, 250)));
    let mut last_propose_attempt = Instant::now() - block_interval;
    let mut last_sync_req = Instant::now() - Duration::from_secs(5);

    let committee_map: HashMap<String, &ValidatorGenesisEntry> = genesis
        .validators
        .iter()
        .map(|v| (v.validator_id_hex.to_lowercase(), v))
        .collect();

    loop {
        tokio::select! {
            _ = async {
                if let Some(ref mut rx) = shutdown_rx {
                    let _ = rx.await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                info!(node_id = cfg.node_id, height = head_height, "bitblock-node shutting down gracefully");
                break;
            }
            _ = tick.tick() => {
                let next_height = head_height + 1;
                let epoch = next_height / genesis.epoch_blocks.max(1);

                // View-change round advance if current leader is silent/offline.
                if round_started.elapsed() >= round_timeout {
                    round = round.saturating_add(1);
                    round_started = Instant::now();
                    shared.bft_round.store(u64::from(round), Ordering::Relaxed);
                    if last_sync_req.elapsed() >= Duration::from_millis(500) {
                        let _ = cmd_tx.send(NetCommand::RequestSync {
                            from_height: next_height,
                            max_blocks: MAX_SYNC_BLOCKS,
                        });
                        last_sync_req = Instant::now();
                    }
                }

                // If we are the deterministic leader for (next_height, round), propose!
                if let Some(my_idx) = local_val_idx {
                    let proposer_idx =
                        ((next_height as usize).wrapping_add(round as usize)) % genesis.validators.len();
                    if proposer_idx == my_idx
                        && !proposed_rounds.contains(&(next_height, round))
                        && (last_propose_attempt.elapsed() >= block_interval || round > 0)
                    {
                        last_propose_attempt = Instant::now();
                        proposed_rounds.insert((next_height, round));

                        let mut sim = state.clone();
                        let mut chosen_hex = Vec::new();
                        let mut chosen_hashes = Vec::new();
                        let mut chosen_receipts = Vec::new();

                        for (tx, raw, _) in &mempool {
                            if chosen_hex.len() >= MAX_BLOCK_TXS {
                                break;
                            }
                            if let Ok(rcpt) = sim.apply_signed_tx(tx, next_height) {
                                chosen_hashes.push(rcpt.tx_hash);
                                chosen_hex.push(hex::encode(raw));
                                chosen_receipts.push(TxReceiptRecord::from_committed(tx, &rcpt));
                            }
                        }

                        let tx_root = compute_tx_root(&chosen_hashes);
                        let header = BitsBlockHeaderJson {
                            chain_id: genesis.chain_id,
                            genesis_id: genesis_hex.clone(),
                            height: next_height,
                            round,
                            epoch,
                            parent_hash: hex::encode(head_hash),
                            balance_root: hex::encode(sim.balance_root()),
                            nonce_root: hex::encode(sim.nonce_root()),
                            tx_root: hex::encode(tx_root),
                            validator_schedule_root: schedule_root_hex.clone(),
                        };

                        if let Ok(bh) = compute_bits_header_hash(&header) {
                            match signer.sign_proposal_durable(epoch, next_height, round, bh) {
                                Ok((prop_sig, _)) => {
                                    shared.wal_writes_total.fetch_add(1, Ordering::Relaxed);
                                    let msg = BftWireMsg::Proposal {
                                        height: next_height,
                                        round,
                                        epoch,
                                        proposer_id: signer.validator_id_hex(),
                                        proposal_pubkey_hex: signer.proposal_pubkey_hex().to_string(),
                                        header: header.clone(),
                                        txs_hex: chosen_hex.clone(),
                                        sig_hex: hex::encode(prop_sig.as_bytes()),
                                    };
                                    let _ = cmd_tx.send(NetCommand::BroadcastBft(msg));

                                    pending_proposals.insert(
                                        (next_height, round, bh),
                                        PendingProposal {
                                            header,
                                            txs_hex: chosen_hex,
                                            receipts: chosen_receipts,
                                            next_state: sim,
                                            block_hash: bh,
                                        },
                                    );

                                    // Proposer immediately casts its own SIGNER-OPS-1 commit vote.
                                    if voted_rounds.insert((next_height, round))
                                        && let Ok((vote_sig, _)) =
                                            signer.sign_qc_commit_durable(epoch, next_height, round, bh)
                                    {
                                        shared.wal_writes_total.fetch_add(1, Ordering::Relaxed);
                                        let sig_entry = BitsQcSignatureJson {
                                            validator_id: signer.validator_id_hex(),
                                            vote_pubkey_hex: signer.vote_pubkey_hex().to_string(),
                                            sig_hex: hex::encode(vote_sig.as_bytes()),
                                        };
                                        let _ = cmd_tx.send(NetCommand::BroadcastBft(BftWireMsg::Vote {
                                            height: next_height,
                                            round,
                                            epoch,
                                            block_hash: hex::encode(bh),
                                            validator_id: sig_entry.validator_id.clone(),
                                            vote_pubkey_hex: sig_entry.vote_pubkey_hex.clone(),
                                            sig_hex: sig_entry.sig_hex.clone(),
                                        }));
                                        votes
                                            .entry((next_height, round, bh))
                                            .or_default()
                                            .insert(sig_entry.validator_id.clone(), sig_entry);
                                        try_finalize(
                                            next_height,
                                            round,
                                            bh,
                                            &genesis,
                                            &committee_map,
                                            &committee_json,
                                            total_power,
                                            &mut pending_proposals,
                                            &mut votes,
                                            &mut proposed_rounds,
                                            &mut voted_rounds,
                                            &mut state,
                                            &mut head_height,
                                            &mut head_hash,
                                            &mut round,
                                            &mut round_started,
                                            &mut mempool,
                                            &mut mempool_hashes,
                                            &store,
                                            &shared,
                                            &cmd_tx,
                                        );
                                    }
                                }
                                Err(e) => {
                                    warn!(node_id = cfg.node_id, height = next_height, round, "signer refused proposal: {e}");
                                }
                            }
                        }
                    }
                }
            }
            Some(action) = action_rx.recv() => {
                match action {
                    RpcAction::SubmitTx { tx, raw_bytes, reply } => {
                        let res = admit_tx_to_mempool(
                            tx,
                            raw_bytes,
                            &state,
                            head_height + 1,
                            &mut mempool,
                            &mut mempool_hashes,
                            &shared,
                            &cmd_tx,
                            true,
                        );
                        let _ = reply.send(res);
                    }
                    RpcAction::RequestFaucet { recipient_pubkey, amount_bits, reply } => {
                        if !genesis.lab {
                            let _ = reply.send(Err("faucet disabled when lab == false".into()));
                            continue;
                        }
                        let Some(seed) = faucet_seed else {
                            let _ = reply.send(Err("node has no testnet faucet relayer key".into()));
                            continue;
                        };
                        let res = (|| -> Result<String, String> {
                            let pk = CanonicalEd25519PublicKeyV1::from_bytes(recipient_pubkey)
                                .map_err(|e| e.to_string())?;
                            let recipient_actor = derive_default_bits_actor_id(genesis.chain_id, &pk)
                                .map_err(|e| e.to_string())?;
                            let grant_pool_actor = state.config().grant_pool_actor;
                            let mut next_seq = state.nonce_of(&grant_pool_actor);
                            for (m_tx, _, _) in &mempool {
                                if m_tx.sender_actor == grant_pool_actor && m_tx.seq > next_seq {
                                    next_seq = m_tx.seq;
                                }
                            }
                            next_seq = next_seq.saturating_add(1);
                            let mut grant_id = [0u8; 32];
                            rand::rngs::OsRng.fill_bytes(&mut grant_id);
                            let tx = BitsSignedTxV1::sign_with_seed(
                                genesis.chain_id,
                                genesis_id,
                                grant_pool_actor,
                                &seed,
                                next_seq,
                                head_height + 200,
                                BitsTxPayloadV1::TestnetGrant(BitsTestnetGrantV1 {
                                    recipient: recipient_actor,
                                    recipient_pubkey: pk,
                                    amount_bits: u128::from(amount_bits),
                                    grant_kind: BitsGrantKindV1::Faucet,
                                    grant_id,
                                }),
                            )
                            .map_err(|e| e.to_string())?;
                            let raw = tx.encode_canonical().map_err(|e| e.to_string())?;
                            admit_tx_to_mempool(
                                tx,
                                raw,
                                &state,
                                head_height + 1,
                                &mut mempool,
                                &mut mempool_hashes,
                                &shared,
                                &cmd_tx,
                                true,
                            )
                        })();
                        let _ = reply.send(res);
                    }
                }
            }
            Some(evt) = evt_rx.recv() => {
                match evt {
                    NetEvent::PeerConnected(_) => {
                        let c = p2p.metrics.peers_connected.load(Ordering::Relaxed) as usize;
                        *shared.peer_count.write().await = c;
                        let _ = cmd_tx.send(NetCommand::RequestSync {
                            from_height: head_height + 1,
                            max_blocks: MAX_SYNC_BLOCKS,
                        });
                    }
                    NetEvent::PeerDisconnected(_) => {
                        let c = p2p.metrics.peers_connected.load(Ordering::Relaxed) as usize;
                        *shared.peer_count.write().await = c;
                    }
                    NetEvent::SyncRequest { from_height, max_blocks, channel, .. } => {
                        let blocks = store
                            .list_blocks(from_height.max(1), max_blocks as usize)
                            .unwrap_or_default();
                        let _ = cmd_tx.send(NetCommand::RespondSync { channel, blocks });
                    }
                    NetEvent::SyncBlocks { blocks, .. } => {
                        for fb in blocks {
                            apply_external_finalized(
                                fb,
                                &genesis,
                                genesis_id,
                                &schedule_root_hex,
                                &committee_json,
                                &mut state,
                                &mut head_height,
                                &mut head_hash,
                                &mut round,
                                &mut round_started,
                                &mut proposed_rounds,
                                &mut voted_rounds,
                                &mut pending_proposals,
                                &mut votes,
                                &mut mempool,
                                &mut mempool_hashes,
                                &store,
                                &shared,
                            );
                        }
                    }
                    NetEvent::Packet { packet, .. } => match packet {
                        WirePacket::Tx(raw) => {
                            if let Ok(tx) = BitsSignedTxV1::decode_canonical(&raw)
                                && tx.verify_signature().is_ok()
                            {
                                let _ = admit_tx_to_mempool(
                                    tx,
                                    raw,
                                    &state,
                                    head_height + 1,
                                    &mut mempool,
                                    &mut mempool_hashes,
                                    &shared,
                                    &cmd_tx,
                                    false,
                                );
                            }
                        }
                        WirePacket::Finalized(fb) => {
                            let target_h = fb.header.height;
                            if target_h == head_height + 1 {
                                apply_external_finalized(
                                    fb,
                                    &genesis,
                                    genesis_id,
                                    &schedule_root_hex,
                                    &committee_json,
                                    &mut state,
                                    &mut head_height,
                                    &mut head_hash,
                                    &mut round,
                                    &mut round_started,
                                    &mut proposed_rounds,
                                    &mut voted_rounds,
                                    &mut pending_proposals,
                                    &mut votes,
                                    &mut mempool,
                                    &mut mempool_hashes,
                                    &store,
                                    &shared,
                                );
                            } else if target_h > head_height + 1 && last_sync_req.elapsed() >= Duration::from_millis(300) {
                                let _ = cmd_tx.send(NetCommand::RequestSync {
                                    from_height: head_height + 1,
                                    max_blocks: MAX_SYNC_BLOCKS,
                                });
                                last_sync_req = Instant::now();
                            }
                        }
                        WirePacket::Bft(BftWireMsg::Proposal {
                            height,
                            round: p_round,
                            epoch,
                            proposer_id,
                            proposal_pubkey_hex,
                            header,
                            txs_hex,
                            sig_hex,
                        }) => {
                            if height != head_height + 1 || p_round < round {
                                continue;
                            }
                            let expected_epoch = height / genesis.epoch_blocks.max(1);
                            if epoch != expected_epoch || header.epoch != expected_epoch || header.height != height || header.round != p_round {
                                continue;
                            }
                            let proposer_idx = ((height as usize).wrapping_add(p_round as usize)) % genesis.validators.len();
                            let expected_val = &genesis.validators[proposer_idx];
                            if !expected_val.validator_id_hex.eq_ignore_ascii_case(&proposer_id)
                                || !expected_val.proposal_pubkey_hex.eq_ignore_ascii_case(&proposal_pubkey_hex)
                            {
                                continue;
                            }
                            if header.parent_hash != hex::encode(head_hash)
                                || header.genesis_id != genesis_hex
                                || header.validator_schedule_root != schedule_root_hex
                                || header.chain_id != genesis.chain_id
                            {
                                continue;
                            }
                            let Ok(bh) = compute_bits_header_hash(&header) else { continue; };
                            let Ok(pk_bytes) = decode_hex_32(&expected_val.proposal_pubkey_hex) else { continue; };
                            let Ok(pk) = CanonicalEd25519PublicKeyV1::from_bytes(pk_bytes) else { continue; };
                            let Ok(sig_raw) = hex::decode(&sig_hex) else { continue; };
                            let Ok(sig_arr) = <[u8; 64]>::try_from(sig_raw.as_slice()) else { continue; };
                            let Ok(sig) = CanonicalEd25519SignatureV1::from_bytes(sig_arr) else { continue; };
                            let preimage = SigningPreimageV1 {
                                protocol_version: BITS_1_VERSION,
                                chain_id: genesis.chain_id,
                                genesis_id,
                                launch_manifest_id: genesis_id,
                                profile_id: genesis_id,
                                domain_tag: SigningDomainTagV1::BlockProposal,
                                signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
                                hash_suite: HashSuiteIdV1::Sha256V1,
                                payload_digest: proposal_payload_digest(genesis_id, height, p_round, epoch, bh),
                            };
                            if verify_preimage_strict(&pk, &preimage, &sig).is_err() {
                                continue;
                            }

                            // Re-execute all proposed txs deterministically and verify roots!
                            let mut sim = state.clone();
                            let mut tx_hashes = Vec::with_capacity(txs_hex.len());
                            let mut receipts = Vec::with_capacity(txs_hex.len());
                            let mut valid_txs = true;
                            for th in &txs_hex {
                                let Ok(raw) = hex::decode(th) else { valid_txs = false; break; };
                                let Ok(tx) = BitsSignedTxV1::decode_canonical(&raw) else { valid_txs = false; break; };
                                let Ok(rcpt) = sim.apply_signed_tx(&tx, height) else { valid_txs = false; break; };
                                tx_hashes.push(rcpt.tx_hash);
                                receipts.push(TxReceiptRecord::from_committed(&tx, &rcpt));
                            }
                            if !valid_txs {
                                continue;
                            }
                            let tx_root = compute_tx_root(&tx_hashes);
                            if header.balance_root != hex::encode(sim.balance_root())
                                || header.nonce_root != hex::encode(sim.nonce_root())
                                || header.tx_root != hex::encode(tx_root)
                            {
                                continue;
                            }

                            // Catch local round up to valid higher-round proposal if needed.
                            if p_round > round {
                                round = p_round;
                                round_started = Instant::now();
                                shared.bft_round.store(u64::from(round), Ordering::Relaxed);
                            }

                            pending_proposals.insert(
                                (height, p_round, bh),
                                PendingProposal {
                                    header,
                                    txs_hex,
                                    receipts,
                                    next_state: sim,
                                    block_hash: bh,
                                },
                            );

                            // Cast our durable SIGNER-OPS-1 commit vote (at most once per (height, round)).
                            if local_val_idx.is_some()
                                && voted_rounds.insert((height, p_round))
                                && let Ok((vote_sig, _)) = signer.sign_qc_commit_durable(epoch, height, p_round, bh)
                            {
                                shared.wal_writes_total.fetch_add(1, Ordering::Relaxed);
                                let sig_entry = BitsQcSignatureJson {
                                    validator_id: signer.validator_id_hex(),
                                    vote_pubkey_hex: signer.vote_pubkey_hex().to_string(),
                                    sig_hex: hex::encode(vote_sig.as_bytes()),
                                };
                                let _ = cmd_tx.send(NetCommand::BroadcastBft(BftWireMsg::Vote {
                                    height,
                                    round: p_round,
                                    epoch,
                                    block_hash: hex::encode(bh),
                                    validator_id: sig_entry.validator_id.clone(),
                                    vote_pubkey_hex: sig_entry.vote_pubkey_hex.clone(),
                                    sig_hex: sig_entry.sig_hex.clone(),
                                }));
                                votes
                                    .entry((height, p_round, bh))
                                    .or_default()
                                    .insert(sig_entry.validator_id.clone(), sig_entry);
                            }

                            try_finalize(
                                height,
                                p_round,
                                bh,
                                &genesis,
                                &committee_map,
                                &committee_json,
                                total_power,
                                &mut pending_proposals,
                                &mut votes,
                                &mut proposed_rounds,
                                &mut voted_rounds,
                                &mut state,
                                &mut head_height,
                                &mut head_hash,
                                &mut round,
                                &mut round_started,
                                &mut mempool,
                                &mut mempool_hashes,
                                &store,
                                &shared,
                                &cmd_tx,
                            );
                        }
                        WirePacket::Bft(BftWireMsg::Vote {
                            height,
                            round: v_round,
                            epoch,
                            block_hash,
                            validator_id,
                            vote_pubkey_hex,
                            sig_hex,
                        }) => {
                            if height != head_height + 1 {
                                continue;
                            }
                            let vid_lc = validator_id.to_lowercase();
                            let Some(member) = committee_map.get(&vid_lc) else { continue; };
                            if !member.vote_pubkey_hex.eq_ignore_ascii_case(&vote_pubkey_hex) {
                                continue;
                            }
                            let Ok(bh) = decode_hex_32(&block_hash) else { continue; };
                            let Ok(pk_bytes) = decode_hex_32(&member.vote_pubkey_hex) else { continue; };
                            let Ok(pk) = CanonicalEd25519PublicKeyV1::from_bytes(pk_bytes) else { continue; };
                            let Ok(sig_raw) = hex::decode(&sig_hex) else { continue; };
                            let Ok(sig_arr) = <[u8; 64]>::try_from(sig_raw.as_slice()) else { continue; };
                            let Ok(sig) = CanonicalEd25519SignatureV1::from_bytes(sig_arr) else { continue; };

                            let preimage = build_qc_commit_preimage(
                                genesis.chain_id,
                                genesis_id,
                                height,
                                v_round,
                                epoch,
                                bh,
                            );
                            if verify_preimage_strict(&pk, &preimage, &sig).is_err() {
                                continue;
                            }

                            votes.entry((height, v_round, bh)).or_default().insert(
                                vid_lc.clone(),
                                BitsQcSignatureJson {
                                    validator_id: vid_lc,
                                    vote_pubkey_hex: member.vote_pubkey_hex.to_lowercase(),
                                    sig_hex: sig_hex.to_lowercase(),
                                },
                            );

                            try_finalize(
                                height,
                                v_round,
                                bh,
                                &genesis,
                                &committee_map,
                                &committee_json,
                                total_power,
                                &mut pending_proposals,
                                &mut votes,
                                &mut proposed_rounds,
                                &mut voted_rounds,
                                &mut state,
                                &mut head_height,
                                &mut head_hash,
                                &mut round,
                                &mut round_started,
                                &mut mempool,
                                &mut mempool_hashes,
                                &store,
                                &shared,
                                &cmd_tx,
                            );
                        }
                    },
                    NetEvent::Listening(_) => {}
                }
            }
        }
    }

    let _ = http_stop_tx.send(());
    let _ = http_task.await;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn admit_tx_to_mempool(
    tx: BitsSignedTxV1,
    raw_bytes: Vec<u8>,
    state: &BitsEconomyStateV1,
    next_height: u64,
    mempool: &mut Vec<(BitsSignedTxV1, Vec<u8>, [u8; 32])>,
    mempool_hashes: &mut HashSet<[u8; 32]>,
    shared: &Arc<SharedRpcState>,
    cmd_tx: &mpsc::UnboundedSender<NetCommand>,
    broadcast: bool,
) -> Result<String, String> {
    if mempool.len() >= MAX_MEMPOOL_TXS {
        return Err("mempool full".into());
    }
    if tx.chain_id != state.config().chain_id || tx.genesis_id != state.genesis_id() {
        return Err("chain_id or genesis_id mismatch".into());
    }
    let tx_hash = tx.tx_hash().map_err(|e| e.to_string())?;
    let tx_hash_hex = hex::encode(tx_hash);
    if mempool_hashes.contains(&tx_hash) {
        return Ok(tx_hash_hex);
    }

    // Dry-run against current state + any earlier pending txs from the same sender.
    let mut sim = state.clone();
    for (existing, _, _) in mempool.iter() {
        if existing.sender_actor == tx.sender_actor {
            let _ = sim.apply_signed_tx(existing, next_height);
        }
    }
    sim.apply_signed_tx(&tx, next_height)
        .map_err(|e| e.to_string())?;

    mempool_hashes.insert(tx_hash);
    mempool.push((tx, raw_bytes.clone(), tx_hash));
    shared
        .mempool_len
        .store(mempool.len() as u64, Ordering::Relaxed);
    if broadcast {
        let _ = cmd_tx.send(NetCommand::BroadcastTxBytes(raw_bytes));
    }
    Ok(tx_hash_hex)
}

#[allow(clippy::too_many_arguments)]
fn try_finalize(
    height: u64,
    target_round: u32,
    block_hash: [u8; 32],
    genesis: &BitsGenesisFile,
    committee_map: &HashMap<String, &ValidatorGenesisEntry>,
    committee_json: &[bitblock_light_wasm::BitsCommitteeMemberJson],
    total_power: u128,
    pending_proposals: &mut HashMap<(u64, u32, [u8; 32]), PendingProposal>,
    votes: &mut HashMap<(u64, u32, [u8; 32]), BTreeMap<String, BitsQcSignatureJson>>,
    proposed_rounds: &mut HashSet<(u64, u32)>,
    voted_rounds: &mut HashSet<(u64, u32)>,
    state: &mut BitsEconomyStateV1,
    head_height: &mut u64,
    head_hash: &mut [u8; 32],
    round: &mut u32,
    round_started: &mut Instant,
    mempool: &mut Vec<(BitsSignedTxV1, Vec<u8>, [u8; 32])>,
    mempool_hashes: &mut HashSet<[u8; 32]>,
    store: &ChainStore,
    shared: &Arc<SharedRpcState>,
    cmd_tx: &mpsc::UnboundedSender<NetCommand>,
) {
    if height != *head_height + 1 {
        return;
    }
    let Some(vote_map) = votes.get(&(height, target_round, block_hash)) else {
        return;
    };
    let signed_power: u128 = vote_map
        .keys()
        .filter_map(|vid| committee_map.get(vid).map(|m| u128::from(m.power)))
        .sum();
    if signed_power.saturating_mul(3) <= total_power.saturating_mul(2) {
        return;
    }
    let Some(prop) = pending_proposals.remove(&(height, target_round, block_hash)) else {
        return;
    };
    let qc = BitsQuorumCertJson {
        genesis_id: prop.header.genesis_id.clone(),
        height,
        round: target_round,
        epoch: prop.header.epoch,
        block_hash: hex::encode(prop.block_hash),
        signatures: vote_map.values().cloned().collect(),
    };
    let check_json = serde_json::to_vec(&json!({
        "header": prop.header,
        "qc": qc,
        "committee": committee_json,
    }))
    .unwrap_or_default();
    if check_header_qc_slice(&check_json) != 1 {
        warn!(height, target_round, "assembled QC failed light-wasm check");
        return;
    }

    let fb = FinalizedBitsBlock {
        header: prop.header,
        qc,
        txs_hex: prop.txs_hex,
        receipts: prop.receipts,
    };
    if let Err(e) = store.commit_block(&fb) {
        warn!(height, "redb commit_block failed: {e}");
        return;
    }

    *state = prop.next_state;
    *head_height = height;
    *head_hash = block_hash;
    *round = 0;
    *round_started = Instant::now();
    shared.bft_round.store(0, Ordering::Relaxed);

    prune_committed_and_stale(
        &fb,
        state,
        height,
        proposed_rounds,
        voted_rounds,
        pending_proposals,
        votes,
        mempool,
        mempool_hashes,
        shared,
    );
    shared.update_view(state.clone(), *head_height, *head_hash, Some(fb.clone()));
    let _ = cmd_tx.send(NetCommand::BroadcastFinalized(fb));
    let _ = genesis;
}

#[allow(clippy::too_many_arguments)]
fn apply_external_finalized(
    fb: FinalizedBitsBlock,
    genesis: &BitsGenesisFile,
    genesis_id: [u8; 32],
    schedule_root_hex: &str,
    committee_json: &[bitblock_light_wasm::BitsCommitteeMemberJson],
    state: &mut BitsEconomyStateV1,
    head_height: &mut u64,
    head_hash: &mut [u8; 32],
    round: &mut u32,
    round_started: &mut Instant,
    proposed_rounds: &mut HashSet<(u64, u32)>,
    voted_rounds: &mut HashSet<(u64, u32)>,
    pending_proposals: &mut HashMap<(u64, u32, [u8; 32]), PendingProposal>,
    votes: &mut HashMap<(u64, u32, [u8; 32]), BTreeMap<String, BitsQcSignatureJson>>,
    mempool: &mut Vec<(BitsSignedTxV1, Vec<u8>, [u8; 32])>,
    mempool_hashes: &mut HashSet<[u8; 32]>,
    store: &ChainStore,
    shared: &Arc<SharedRpcState>,
) {
    let h = fb.header.height;
    if h != *head_height + 1 {
        return;
    }
    if fb.header.chain_id != genesis.chain_id
        || fb.header.genesis_id != hex::encode(genesis_id)
        || fb.header.parent_hash != hex::encode(*head_hash)
        || fb.header.validator_schedule_root != schedule_root_hex
    {
        return;
    }
    let check_json = serde_json::to_vec(&json!({
        "header": fb.header,
        "qc": fb.qc,
        "committee": committee_json,
    }))
    .unwrap_or_default();
    if check_header_qc_slice(&check_json) != 1 {
        return;
    }
    let Ok(bh) = compute_bits_header_hash(&fb.header) else {
        return;
    };

    let mut sim = state.clone();
    let mut tx_hashes = Vec::with_capacity(fb.txs_hex.len());
    for th in &fb.txs_hex {
        let Ok(raw) = hex::decode(th) else {
            return;
        };
        let Ok(tx) = BitsSignedTxV1::decode_canonical(&raw) else {
            return;
        };
        let Ok(rcpt) = sim.apply_signed_tx(&tx, h) else {
            return;
        };
        tx_hashes.push(rcpt.tx_hash);
    }
    if fb.header.balance_root != hex::encode(sim.balance_root())
        || fb.header.nonce_root != hex::encode(sim.nonce_root())
        || fb.header.tx_root != hex::encode(compute_tx_root(&tx_hashes))
    {
        return;
    }
    if store.commit_block(&fb).is_err() {
        return;
    }

    *state = sim;
    *head_height = h;
    *head_hash = bh;
    *round = 0;
    *round_started = Instant::now();
    shared.bft_round.store(0, Ordering::Relaxed);

    prune_committed_and_stale(
        &fb,
        state,
        h,
        proposed_rounds,
        voted_rounds,
        pending_proposals,
        votes,
        mempool,
        mempool_hashes,
        shared,
    );
    shared.update_view(state.clone(), *head_height, *head_hash, Some(fb));
}

#[allow(clippy::too_many_arguments)]
fn prune_committed_and_stale(
    fb: &FinalizedBitsBlock,
    state: &BitsEconomyStateV1,
    committed_height: u64,
    proposed_rounds: &mut HashSet<(u64, u32)>,
    voted_rounds: &mut HashSet<(u64, u32)>,
    pending_proposals: &mut HashMap<(u64, u32, [u8; 32]), PendingProposal>,
    votes: &mut HashMap<(u64, u32, [u8; 32]), BTreeMap<String, BitsQcSignatureJson>>,
    mempool: &mut Vec<(BitsSignedTxV1, Vec<u8>, [u8; 32])>,
    mempool_hashes: &mut HashSet<[u8; 32]>,
    shared: &Arc<SharedRpcState>,
) {
    for rcpt in &fb.receipts {
        if let Ok(th) = decode_hex_32(&rcpt.tx_hash) {
            mempool_hashes.remove(&th);
        }
    }
    mempool.retain(|(tx, _, th)| {
        if !mempool_hashes.contains(th) {
            return false;
        }
        if tx.seq <= state.nonce_of(&tx.sender_actor) {
            mempool_hashes.remove(th);
            return false;
        }
        if tx.valid_until_height != 0 && committed_height + 1 > tx.valid_until_height {
            mempool_hashes.remove(th);
            return false;
        }
        true
    });
    shared
        .mempool_len
        .store(mempool.len() as u64, Ordering::Relaxed);
    proposed_rounds.retain(|(h, _)| *h > committed_height);
    voted_rounds.retain(|(h, _)| *h > committed_height);
    pending_proposals.retain(|(h, _, _), _| *h > committed_height);
    votes.retain(|(h, _, _), _| *h > committed_height);
}
