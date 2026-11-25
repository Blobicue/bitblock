#!/usr/bin/env bash
# Generate a 4-validator localhost-only Bitblock L1 shadow testnet config for docker-compose.testnet.yml.
# Testnet / devnet ONLY (`lab: true`). Does not create or route any tunnel or DNS name.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
NODE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT="$NODE_DIR/.runtime/docker-testnet"
BIN="$NODE_DIR/target/release/bitblock-node"

if [[ ! -x "$BIN" ]]; then
  echo "Building bitblock-node release binary..."
  cargo build --release --locked --manifest-path "$NODE_DIR/Cargo.toml" -p bitblock-node
fi

rm -rf "$OUT"
BLOCK_MS="${BLOCK_MS:-1500}"
KEYSTORE_PASS="${BB_KEYSTORE_PASSWORD:-$(openssl rand -hex 24)}"
"$BIN" init-testnet \
  --dir "$OUT" \
  --validators 4 \
  --base-p2p-port 19100 \
  --base-rpc-port 8645 \
  --block-ms "$BLOCK_MS" \
  --keystore-password "$KEYSTORE_PASS"

chmod 600 "$OUT"/node-*/keystore.pass "$OUT"/node-*/validator-keystore.json

node - "$OUT" <<'EOF'
const fs = require('node:fs');
const path = require('node:path');
const out = process.argv[2];
for (let i = 0; i < 4; i++) {
  const p = path.join(out, `node-${i}`, 'node.json');
  const cfg = JSON.parse(fs.readFileSync(p, 'utf8'));
  const p2pPort = 19100 + i;
  cfg.data_dir = '/data';
  cfg.genesis_path = '/data/genesis.json';
  cfg.validator_keystore_path = '/data/validator-keystore.json';
  cfg.p2p_listen = [
    `/ip4/0.0.0.0/tcp/${p2pPort}`,
    `/ip4/0.0.0.0/udp/${p2pPort}/quic-v1`,
  ];
  cfg.bootstrap_peers = [0, 1, 2, 3]
    .filter(j => j !== i)
    .map(j => `/dns4/node-${j}/tcp/${19100 + j}`);
  cfg.rpc_listen = '0.0.0.0:8645';
  fs.writeFileSync(p, JSON.stringify(cfg, null, 2) + '\n');
}
console.log('Rewrote node-{0..3}/node.json for localhost Docker shadow testnet + encrypted /data keystores.');
EOF
