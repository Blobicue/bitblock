# Bitblock L1 Shadow Testnet Runbook (Phase 1 — Localhost Only)

> **Boundary:** Strictly testnet/devnet (`lab: true`, `DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`).
> No `L1-PRODUCTION`, no `VALUE-1`, no mainnet genesis, no real-money payouts.
> All Phase 1 ports stay strictly bound to `127.0.0.1`. Tunnel credentials and DNS remain Owen's.

## Shadow Port Map (Zero Conflict With Legacy `rngcoin-node`)

| Service | Host Binding | Purpose |
|---|---|---|
| `rpc-lb` (Caddy) | `127.0.0.1:8640` | HA primary-with-failover HTTP proof-door LB (`node-0..3`) |
| `node-0..3` HTTP | `127.0.0.1:8645..8648` | Direct per-validator proof doors (`/health`, `/proof*`, `/metrics`, `POST /tx`, `POST /faucet`) |
| `node-0..3` P2P | `127.0.0.1:19100..19103` | `libp2p` TCP/Noise/Yamux + QUIC consensus & block sync |

## Localhost Native Launch (No Docker Required)

```bash
cargo build --release --manifest-path node/Cargo.toml -p bitblock-node
./node/target/release/bitblock-node init-testnet \
  --dir node/.runtime/local-testnet \
  --validators 4 \
  --base-p2p-port 19100 \
  --base-rpc-port 8645 \
  --block-ms 1000
```

Start each validator with its encrypted keystore password file:

```bash
for i in 0 1 2 3; do
  ./node/target/release/bitblock-node run \
    --config "node/.runtime/local-testnet/node-$i/node.json" \
    --password-file "node/.runtime/local-testnet/node-$i/keystore.pass" &
done
```

## Docker Compose Launch (Localhost Only)

```bash
chmod +x node/deploy/init-docker-testnet.sh
./node/deploy/init-docker-testnet.sh
BB_UID=$(id -u) BB_GID=$(id -g) docker compose -f node/deploy/docker-compose.testnet.yml up -d --build
curl -fsS http://127.0.0.1:8640/proof | jq .
```

The optional `named-tunnel` compose profile is an **inert template only** and is never enabled by
default; do not provision tunnels or DNS records during Phase 1.

## Safety & Crash-Recovery Invariants

- **ACID persistence:** Every finalized block, `tx_index`, `order_index`, and head pointer commits
  in one atomic `redb` write transaction; cold restart replays `BitsEconomyStateV1` from genesis and
  re-verifies every block's `AUTH-COMMIT-1` `balance_root` and `nonce_root`.
- **Anti-double-sign boundary (`SIGNER-OPS-1`):** Each validator holds an exclusive OS lock
  (`flock` / `share_mode(0)`) on `<data_dir>/signer/signer.lock` and atomically fsyncs
  `<data_dir>/signer/watermark.bin` BEFORE emitting any proposal or HotStuff commit vote signature.
- **Zero runtime minting:** Every faucet or signup grant debits strictly from `TestnetGrantPool`
  (`lab == true` only; structurally refused with HTTP `403` when `lab == false`).
