#!/usr/bin/env node
// Standalone zero-dependency Node 22 Gameladder -> Bitblock L1 Bits shadow testnet relayer sidecar.
// Testnet/devnet only (`lab: true`, `DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`).
// Holds an isolated Ed25519 relayer key locally (0600); zero keys ever touch Cloudflare Workers.

import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import {
  DEFAULT_WASM_PATH,
  generateRelayerKey,
  loadLightWasm,
  relayerKeyFromSeedHex,
  runBitsRelayerCycle,
} from './lib.mjs';

const GAMELADDER_URL = process.env.GAMELADDER_URL || 'http://127.0.0.1:8787';
const GAMELADDER_GATE_KEY_ENV = process.env.GAMELADDER_GATE_KEY || '';
const GATE_KEY_ENV_FILE = process.env.GATE_KEY_ENV_FILE || '';
const L1_RPC_URL = process.env.L1_RPC_URL || 'http://127.0.0.1:8640';

/**
 * Resolve the current Gameladder soft-launch gate key without ever logging its value.
 * Prefers `GATE_KEY_ENV_FILE` (read-only mount of shared `rngcoin-node/.runtime/relayer/relayer.env`)
 * so Owen's single `sync-relayer-gate-key.sh` run activates both relayers live on the next tick.
 */
function resolveGateKey() {
  if (GATE_KEY_ENV_FILE) {
    try {
      const raw = fs.readFileSync(GATE_KEY_ENV_FILE, 'utf8');
      for (const line of raw.split(/\r?\n/)) {
        const trimmed = line.trim();
        if (!trimmed || trimmed.startsWith('#')) continue;
        if (trimmed.startsWith('GAMELADDER_GATE_KEY=')) {
          const val = trimmed
            .slice('GAMELADDER_GATE_KEY='.length)
            .replace(/^\uFEFF/, '')
            .replace(/^["']|["']$/g, '')
            .trim();
          if (val) return val;
        }
      }
    } catch {
      // File may not exist yet before Owner runs sync-relayer-gate-key.sh; fall back safely.
    }
  }
  return GAMELADDER_GATE_KEY_ENV;
}
const APP_ID = process.env.APP_ID || 'gameladder';
const WASM_PATH = process.env.WASM_PATH || DEFAULT_WASM_PATH;
const POLL_MS = Math.max(250, Number(process.env.POLL_MS || 5_000));
const HEALTH_HOST = process.env.HEALTH_HOST || '127.0.0.1';
const HEALTH_PORT = Number(process.env.HEALTH_PORT || 8690);
const DATA_DIR = process.env.RELAYER_DATA_DIR || path.resolve(process.cwd(), 'node/.runtime/relayer');
const KEY_FILE = process.env.RELAYER_KEY_FILE || path.join(DATA_DIR, 'relayer-key.json');
const STATE_FILE = process.env.RELAYER_STATE_FILE || path.join(DATA_DIR, 'relayer-state.json');

fs.mkdirSync(path.dirname(KEY_FILE), { recursive: true });
fs.mkdirSync(path.dirname(STATE_FILE), { recursive: true });

let relayerKey;
if (process.env.RELAYER_SEED_HEX) {
  relayerKey = relayerKeyFromSeedHex(process.env.RELAYER_SEED_HEX);
} else if (fs.existsSync(KEY_FILE)) {
  relayerKey = JSON.parse(fs.readFileSync(KEY_FILE, 'utf8'));
} else {
  relayerKey = generateRelayerKey();
  fs.writeFileSync(KEY_FILE, JSON.stringify(relayerKey, null, 2) + '\n', { mode: 0o600 });
  fs.chmodSync(KEY_FILE, 0o600);
}

let state = fs.existsSync(STATE_FILE)
  ? JSON.parse(fs.readFileSync(STATE_FILE, 'utf8'))
  : {
      lastAnchoredBlock: 0,
      lastAnchoredEpoch: 0,
      lastAnchoredRoot: null,
      anchoredCount: 0,
      signupGrantsRelayed: 0,
      intentsRelayed: 0,
      lastTxHash: null,
      processedIntentIds: [],
    };

const wasm = await loadLightWasm(WASM_PATH);

const status = {
  ok: true,
  protocol: 'bitblock-l1-v1',
  lab: true,
  custodyLabel: 'DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY',
  mode: 'starting',
  gameladderUrl: GAMELADDER_URL,
  l1RpcUrl: L1_RPC_URL,
  relayerPubkey: relayerKey.pubkeyHex,
  lastCheckAt: null,
  lastError: null,
};

function saveStateAtomic(next) {
  const tmp = `${STATE_FILE}.tmp`;
  fs.writeFileSync(tmp, JSON.stringify(next, null, 2) + '\n', { mode: 0o600 });
  fs.renameSync(tmp, STATE_FILE);
  state = next;
}

const log = (level, msg, extra = {}) => {
  console.log(JSON.stringify({ ts: new Date().toISOString(), level, msg, ...extra }));
};

let running = false;
async function tick() {
  if (running) return;
  running = true;
  try {
    const gateKey = resolveGateKey();
    const res = await runBitsRelayerCycle({
      gameladderUrl: GAMELADDER_URL,
      gateKey,
      l1RpcUrl: L1_RPC_URL,
      relayerKey,
      wasm,
      state,
      appId: APP_ID,
    });
    status.lastCheckAt = new Date().toISOString();
    if (res.standbyGate) {
      status.mode = 'standby_gate';
      status.lastError = res.reason;
    } else if (!res.ok) {
      status.mode = 'degraded';
      status.lastError = res.reason;
      log('warn', 'bits relayer cycle error', { reason: res.reason });
    } else {
      status.mode = 'active';
      status.lastError = null;
      if (res.nextState) saveStateAtomic(res.nextState);
      if (res.submittedTxs && res.submittedTxs.length > 0) {
        log('info', 'relayed Bits transactions to Bitblock L1', {
          count: res.submittedTxs.length,
          signupGrantsCount: res.signupGrantsCount,
          signedIntentsCount: res.signedIntentsCount,
          anchored: res.anchored,
          lastTxHash: res.lastTxHash,
        });
      }
    }
  } catch (e) {
    status.lastCheckAt = new Date().toISOString();
    status.mode = 'degraded';
    status.lastError = String(e.message || e);
    log('warn', 'bits relayer exception', { error: status.lastError });
  } finally {
    running = false;
  }
}

const server = http.createServer((req, res) => {
  if (req.url === '/health') {
    res.writeHead(200, {
      'content-type': 'application/json',
      'cache-control': 'no-store',
      'access-control-allow-origin': '*',
    });
    res.end(
      JSON.stringify({
        ...status,
        lastAnchoredBlock: state.lastAnchoredBlock,
        lastAnchoredEpoch: state.lastAnchoredEpoch,
        anchoredCount: state.anchoredCount,
        signupGrantsRelayed: state.signupGrantsRelayed,
        intentsRelayed: state.intentsRelayed,
        lastTxHash: state.lastTxHash,
      }),
    );
    return;
  }
  res.writeHead(404);
  res.end('not found');
});

server.listen(HEALTH_PORT, HEALTH_HOST, () => {
  log('info', 'bitblock-bits-relayer online', {
    healthHost: HEALTH_HOST,
    healthPort: HEALTH_PORT,
    relayerPubkey: relayerKey.pubkeyHex,
    gameladderUrl: GAMELADDER_URL,
    l1RpcUrl: L1_RPC_URL,
  });
  tick();
  const timer = setInterval(tick, POLL_MS);
  const shutdown = () => {
    clearInterval(timer);
    server.close(() => process.exit(0));
  };
  process.on('SIGTERM', shutdown);
  process.on('SIGINT', shutdown);
});
