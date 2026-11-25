//! Hardened `libp2p` P2P networking layer for the merged Bitblock L1 node:
//! QUIC + TCP/Noise/Yamux + DNS + scored Gossipsub + bounded unicast `request_response`
//! block sync + Kademlia DHT + Identify + isolated per-peer tx/consensus/sync rate buckets.

use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bitblock_light_wasm::BitsBlockHeaderJson;
use bitblock_store::FinalizedBitsBlock;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, StreamExt};
use libp2p::gossipsub::{self, IdentTopic, MessageAcceptance, MessageAuthenticity, ValidationMode};
use libp2p::identify;
use libp2p::kad::{self, store::MemoryStore};
use libp2p::request_response::{self, ProtocolSupport, ResponseChannel};
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{Multiaddr, PeerId, StreamProtocol, SwarmBuilder, noise, tcp, yamux};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::mpsc;

/// Maximum blocks returned in a single unicast sync reply.
pub const MAX_SYNC_BLOCKS: u16 = 32;
/// Hard cap on unicast sync wire payload (`2 MiB`).
pub const MAX_SYNC_WIRE_BYTES: usize = 2 * 1024 * 1024;
/// Hard cap on single canonical signed Bits tx wire bytes (`64 KiB`).
pub const MAX_TX_WIRE_BYTES: usize = 64 * 1024;
/// Hard cap on a single BFT consensus wire packet (`2 MiB`).
pub const MAX_BFT_WIRE_BYTES: usize = 2 * 1024 * 1024;
/// Hard cap on a finalized block broadcast (`2 MiB`).
pub const MAX_BLOCK_WIRE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum NetError {
    #[error("libp2p transport/swarm error: {0}")]
    Swarm(String),
    #[error("invalid multiaddr: {0}")]
    BadAddr(String),
}

/// Consensus messages gossiped over `bitblock/{genesis}/bft`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BftWireMsg {
    /// Block proposal signed by the round proposer's `ValidatorProposal` key.
    Proposal {
        height: u64,
        round: u32,
        epoch: u64,
        proposer_id: String,
        proposal_pubkey_hex: String,
        header: BitsBlockHeaderJson,
        txs_hex: Vec<String>,
        sig_hex: String,
    },
    /// HotStuff commit vote signed strictly through `SIGNER-OPS-1` (`ValidatorVote` key).
    Vote {
        height: u64,
        round: u32,
        epoch: u64,
        block_hash: String,
        validator_id: String,
        vote_pubkey_hex: String,
        sig_hex: String,
    },
}

/// Decoded gossip payload delivered to the node consensus loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WirePacket {
    Bft(BftWireMsg),
    /// Canonical big-endian `BitsSignedTxV1` bytes.
    Tx(Vec<u8>),
    Finalized(FinalizedBitsBlock),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncReq {
    pub from_height: u64,
    pub max_blocks: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResp {
    pub blocks: Vec<FinalizedBitsBlock>,
}

/// Bounded length-prefixed codec for unicast block sync (`MAX_SYNC_WIRE_BYTES`).
#[derive(Debug, Clone, Default)]
struct SyncCodec;

#[async_trait]
impl request_response::Codec for SyncCodec {
    type Protocol = StreamProtocol;
    type Request = SyncReq;
    type Response = SyncResp;

    async fn read_request<T>(&mut self, _: &Self::Protocol, io: &mut T) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut len_buf = [0u8; 4];
        io.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "sync request too large",
            ));
        }
        let mut buf = vec![0u8; len];
        io.read_exact(&mut buf).await?;
        serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut len_buf = [0u8; 4];
        io.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > MAX_SYNC_WIRE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "sync response exceeds 2MiB cap",
            ));
        }
        let mut buf = vec![0u8; len];
        io.read_exact(&mut buf).await?;
        serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    async fn write_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let buf =
            serde_json::to_vec(&req).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        io.write_all(&(buf.len() as u32).to_be_bytes()).await?;
        io.write_all(&buf).await?;
        io.close().await
    }

    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        resp: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let buf =
            serde_json::to_vec(&resp).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        if buf.len() > MAX_SYNC_WIRE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "sync response exceeds 2MiB cap",
            ));
        }
        io.write_all(&(buf.len() as u32).to_be_bytes()).await?;
        io.write_all(&buf).await?;
        io.close().await
    }
}

#[derive(Debug, Default)]
pub struct NetMetrics {
    pub peers_connected: AtomicU64,
    pub gossip_rx_total: AtomicU64,
    pub gossip_rejected_total: AtomicU64,
    pub sync_requests_served_total: AtomicU64,
}

#[derive(Debug, Clone)]
pub struct P2pConfig {
    pub listen_addrs: Vec<String>,
    pub bootstrap_peers: Vec<String>,
    pub genesis_hex: String,
    pub key_seed: [u8; 32],
}

#[derive(Debug)]
pub enum NetCommand {
    BroadcastBft(BftWireMsg),
    BroadcastTxBytes(Vec<u8>),
    BroadcastFinalized(FinalizedBitsBlock),
    RequestSync {
        from_height: u64,
        max_blocks: u16,
    },
    RespondSync {
        channel: ResponseChannel<SyncResp>,
        blocks: Vec<FinalizedBitsBlock>,
    },
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum NetEvent {
    Listening(Multiaddr),
    PeerConnected(PeerId),
    PeerDisconnected(PeerId),
    Packet {
        from: PeerId,
        packet: WirePacket,
    },
    SyncRequest {
        from: PeerId,
        from_height: u64,
        max_blocks: u16,
        channel: ResponseChannel<SyncResp>,
    },
    SyncBlocks {
        from: PeerId,
        blocks: Vec<FinalizedBitsBlock>,
    },
}

#[derive(NetworkBehaviour)]
struct BitblockBehaviour {
    gossipsub: gossipsub::Behaviour,
    sync: request_response::Behaviour<SyncCodec>,
    kademlia: kad::Behaviour<MemoryStore>,
    identify: identify::Behaviour,
}

pub struct P2pHandle {
    pub local_peer_id: PeerId,
    pub cmd_tx: mpsc::UnboundedSender<NetCommand>,
    pub evt_rx: mpsc::UnboundedReceiver<NetEvent>,
    pub metrics: Arc<NetMetrics>,
}

/// Isolated per-peer rate buckets so tx floods on `topic_tx` can never starve BFT or sync.
struct PeerRate {
    tx_window_start: Instant,
    tx_count: u32,
    bft_window_start: Instant,
    bft_count: u32,
    sync_window_start: Instant,
    sync_count: u32,
}

impl PeerRate {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            tx_window_start: now,
            tx_count: 0,
            bft_window_start: now,
            bft_count: 0,
            sync_window_start: now,
            sync_count: 0,
        }
    }

    fn allow_tx(&mut self) -> bool {
        if self.tx_window_start.elapsed() >= Duration::from_secs(1) {
            self.tx_window_start = Instant::now();
            self.tx_count = 0;
        }
        self.tx_count += 1;
        self.tx_count <= 600
    }

    fn allow_consensus(&mut self) -> bool {
        if self.bft_window_start.elapsed() >= Duration::from_secs(1) {
            self.bft_window_start = Instant::now();
            self.bft_count = 0;
        }
        self.bft_count += 1;
        self.bft_count <= 600
    }

    fn allow_sync(&mut self) -> bool {
        if self.sync_window_start.elapsed() >= Duration::from_secs(10) {
            self.sync_window_start = Instant::now();
            self.sync_count = 0;
        }
        self.sync_count += 1;
        self.sync_count <= 30
    }
}

/// Spawns the hardened `libp2p` QUIC + TCP + DNS + scored Gossipsub + unicast sync swarm.
pub fn spawn_p2p(cfg: P2pConfig) -> Result<P2pHandle, NetError> {
    let mut ed_secret = cfg.key_seed;
    let kp = libp2p::identity::Keypair::ed25519_from_bytes(&mut ed_secret)
        .map_err(|e| NetError::Swarm(e.to_string()))?;
    let local_peer_id = PeerId::from(kp.public());

    let hg_short = &cfg.genesis_hex[..cfg.genesis_hex.len().min(16)];
    let topic_bft = IdentTopic::new(format!("bitblock/{hg_short}/bft"));
    let topic_tx = IdentTopic::new(format!("bitblock/{hg_short}/tx"));
    let topic_blk = IdentTopic::new(format!("bitblock/{hg_short}/blocks"));
    let topic_tx_hash = topic_tx.hash();
    let topic_bft_hash = topic_bft.hash();
    let topic_blk_hash = topic_blk.hash();

    let gossip_cfg = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_millis(200))
        .validation_mode(ValidationMode::Strict)
        .validate_messages()
        .max_transmit_size(MAX_BLOCK_WIRE_BYTES)
        .message_id_fn(|msg: &gossipsub::Message| {
            let h = Sha256::digest(&msg.data);
            gossipsub::MessageId::from(hex::encode(&h[..16]))
        })
        .build()
        .map_err(|e| NetError::Swarm(e.to_string()))?;

    let mut gossipsub =
        gossipsub::Behaviour::new(MessageAuthenticity::Signed(kp.clone()), gossip_cfg)
            .map_err(|e| NetError::Swarm(e.to_string()))?;
    gossipsub
        .with_peer_score(
            gossipsub::PeerScoreParams::default(),
            gossipsub::PeerScoreThresholds::default(),
        )
        .map_err(|e| NetError::Swarm(e.to_string()))?;
    gossipsub
        .subscribe(&topic_bft)
        .map_err(|e| NetError::Swarm(e.to_string()))?;
    gossipsub
        .subscribe(&topic_tx)
        .map_err(|e| NetError::Swarm(e.to_string()))?;
    gossipsub
        .subscribe(&topic_blk)
        .map_err(|e| NetError::Swarm(e.to_string()))?;

    let sync_proto = StreamProtocol::try_from_owned(format!("/bitblock/{hg_short}/sync/1.0.0"))
        .map_err(|e| NetError::Swarm(e.to_string()))?;
    let sync = request_response::Behaviour::new(
        [(sync_proto, ProtocolSupport::Full)],
        request_response::Config::default().with_request_timeout(Duration::from_secs(10)),
    );

    let kademlia = kad::Behaviour::new(local_peer_id, MemoryStore::new(local_peer_id));
    let identify = identify::Behaviour::new(identify::Config::new(
        format!("/bitblock/{hg_short}/1.0.0"),
        kp.public(),
    ));

    let mut swarm = SwarmBuilder::with_existing_identity(kp)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )
        .map_err(|e| NetError::Swarm(e.to_string()))?
        .with_quic()
        .with_dns()
        .map_err(|e| NetError::Swarm(e.to_string()))?
        .with_behaviour(|_| BitblockBehaviour {
            gossipsub,
            sync,
            kademlia,
            identify,
        })
        .map_err(|e| NetError::Swarm(e.to_string()))?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    for addr_str in &cfg.listen_addrs {
        let addr: Multiaddr = addr_str
            .parse()
            .map_err(|_| NetError::BadAddr(addr_str.clone()))?;
        swarm
            .listen_on(addr)
            .map_err(|e| NetError::Swarm(e.to_string()))?;
    }

    let bootstrap_addrs: Vec<Multiaddr> = cfg
        .bootstrap_peers
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();

    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<NetCommand>();
    let (evt_tx, evt_rx) = mpsc::unbounded_channel::<NetEvent>();
    let metrics = Arc::new(NetMetrics::default());
    let metrics_bg = metrics.clone();

    tokio::spawn(async move {
        for addr in &bootstrap_addrs {
            let _ = swarm.dial(addr.clone());
        }
        let mut redial = tokio::time::interval(Duration::from_millis(800));
        let mut connected: HashSet<PeerId> = HashSet::new();
        let mut sync_rr: usize = 0;
        let mut rates: HashMap<PeerId, PeerRate> = HashMap::new();

        loop {
            tokio::select! {
                _ = redial.tick() => {
                    if connected.len() < bootstrap_addrs.len() {
                        for addr in &bootstrap_addrs {
                            let _ = swarm.dial(addr.clone());
                        }
                    }
                }
                cmd = cmd_rx.recv() => {
                    let Some(cmd) = cmd else { break; };
                    match cmd {
                        NetCommand::BroadcastBft(m) => {
                            if let Ok(bytes) = serde_json::to_vec(&WirePacket::Bft(m))
                                && bytes.len() <= MAX_BFT_WIRE_BYTES
                            {
                                let _ = swarm.behaviour_mut().gossipsub.publish(topic_bft.clone(), bytes);
                            }
                        }
                        NetCommand::BroadcastTxBytes(bytes) => {
                            if bytes.len() <= MAX_TX_WIRE_BYTES {
                                let _ = swarm.behaviour_mut().gossipsub.publish(topic_tx.clone(), bytes);
                            }
                        }
                        NetCommand::BroadcastFinalized(b) => {
                            if let Ok(bytes) = serde_json::to_vec(&WirePacket::Finalized(b))
                                && bytes.len() <= MAX_BLOCK_WIRE_BYTES
                            {
                                let _ = swarm.behaviour_mut().gossipsub.publish(topic_blk.clone(), bytes);
                            }
                        }
                        NetCommand::RequestSync { from_height, max_blocks } => {
                            let peers: Vec<PeerId> = connected.iter().copied().collect();
                            if !peers.is_empty() {
                                let target = peers[sync_rr % peers.len()];
                                sync_rr = sync_rr.wrapping_add(1);
                                swarm.behaviour_mut().sync.send_request(
                                    &target,
                                    SyncReq {
                                        from_height,
                                        max_blocks: max_blocks.clamp(1, MAX_SYNC_BLOCKS),
                                    },
                                );
                            }
                        }
                        NetCommand::RespondSync { channel, blocks } => {
                            let capped: Vec<FinalizedBitsBlock> =
                                blocks.into_iter().take(MAX_SYNC_BLOCKS as usize).collect();
                            if swarm
                                .behaviour_mut()
                                .sync
                                .send_response(channel, SyncResp { blocks: capped })
                                .is_ok()
                            {
                                metrics_bg
                                    .sync_requests_served_total
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                }
                event = swarm.select_next_some() => {
                    match event {
                        SwarmEvent::NewListenAddr { address, .. } => {
                            let _ = evt_tx.send(NetEvent::Listening(address));
                        }
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            connected.insert(peer_id);
                            metrics_bg.peers_connected.store(connected.len() as u64, Ordering::Relaxed);
                            swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                            let _ = evt_tx.send(NetEvent::PeerConnected(peer_id));
                        }
                        SwarmEvent::ConnectionClosed { peer_id, num_established: 0, .. } => {
                            connected.remove(&peer_id);
                            rates.remove(&peer_id);
                            metrics_bg.peers_connected.store(connected.len() as u64, Ordering::Relaxed);
                            let _ = evt_tx.send(NetEvent::PeerDisconnected(peer_id));
                        }
                        SwarmEvent::Behaviour(BitblockBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                            propagation_source,
                            message_id,
                            message,
                        })) => {
                            metrics_bg.gossip_rx_total.fetch_add(1, Ordering::Relaxed);
                            let limiter = rates.entry(propagation_source).or_insert_with(PeerRate::new);
                            let allowed = if message.topic == topic_tx_hash {
                                limiter.allow_tx()
                            } else {
                                limiter.allow_consensus()
                            };
                            if !allowed {
                                metrics_bg.gossip_rejected_total.fetch_add(1, Ordering::Relaxed);
                                let _ = swarm.behaviour_mut().gossipsub.report_message_validation_result(
                                    &message_id,
                                    &propagation_source,
                                    MessageAcceptance::Ignore,
                                );
                                continue;
                            }

                            let decoded: Option<WirePacket> = if message.topic == topic_tx_hash {
                                if !message.data.is_empty() && message.data.len() <= MAX_TX_WIRE_BYTES {
                                    Some(WirePacket::Tx(message.data))
                                } else {
                                    None
                                }
                            } else if message.topic == topic_bft_hash {
                                if message.data.len() <= MAX_BFT_WIRE_BYTES {
                                    serde_json::from_slice::<WirePacket>(&message.data)
                                        .ok()
                                        .filter(|p| matches!(p, WirePacket::Bft(_)))
                                } else {
                                    None
                                }
                            } else if message.topic == topic_blk_hash {
                                if message.data.len() <= MAX_BLOCK_WIRE_BYTES {
                                    serde_json::from_slice::<WirePacket>(&message.data)
                                        .ok()
                                        .filter(|p| matches!(p, WirePacket::Finalized(_)))
                                } else {
                                    None
                                }
                            } else {
                                None
                            };

                            if let Some(pkt) = decoded {
                                let _ = swarm.behaviour_mut().gossipsub.report_message_validation_result(
                                    &message_id,
                                    &propagation_source,
                                    MessageAcceptance::Accept,
                                );
                                let _ = evt_tx.send(NetEvent::Packet { from: propagation_source, packet: pkt });
                            } else {
                                metrics_bg.gossip_rejected_total.fetch_add(1, Ordering::Relaxed);
                                let _ = swarm.behaviour_mut().gossipsub.report_message_validation_result(
                                    &message_id,
                                    &propagation_source,
                                    MessageAcceptance::Reject,
                                );
                            }
                        }
                        SwarmEvent::Behaviour(BitblockBehaviourEvent::Sync(request_response::Event::Message {
                            peer,
                            message,
                        })) => match message {
                            request_response::Message::Request { request, channel, .. } => {
                                let limiter = rates.entry(peer).or_insert_with(PeerRate::new);
                                if limiter.allow_sync() {
                                    let _ = evt_tx.send(NetEvent::SyncRequest {
                                        from: peer,
                                        from_height: request.from_height,
                                        max_blocks: request.max_blocks.clamp(1, MAX_SYNC_BLOCKS),
                                        channel,
                                    });
                                } else {
                                    metrics_bg.gossip_rejected_total.fetch_add(1, Ordering::Relaxed);
                                    let _ = swarm.behaviour_mut().sync.send_response(
                                        channel,
                                        SyncResp { blocks: Vec::new() },
                                    );
                                }
                            }
                            request_response::Message::Response { response, .. } => {
                                let _ = evt_tx.send(NetEvent::SyncBlocks {
                                    from: peer,
                                    blocks: response.blocks,
                                });
                            }
                        },
                        _ => {}
                    }
                }
            }
        }
    });

    Ok(P2pHandle {
        local_peer_id,
        cmd_tx,
        evt_rx,
        metrics,
    })
}
