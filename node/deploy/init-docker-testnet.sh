#!/usr/bin/env bash
# Generate a 4-validator localhost-only Bitblock L1 shadow testnet config for docker-compose.testnet.yml.
# Pre-generates an isolated bb-relayer Ed25519 key (0600) and authorizes it in genesis.json
# for lab-only TestnetGrantPool signup grants. Does NOT copy any Gameladder secret or create
# any Cloudflare Tunnel / DNS record.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
NODE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT="$NODE_DIR/.runtime/docker-testnet"
RELAYER_DIR="$NODE_DIR/.runtime/relayer"
RELAYER_KEY_FILE="$RELAYER_DIR/relayer-key.json"
BIN="$NODE_DIR/target/release/bitblock-node"
WASM_OUT="$NODE_DIR/target/wasm32-unknown-unknown/release/bitblock_light_wasm.wasm"

if [[ ! -x "$BIN" ]]; then
  echo "Building bitblock-node release binary..."
  cargo build --release --locked --manifest-path "$NODE_DIR/Cargo.toml" -p bitblock-node
fi

if [[ ! -f "$WASM_OUT" ]]; then
  echo "Building bitblock-light-wasm release artifact..."
  cargo build --release --locked --manifest-path "$NODE_DIR/Cargo.toml" -p bitblock-light-wasm --target wasm32-unknown-unknown
fi

mkdir -p "$RELAYER_DIR"
chmod 700 "$RELAYER_DIR"
RELAYER_PUBKEY="$(node --input-type=module - "$RELAYER_KEY_FILE" "$NODE_DIR/relayer/lib.mjs" <<'EOF'
import fs from 'node:fs';
const [keyFile, libPath] = process.argv.slice(2);
const { generateRelayerKey } = await import(`file://${libPath}`);
let key;
if (fs.existsSync(keyFile)) {
  key = JSON.parse(fs.readFileSync(keyFile, 'utf8'));
} else {
  key = generateRelayerKey();
  fs.writeFileSync(keyFile, JSON.stringify(key, null, 2) + '\n', { mode: 0o600 });
  fs.chmodSync(keyFile, 0o600);
}
process.stdout.write(key.pubkeyHex);
EOF
)"
echo "Relayer pubkey (authorized in genesis.json): $RELAYER_PUBKEY"

rm -rf "$OUT"
BLOCK_MS="${BLOCK_MS:-1500}"
KEYSTORE_PASS="${BB_KEYSTORE_PASSWORD:-$(openssl rand -hex 24)}"
"$BIN" init-testnet \
  --dir "$OUT" \
  --validators 4 \
  --base-p2p-port 19100 \
  --base-rpc-port 8645 \
  --block-ms "$BLOCK_MS" \
  --keystore-password "$KEYSTORE_PASS" \
  --grant-relayer-pk "$RELAYER_PUBKEY"

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

node --input-type=module - "$WASM_OUT" <<'EOF'
import crypto from 'node:crypto';
import fs from 'node:fs';
import zlib from 'node:zlib';
const raw = fs.readFileSync(process.argv[2]);
const gz = zlib.gzipSync(raw, { level: 9 });
const sha = crypto.createHash('sha256').update(raw).digest('hex');
console.log(`bitblock_light_wasm.wasm: raw=${raw.length} B gzip=${gz.length} B sha256=${sha}`);
EOF
