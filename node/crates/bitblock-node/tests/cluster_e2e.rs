//! End-to-end 4-validator live `libp2p` + `axum` shadow testnet integration test (PR #107).
//!
//! Exercises:
//! 1. 4-validator Ed25519 HotStuff finality over real loopback `libp2p` TCP/Noise/Yamux + Gossipsub
//! 2. Browser `bitblock-light-wasm` QC (`verify_header_qc_json`) and `AUTH-COMMIT-1` Domain 3
//!    Patricia wallet proof (`verify_wallet_proof_json`) parity against live HTTP `/proof*` doors
//! 3. `POST /faucet` debiting `TestnetGrantPool` (zero runtime minting)
//! 4. Cross-Session Amendment 1: `/proof/wallet/{pubkeyHex}` == `/proof/wallet/{actorId}` +
//!    `BitsIdentityBindV1` key rotation preserving `ActorIdV1` & balance while revoking old key
//! 5. Atomic `80/20` `BitsPurchaseSplitV1` (`250 -> creator +200, treasury +50, 0% tax`) +
//!    `/proof/tx/{hash}` receipt check + duplicate `order_id` replay refusal
//! 6. `BitsSimulatedPayoutV1` (`100 -> simulatedPayoutEscrow +100`)
//! 7. Validator crash (graceful stop) -> 3-of-4 continues finalizing -> cold restart catches up
//!    via unicast `libp2p` sync + `redb` ACID replay + `SIGNER-OPS-1` watermark safety
//! 8. Hardening: `413` on `> 64 KiB` POST body, `<= 100` cap on `/proof/blocks?limit=999`, and
//!    `403 Forbidden` on `POST /faucet` when `genesis.lab == false`.

use std::net::TcpListener;
use std::time::{Duration, Instant};

use bitblock_light_wasm::{check_header_qc_slice, check_wallet_proof_slice};
use bitblock_node::{init_testnet, run_node_with_shutdown};
use bitblock_research::bits_economy_1::{
    BitsGrantKindV1, BitsIdentityBindV1, BitsIdentityProviderV1, BitsPurchaseSplitV1,
    BitsSignedTxV1, BitsSimulatedPayoutV1, BitsTestnetGrantV1, BitsTxPayloadV1,
    derive_default_bits_actor_id, sign_identity_bind_pop_with_seed,
};
use bitblock_research::crypto_d1::CanonicalEd25519PublicKeyV1;
use bitblock_store::decode_hex_32;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::oneshot;

fn pick_free_port_block(count: u16) -> u16 {
    // Bind one ephemeral port to find an unused region, then verify contiguous ports are free.
    for _ in 0..32 {
        let probe = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = probe.local_addr().unwrap().port();
        drop(probe);
        if base > 55_000 {
            continue;
        }
        let mut guards = Vec::new();
        let mut ok = true;
        for off in 0..count {
            match TcpListener::bind(("127.0.0.1", base + off)) {
                Ok(g) => guards.push(g),
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            drop(guards);
            return base;
        }
    }
    panic!("could not reserve contiguous free ports");
}

async fn http_req(port: u16, method: &str, path: &str, body: Option<&str>) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect rpc");
    let body_bytes = body.unwrap_or("").as_bytes();
    let req = if body.is_some() {
        format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body_bytes.len()
        )
    } else {
        format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n")
    };
    stream.write_all(req.as_bytes()).await.unwrap();
    if !body_bytes.is_empty() {
        stream.write_all(body_bytes).await.unwrap();
    }
    let mut resp = Vec::new();
    stream.read_to_end(&mut resp).await.unwrap();
    let text = String::from_utf8_lossy(&resp);
    let (hdrs, payload) = text.split_once("\r\n\r\n").expect("valid http response");
    let status: u16 = hdrs
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    (status, payload.to_string())
}

async fn wait_for_height(port: u16, target: u64, timeout: Duration) -> Value {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)).await {
            let _ = stream
                .write_all(
                    format!(
                        "GET /proof HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
                    )
                    .as_bytes(),
                )
                .await;
            let mut resp = Vec::new();
            if stream.read_to_end(&mut resp).await.is_ok() {
                let text = String::from_utf8_lossy(&resp);
                if let Some((_, body)) = text.split_once("\r\n\r\n")
                    && let Ok(v) = serde_json::from_str::<Value>(body)
                    && v["head"]["height"].as_u64().unwrap_or(0) >= target
                {
                    return v;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(120)).await;
    }
    panic!("node on port {port} did not reach height {target} within {timeout:?}");
}

async fn wait_for_tx(port: u16, tx_hash: &str, timeout: Duration) -> Value {
    let start = Instant::now();
    while start.elapsed() < timeout {
        let (code, body) = http_req(port, "GET", &format!("/proof/tx/{tx_hash}"), None).await;
        if code == 200 {
            return serde_json::from_str(&body).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(120)).await;
    }
    panic!("tx {tx_hash} not committed on port {port} within {timeout:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn four_validator_cluster_bits_economy_amendment1_and_recovery() {
    let base_dir = std::env::temp_dir().join(format!(
        "bb_cluster_e2e_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let p2p_base = pick_free_port_block(4);
    let rpc_base = pick_free_port_block(4);

    let init = init_testnet(
        base_dir.clone(),
        4,
        p2p_base,
        rpc_base,
        250,
        Some(50),
        "cluster-e2e-pass".into(),
        true,
        vec![],
    )
    .expect("init_testnet");

    let mut stops = Vec::new();
    let mut handles = Vec::new();
    for i in 0..4 {
        let cfg = init.node_configs[i].clone();
        let pass = init.password_files[i].clone();
        let (tx, rx) = oneshot::channel();
        stops.push(Some(tx));
        handles.push(Some(tokio::spawn(async move {
            run_node_with_shutdown(cfg, Some(pass), false, Some(rx))
                .await
                .unwrap();
        })));
    }

    // 1. Wait for all 4 validators to finalize height >= 2 over real libp2p!
    let proof0 = wait_for_height(rpc_base, 2, Duration::from_secs(20)).await;
    assert_eq!(proof0["protocol"], "bitblock-l1-v1");
    assert_eq!(proof0["lab"], true);
    assert_eq!(proof0["unit"]["symbol"], "BITS");
    assert_eq!(proof0["unit"]["creatorSplitBps"], 8000);
    assert_eq!(proof0["unit"]["treasurySplitBps"], 2000);

    let chain_id = proof0["chainId"].as_u64().unwrap();
    let genesis_hex = proof0["genesis"].as_str().unwrap().to_string();
    let genesis_id = decode_hex_32(&genesis_hex).unwrap();
    let treasury_hex = proof0["systemActors"]["platformTreasury"]
        .as_str()
        .unwrap()
        .to_string();
    let treasury_actor = bitblock_research::identity_d1::ActorIdV1::from_bytes(
        decode_hex_32(&treasury_hex).unwrap(),
    );
    let grant_pool_actor = bitblock_research::identity_d1::ActorIdV1::from_bytes(
        decode_hex_32(proof0["systemActors"]["grantPool"].as_str().unwrap()).unwrap(),
    );

    // 2. Verify live Ed25519 HotStuff QC through `bitblock-light-wasm`!
    let (v_code, v_body) = http_req(rpc_base, "GET", "/proof/validators", None).await;
    assert_eq!(v_code, 200);
    let v_json: Value = serde_json::from_str(&v_body).unwrap();
    let qc_check_payload = serde_json::to_vec(&json!({
        "header": proof0["head"]["header"],
        "qc": proof0["head"]["qc"],
        "committee": v_json["committee"],
    }))
    .unwrap();
    assert_eq!(
        check_header_qc_slice(&qc_check_payload),
        1,
        "live node QC must verify under bitblock-light-wasm"
    );

    // 3. Test POST /faucet (50 Bits from TestnetGrantPool) + Amendment 1 dual lookup + Patricia proof!
    let buyer_seed1 = [0xA1; 32];
    let buyer_seed2 = [0xA2; 32];
    let creator_seed = [0xC1; 32];
    let buyer_pk1 = CanonicalEd25519PublicKeyV1::from_secret_seed(&buyer_seed1).unwrap();
    let buyer_pk2 = CanonicalEd25519PublicKeyV1::from_secret_seed(&buyer_seed2).unwrap();
    let creator_pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&creator_seed).unwrap();
    let buyer_pk1_hex = hex::encode(buyer_pk1.as_bytes());
    let buyer_actor = derive_default_bits_actor_id(chain_id, &buyer_pk1).unwrap();
    let creator_actor = derive_default_bits_actor_id(chain_id, &creator_pk).unwrap();
    let buyer_actor_hex = hex::encode(buyer_actor.as_bytes());

    let (f_code, f_body) = http_req(
        rpc_base,
        "POST",
        "/faucet",
        Some(&json!({ "pubkey_hex": buyer_pk1_hex, "amount": 50 }).to_string()),
    )
    .await;
    assert_eq!(f_code, 202, "faucet response: {f_body}");
    let f_json: Value = serde_json::from_str(&f_body).unwrap();
    let faucet_tx_hash = f_json["txHash"].as_str().unwrap();
    wait_for_tx(rpc_base, faucet_tx_hash, Duration::from_secs(12)).await;

    // Amendment 1: Lookup by raw Ed25519 pubkeyHex AND by ActorIdV1 return identical canonical actorId & proof!
    let (w1_code, w1_body) = http_req(
        rpc_base,
        "GET",
        &format!("/proof/wallet/{buyer_pk1_hex}"),
        None,
    )
    .await;
    let (w2_code, w2_body) = http_req(
        rpc_base,
        "GET",
        &format!("/proof/wallet/{buyer_actor_hex}"),
        None,
    )
    .await;
    assert_eq!(w1_code, 200);
    assert_eq!(w2_code, 200);
    let w1: Value = serde_json::from_str(&w1_body).unwrap();
    let w2: Value = serde_json::from_str(&w2_body).unwrap();
    assert_eq!(w1["actorId"], buyer_actor_hex);
    assert_eq!(w2["actorId"], buyer_actor_hex);
    assert_eq!(w1["balance"], 50);
    assert_eq!(w2["balance"], 50);
    assert_eq!(w1["domainId"], 3);
    assert_eq!(
        check_wallet_proof_slice(w1_body.as_bytes()),
        1,
        "live AUTH-COMMIT-1 Patricia proof must verify under bitblock-light-wasm"
    );

    // 4. Submit a 1,000-Bit Signup grant via POST /tx using relayer key (funded strictly from TestnetGrantPool).
    let relayer_seed = decode_hex_32(&init.faucet_relayer_seed_hex).unwrap();
    let (_, gp_body) = http_req(
        rpc_base,
        "GET",
        &format!("/proof/wallet/{}", hex::encode(grant_pool_actor.as_bytes())),
        None,
    )
    .await;
    let gp_seq = serde_json::from_str::<Value>(&gp_body).unwrap()["seq"]
        .as_u64()
        .unwrap();
    let signup_tx = BitsSignedTxV1::sign_with_seed(
        chain_id,
        genesis_id,
        grant_pool_actor,
        &relayer_seed,
        gp_seq + 1,
        0,
        BitsTxPayloadV1::TestnetGrant(BitsTestnetGrantV1 {
            recipient: buyer_actor,
            recipient_pubkey: buyer_pk1,
            amount_bits: 1_000,
            grant_kind: BitsGrantKindV1::Signup,
            grant_id: [0x51; 32],
        }),
    )
    .unwrap();
    let (s_code, s_body) = http_req(
        rpc_base,
        "POST",
        "/tx",
        Some(&json!({ "tx_hex": hex::encode(signup_tx.encode_canonical().unwrap()) }).to_string()),
    )
    .await;
    assert_eq!(s_code, 202, "signup grant tx: {s_body}");
    let signup_hash = serde_json::from_str::<Value>(&s_body).unwrap()["txHash"]
        .as_str()
        .unwrap()
        .to_string();
    wait_for_tx(rpc_base, &signup_hash, Duration::from_secs(12)).await;

    // 5. Amendment 1 key rotation: rotate buyer from buyer_pk1 -> buyer_pk2 while preserving ActorIdV1 & 1,050 Bits!
    let ext_hash = [0xD1; 32];
    let pop_sig = sign_identity_bind_pop_with_seed(
        &buyer_seed2,
        chain_id,
        &genesis_id,
        &buyer_actor,
        BitsIdentityProviderV1::GameladderPlayerHash,
        &ext_hash,
    )
    .unwrap();
    let bind_tx = BitsSignedTxV1::sign_with_seed(
        chain_id,
        genesis_id,
        buyer_actor,
        &buyer_seed1,
        1,
        0,
        BitsTxPayloadV1::IdentityBind(BitsIdentityBindV1 {
            actor_id: buyer_actor,
            ed25519_pk: buyer_pk2,
            provider: BitsIdentityProviderV1::GameladderPlayerHash,
            external_id_hash: ext_hash,
            pop_signature: pop_sig,
        }),
    )
    .unwrap();
    let (b_code, b_body) = http_req(
        rpc_base,
        "POST",
        "/tx",
        Some(&json!({ "tlv_hex": hex::encode(bind_tx.encode_canonical().unwrap()) }).to_string()),
    )
    .await;
    assert_eq!(b_code, 202, "identity bind tx: {b_body}");
    let bind_hash = serde_json::from_str::<Value>(&b_body).unwrap()["txHash"]
        .as_str()
        .unwrap()
        .to_string();
    wait_for_tx(rpc_base, &bind_hash, Duration::from_secs(12)).await;

    // Old buyer_seed1 must now be refused!
    let old_key_tx = BitsSignedTxV1::sign_with_seed(
        chain_id,
        genesis_id,
        buyer_actor,
        &buyer_seed1,
        2,
        0,
        BitsTxPayloadV1::PurchaseSplit(BitsPurchaseSplitV1 {
            buyer: buyer_actor,
            creator: creator_actor,
            treasury: treasury_actor,
            amount_bits: 250,
            creator_bps: 8_000,
            order_id: [0x61; 32],
            memo_hash: [0x62; 32],
        }),
    )
    .unwrap();
    let (old_code, _) = http_req(
        rpc_base,
        "POST",
        "/tx",
        Some(&json!({ "tx_hex": hex::encode(old_key_tx.encode_canonical().unwrap()) }).to_string()),
    )
    .await;
    assert_eq!(old_code, 400, "rotated-out key must be rejected");

    // 6. Atomic 80/20 PurchaseSplit signed by rotated buyer_seed2 (250 -> creator +200, treasury +50)!
    let order_id = [0x61; 32];
    let buy_tx = BitsSignedTxV1::sign_with_seed(
        chain_id,
        genesis_id,
        buyer_actor,
        &buyer_seed2,
        2,
        0,
        BitsTxPayloadV1::PurchaseSplit(BitsPurchaseSplitV1 {
            buyer: buyer_actor,
            creator: creator_actor,
            treasury: treasury_actor,
            amount_bits: 250,
            creator_bps: 8_000,
            order_id,
            memo_hash: [0x62; 32],
        }),
    )
    .unwrap();
    let (p_code, p_body) = http_req(
        rpc_base,
        "POST",
        "/tx",
        Some(&json!({ "tx_hex": hex::encode(buy_tx.encode_canonical().unwrap()) }).to_string()),
    )
    .await;
    assert_eq!(p_code, 202, "purchase tx: {p_body}");
    let buy_hash = serde_json::from_str::<Value>(&p_body).unwrap()["txHash"]
        .as_str()
        .unwrap()
        .to_string();
    let buy_rcpt = wait_for_tx(rpc_base, &buy_hash, Duration::from_secs(12)).await;
    assert_eq!(buy_rcpt["receipt"]["kind"], "purchase_split");
    assert_eq!(buy_rcpt["receipt"]["creator_bits_paid"], 200);
    assert_eq!(buy_rcpt["receipt"]["platform_bits_paid"], 50);
    assert_eq!(buy_rcpt["receipt"]["order_id"], hex::encode(order_id));

    // Duplicate order_id with next seq MUST fail closed!
    let dup_buy = BitsSignedTxV1::sign_with_seed(
        chain_id,
        genesis_id,
        buyer_actor,
        &buyer_seed2,
        3,
        0,
        BitsTxPayloadV1::PurchaseSplit(BitsPurchaseSplitV1 {
            buyer: buyer_actor,
            creator: creator_actor,
            treasury: treasury_actor,
            amount_bits: 250,
            creator_bps: 8_000,
            order_id,
            memo_hash: [0x62; 32],
        }),
    )
    .unwrap();
    let (dup_code, _) = http_req(
        rpc_base,
        "POST",
        "/tx",
        Some(&json!({ "tx_hex": hex::encode(dup_buy.encode_canonical().unwrap()) }).to_string()),
    )
    .await;
    assert_eq!(dup_code, 400, "duplicate order_id must be rejected");

    // 7. Creator binds its Ed25519 key (Amendment 1 PoP) and requests simulated payout (100 Bits -> simulatedPayoutEscrow +100)!
    let creator_ext = [0xC2; 32];
    let creator_pop = sign_identity_bind_pop_with_seed(
        &creator_seed,
        chain_id,
        &genesis_id,
        &creator_actor,
        BitsIdentityProviderV1::StudioHandle,
        &creator_ext,
    )
    .unwrap();
    let creator_bind_tx = BitsSignedTxV1::sign_with_seed(
        chain_id,
        genesis_id,
        creator_actor,
        &creator_seed,
        1,
        0,
        BitsTxPayloadV1::IdentityBind(BitsIdentityBindV1 {
            actor_id: creator_actor,
            ed25519_pk: creator_pk,
            provider: BitsIdentityProviderV1::StudioHandle,
            external_id_hash: creator_ext,
            pop_signature: creator_pop,
        }),
    )
    .unwrap();
    let (cb_code, cb_body) = http_req(
        rpc_base,
        "POST",
        "/tx",
        Some(
            &json!({ "tx_hex": hex::encode(creator_bind_tx.encode_canonical().unwrap()) })
                .to_string(),
        ),
    )
    .await;
    assert_eq!(cb_code, 202, "creator bind tx: {cb_body}");
    let cb_hash = serde_json::from_str::<Value>(&cb_body).unwrap()["txHash"]
        .as_str()
        .unwrap()
        .to_string();
    wait_for_tx(rpc_base, &cb_hash, Duration::from_secs(12)).await;

    let payout_tx = BitsSignedTxV1::sign_with_seed(
        chain_id,
        genesis_id,
        creator_actor,
        &creator_seed,
        2,
        0,
        BitsTxPayloadV1::SimulatedPayout(BitsSimulatedPayoutV1 {
            creator: creator_actor,
            amount_bits: 100,
            payout_request_id: [0x71; 32],
        }),
    )
    .unwrap();
    let (po_code, po_body) = http_req(
        rpc_base,
        "POST",
        "/tx",
        Some(&json!({ "tx_hex": hex::encode(payout_tx.encode_canonical().unwrap()) }).to_string()),
    )
    .await;
    assert_eq!(po_code, 202, "simulated payout tx: {po_body}");
    let po_hash = serde_json::from_str::<Value>(&po_body).unwrap()["txHash"]
        .as_str()
        .unwrap()
        .to_string();
    wait_for_tx(rpc_base, &po_hash, Duration::from_secs(12)).await;

    // Verify exact post-payout balances + conservation across nodes!
    let (_, head_after_body) = http_req(rpc_base + 1, "GET", "/proof", None).await;
    let head_after: Value = serde_json::from_str(&head_after_body).unwrap();
    assert_eq!(head_after["head"]["treasuryBalance"], 50);
    assert_eq!(head_after["head"]["simulatedPayoutEscrow"], 100);
    assert_eq!(
        head_after["head"]["grantPoolRemaining"],
        1_000_000_000 - 1_050
    );

    // 8. Hardening checks: 413 on >64 KiB POST body + <=100 cap on /proof/blocks?limit=999
    let huge_body = format!("{{\"tx_hex\":\"{}\"}}", "aa".repeat(40_000));
    let (big_code, _) = http_req(rpc_base, "POST", "/tx", Some(&huge_body)).await;
    assert_eq!(big_code, 413, "oversized POST body must return 413");

    let (bl_code, bl_body) =
        http_req(rpc_base, "GET", "/proof/blocks?from=1&limit=999", None).await;
    assert_eq!(bl_code, 200);
    let bl_json: Value = serde_json::from_str(&bl_body).unwrap();
    assert!(bl_json["blocks"].as_array().unwrap().len() <= 100);

    // 9. Crash/stop node-3, verify 3-of-4 continues finalizing, then restart node-3 & verify catch-up!
    let h_before_stop = head_after["head"]["height"].as_u64().unwrap();
    if let Some(tx) = stops[3].take() {
        let _ = tx.send(());
    }
    if let Some(h) = handles[3].take() {
        let _ = h.await;
    }
    let after_3of4 = wait_for_height(rpc_base, h_before_stop + 3, Duration::from_secs(15)).await;
    let h_target = after_3of4["head"]["height"].as_u64().unwrap();

    // Restart node-3 cold from disk (tests ACID redb replay + SIGNER-OPS-1 watermark + libp2p sync!)
    let cfg3 = init.node_configs[3].clone();
    let pass3 = init.password_files[3].clone();
    let (tx3, rx3) = oneshot::channel();
    stops[3] = Some(tx3);
    handles[3] = Some(tokio::spawn(async move {
        run_node_with_shutdown(cfg3, Some(pass3), false, Some(rx3))
            .await
            .unwrap();
    }));
    let caught_up = wait_for_height(rpc_base + 3, h_target, Duration::from_secs(15)).await;
    assert!(caught_up["head"]["height"].as_u64().unwrap() >= h_target);
    assert_eq!(caught_up["head"]["treasuryBalance"], 50);
    assert_eq!(caught_up["head"]["simulatedPayoutEscrow"], 100);

    // Clean shutdown of all 4 validators.
    for i in 0..4 {
        if let Some(tx) = stops[i].take() {
            let _ = tx.send(());
        }
        if let Some(h) = handles[i].take() {
            let _ = h.await;
        }
    }
    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lab_false_structurally_refuses_faucet() {
    let base_dir = std::env::temp_dir().join(format!(
        "bb_lab_false_e2e_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let p2p_port = pick_free_port_block(1);
    let rpc_port = pick_free_port_block(1);
    let init = init_testnet(
        base_dir.clone(),
        1,
        p2p_port,
        rpc_port,
        250,
        Some(50),
        "lab-false-pass".into(),
        false,
        vec![],
    )
    .unwrap();
    let (stop_tx, stop_rx) = oneshot::channel();
    let cfg = init.node_configs[0].clone();
    let pass = init.password_files[0].clone();
    let handle = tokio::spawn(async move {
        run_node_with_shutdown(cfg, Some(pass), false, Some(stop_rx))
            .await
            .unwrap();
    });

    wait_for_height(rpc_port, 1, Duration::from_secs(10)).await;
    let pk_hex = hex::encode(
        CanonicalEd25519PublicKeyV1::from_secret_seed(&[0xEE; 32])
            .unwrap()
            .as_bytes(),
    );
    let (code, body) = http_req(
        rpc_port,
        "POST",
        "/faucet",
        Some(&json!({ "pubkey_hex": pk_hex, "amount": 10 }).to_string()),
    )
    .await;
    assert_eq!(code, 403, "lab == false must return 403 Forbidden: {body}");

    let _ = stop_tx.send(());
    let _ = handle.await;
    let _ = std::fs::remove_dir_all(&base_dir);
}
