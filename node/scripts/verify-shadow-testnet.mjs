#!/usr/bin/env node
/**
 * End-to-end localhost 4-validator Bitblock L1 shadow testnet + Node 22 Bits relayer
 * + optional live headless Chrome `/l1.html` visual verifier (PR #108).
 *
 * STRICT BOUNDARY:
 * - Strictly localhost (127.0.0.1 ephemeral ports; zero tunnel/DNS creation).
 * - Testnet/devnet only (`lab: true`, `DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`).
 * - Non-blocking Chrome `spawn` with hard 25s SIGKILL timeout (never blocks HTTP server).
 * - Guaranteed SIGTERM/SIGKILL cleanup of all child processes on exit or error.
 */
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import http from 'node:http';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { execFileSync, spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { loadLightWasm, relayerKeyFromSeedHex, sha256Hex } from '../relayer/lib.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, '../..');
const WASM_PATH = path.join(
  REPO_ROOT,
  'node/target/wasm32-unknown-unknown/release/bitblock_light_wasm.wasm',
);
const VERIFIER_HTML = path.join(__dirname, 'l1-shadow-verifier.html');

// Locate bitblock-node binary (prefer release if present, else debug).
function findNodeBin() {
  const candidates = [
    process.env.BITBLOCK_NODE_BIN,
    path.join(REPO_ROOT, 'node/target/release/bitblock-node'),
    path.join(REPO_ROOT, 'node/target/debug/bitblock-node'),
  ].filter(Boolean);
  for (const c of candidates) {
    if (fs.existsSync(c)) return c;
  }
  throw new Error('bitblock-node binary not found; run cargo build --manifest-path node/Cargo.toml -p bitblock-node');
}

function getArg(flag) {
  const idx = process.argv.indexOf(flag);
  return idx >= 0 && idx + 1 < process.argv.length ? process.argv[idx + 1] : null;
}

const screenshotPath = getArg('--screenshot');
const CHROME_BIN = process.env.CHROME_BIN || 'google-chrome';
const CHROME_TIMEOUT_MS = Number(process.env.CHROME_TIMEOUT_MS || 25_000);

async function probeFreePort(port) {
  return new Promise((resolve) => {
    const srv = net.createServer();
    srv.once('error', () => resolve(false));
    srv.once('listening', () => {
      srv.close(() => resolve(true));
    });
    srv.listen(port, '127.0.0.1');
  });
}

async function pickContiguousPorts(count) {
  for (let attempt = 0; attempt < 40; attempt++) {
    const base = 22000 + Math.floor(Math.random() * 25000);
    let ok = true;
    for (let i = 0; i < count; i++) {
      if (!(await probeFreePort(base + i))) {
        ok = false;
        break;
      }
    }
    if (ok) return base;
  }
  throw new Error('unable to find contiguous free localhost ports');
}

async function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function waitFor(label, timeoutMs, fn) {
  const deadline = Date.now() + timeoutMs;
  let lastErr = '';
  while (Date.now() < deadline) {
    try {
      const res = await fn();
      if (res) return res;
    } catch (e) {
      lastErr = String(e.message || e);
    }
    await sleep(150);
  }
  throw new Error(`Timed out waiting for ${label} after ${timeoutMs}ms (lastErr=${lastErr})`);
}

function signIdentityBindPop(seedHex, chainId, genesisHex, actorIdHex, providerCode, extHashHex) {
  const key = relayerKeyFromSeedHex(seedHex);
  const u64 = (n) => {
    const b = Buffer.alloc(8);
    b.writeBigUInt64BE(BigInt(n));
    return b;
  };
  const msg = crypto
    .createHash('sha256')
    .update(Buffer.from('bb.bits1.identity_bind.v1', 'ascii'))
    .update(u64(chainId))
    .update(Buffer.from(genesisHex, 'hex'))
    .update(Buffer.from(actorIdHex, 'hex'))
    .update(Buffer.from(key.pubkeyHex, 'hex'))
    .update(Buffer.from([providerCode]))
    .update(Buffer.from(extHashHex, 'hex'))
    .digest();
  const priv = crypto.createPrivateKey({
    key: Buffer.from(key.pkcs8Hex, 'hex'),
    type: 'pkcs8',
    format: 'der',
  });
  return crypto.sign(null, msg, priv).toString('hex');
}

function runChromeWithTimeout(args, timeoutMs) {
  return new Promise((resolve, reject) => {
    const child = spawn(CHROME_BIN, args, { stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      child.kill('SIGKILL');
    }, timeoutMs);
    child.stdout.on('data', (d) => { stdout += d.toString(); });
    child.stderr.on('data', (d) => { stderr += d.toString(); });
    child.on('error', (err) => {
      clearTimeout(timer);
      reject(err);
    });
    child.on('close', (code, signal) => {
      clearTimeout(timer);
      if (timedOut) {
        reject(new Error(`Chrome timed out after ${timeoutMs}ms (SIGKILL). stderr:\n${stderr}`));
      } else if (code !== 0) {
        reject(new Error(`Chrome exited code=${code} signal=${signal}. stderr:\n${stderr}`));
      } else {
        resolve({ stdout, stderr });
      }
    });
  });
}

const nodeBin = findNodeBin();
assert.ok(fs.existsSync(WASM_PATH), `Missing compiled WASM at ${WASM_PATH}`);
const wasm = await loadLightWasm(WASM_PATH);

const workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'bb-shadow-testnet-'));
const children = [];
let mockGlServer = null;
let demoServer = null;

function spawnManaged(cmd, args, opts = {}) {
  const child = spawn(cmd, args, { stdio: ['ignore', 'pipe', 'pipe'], ...opts });
  children.push(child);
  return child;
}

async function killChild(child) {
  if (!child || child.exitCode !== null || child.killed) return;
  child.kill('SIGTERM');
  const exited = await new Promise((resolve) => {
    const t = setTimeout(() => resolve(false), 3_000);
    child.once('exit', () => {
      clearTimeout(t);
      resolve(true);
    });
  });
  if (!exited) {
    child.kill('SIGKILL');
  }
}

try {
  const p2pBase = await pickContiguousPorts(4);
  const rpcBase = await pickContiguousPorts(4);
  const relayerHealthPort = await pickContiguousPorts(1);
  const relayerSeedHex = '42'.repeat(32);
  const relayerKey = relayerKeyFromSeedHex(relayerSeedHex);

  // 1. Initialize 4-validator localhost shadow testnet authorizing our relayer pubkey for signup grants.
  execFileSync(
    nodeBin,
    [
      'init-testnet',
      '--dir', workDir,
      '--validators', '4',
      '--base-p2p-port', String(p2pBase),
      '--base-rpc-port', String(rpcBase),
      '--block-ms', '350',
      '--keystore-password', 'shadow-e2e-pass',
      '--grant-relayer-pk', relayerKey.pubkeyHex,
    ],
    { stdio: 'pipe' },
  );

  const nodeProcs = [];
  for (let i = 0; i < 4; i++) {
    nodeProcs.push(
      spawnManaged(nodeBin, [
        'run',
        '--config', path.join(workDir, `node-${i}`, 'node.json'),
        '--password-file', path.join(workDir, `node-${i}`, 'keystore.pass'),
      ]),
    );
  }

  const rpc0 = `http://127.0.0.1:${rpcBase}`;
  const rpc3 = `http://127.0.0.1:${rpcBase + 3}`;

  // 2. Wait for 4-validator finality & verify Ed25519 HotStuff QC via bitblock-light-wasm!
  const proof0 = await waitFor('4-validator finality (height >= 2)', 20_000, async () => {
    const r = await fetch(`${rpc0}/proof`);
    if (!r.ok) return null;
    const j = await r.json();
    return j.head?.height >= 2 ? j : null;
  });
  assert.equal(proof0.protocol, 'bitblock-l1-v1');
  assert.equal(proof0.lab, true);
  assert.equal(proof0.unit.symbol, 'BITS');

  const valsResp = await fetch(`${rpc0}/proof/validators`).then((r) => r.json());
  assert.equal(
    wasm.verifyHeaderQc({ header: proof0.head.header, qc: proof0.head.qc, committee: valsResp.committee }),
    1,
    'Live 4-validator Ed25519 HotStuff QC must verify via bitblock-light-wasm',
  );

  // 3. Spin up localhost mock Gameladder intent + proof-door server.
  const buyerKey = relayerKeyFromSeedHex('a1'.repeat(32));
  const creatorKey = relayerKeyFromSeedHex('c1'.repeat(32));
  const pendingIntents = [
    {
      kind: 'signup_grant',
      intentId: 'signup-buyer-1',
      grantId: sha256Hex('signup-buyer-1'),
      recipientPubkeyHex: buyerKey.pubkeyHex,
      amountBits: 1000,
    },
  ];
  const mockGlState = {
    height: 7,
    root: sha256Hex('gameladder-epoch-7'),
    eventCount: 12,
  };

  const mockGateKey = 'synthetic-ci-shadow-gate-key';
  const mockGateEnvFile = path.join(workDir, 'shared-relayer.env');
  fs.writeFileSync(mockGateEnvFile, `GAMELADDER_GATE_KEY=${mockGateKey}\n`, { mode: 0o600 });

  mockGlServer = http.createServer((req, res) => {
    if (req.headers['x-gate-key'] !== mockGateKey) {
      res.writeHead(401, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ ok: false, reason: 'missing or wrong x-gate-key' }));
      return;
    }
    if (req.url === '/api/bits/relayer/pending-intents') {
      res.writeHead(200, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ ok: true, intents: pendingIntents }));
      return;
    }
    if (req.url === '/api/market/proof') {
      res.writeHead(200, { 'content-type': 'application/json' });
      res.end(
        JSON.stringify({
          ok: true,
          head: {
            count: mockGlState.height,
            root: mockGlState.root,
            eventCount: mockGlState.eventCount,
            l1: { height: mockGlState.height, head: mockGlState.root },
          },
        }),
      );
      return;
    }
    res.writeHead(404);
    res.end('not found');
  });
  await new Promise((r) => mockGlServer.listen(0, '127.0.0.1', r));
  const mockGlPort = mockGlServer.address().port;

  // 4. Spawn real Node 22 Bits relayer sidecar pointing at localhost node-0 + mock Gameladder!
  const relayerDataDir = path.join(workDir, 'relayer-data');
  spawnManaged(process.execPath, [path.join(REPO_ROOT, 'node/relayer/index.mjs')], {
    env: {
      ...process.env,
      GAMELADDER_URL: `http://127.0.0.1:${mockGlPort}`,
      GATE_KEY_ENV_FILE: mockGateEnvFile,
      L1_RPC_URL: rpc0,
      WASM_PATH,
      POLL_MS: '350',
      HEALTH_HOST: '127.0.0.1',
      HEALTH_PORT: String(relayerHealthPort),
      RELAYER_DATA_DIR: relayerDataDir,
      RELAYER_SEED_HEX: relayerSeedHex,
    },
  });

  // Wait for relayer health + 1,000 Bits signup grant + audit-only L2 anchor!
  const relayerUrl = `http://127.0.0.1:${relayerHealthPort}`;
  await waitFor('relayer active with signup grant & L2 anchor', 15_000, async () => {
    const r = await fetch(`${relayerUrl}/health`);
    if (!r.ok) return null;
    const h = await r.json();
    return h.ok && h.mode === 'active' && h.signupGrantsRelayed >= 1 && h.anchoredCount >= 1 ? h : null;
  });

  // 5. Verify buyer received 1,000 Bits via Amendment 1 dual lookup + Patricia proof!
  const buyerWallet = await waitFor('buyer 1,000 Bits signup grant commitment', 15_000, async () => {
    const w = await fetch(`${rpc0}/proof/wallet/${buyerKey.pubkeyHex}`).then((r) => r.json());
    return w.balance === 1000 ? w : null;
  });
  const buyerByActor = await fetch(`${rpc0}/proof/wallet/${buyerWallet.actorId}`).then((r) => r.json());
  assert.equal(buyerWallet.actorId, buyerByActor.actorId);
  assert.equal(wasm.verifyWalletProof(buyerWallet), 1);

  const creatorWallet0 = await fetch(`${rpc0}/proof/wallet/${creatorKey.pubkeyHex}`).then((r) => r.json());
  const creatorActor = creatorWallet0.actorId;
  assert.equal(wasm.verifyWalletProof(creatorWallet0), 1, 'unfunded non-membership Patricia proof must verify');

  // 6. Queue buyer-signed atomic 80/20 BitsPurchaseSplitV1 (250 -> creator +200, treasury +50) through relayer!
  const orderIdHex = sha256Hex('shadow-order-1');
  const orderMemoHex = sha256Hex('gameladder-order/v1:world-1:item-sword');
  const buyEnc = wasm.signAndEncodeTx(
    {
      chain_id: proof0.chainId,
      genesis_id: proof0.genesis,
      sender_actor: buyerWallet.actorId,
      signer_pubkey: buyerKey.pubkeyHex,
      seq: 1,
      valid_until_height: 0,
      payload: {
        kind: 'purchase_split',
        buyer: buyerWallet.actorId,
        creator: creatorActor,
        treasury: proof0.systemActors.platformTreasury,
        amount_bits: 250,
        creator_bps: 8000,
        order_id: orderIdHex,
        memo_hash: orderMemoHex,
      },
    },
    buyerKey.pkcs8Hex,
  );
  pendingIntents.push({ kind: 'signed_tx', intentId: 'order-1', txHex: buyEnc.txHex });

  const buyReceipt = await waitFor('80/20 PurchaseSplit commitment', 15_000, async () => {
    const r = await fetch(`${rpc0}/proof/tx/${buyEnc.txHashHex}`);
    return r.ok ? r.json() : null;
  });
  assert.equal(buyReceipt.receipt.kind, 'purchase_split');
  assert.equal(buyReceipt.receipt.creator_bits_paid, 200);
  assert.equal(buyReceipt.receipt.platform_bits_paid, 50);

  // 7. Creator binds key (Amendment 1 PoP) and submits SimulatedPayout (100 Bits -> escrow)!
  const creatorExtHash = sha256Hex('studio:creator-alice');
  const creatorPopHex = signIdentityBindPop(
    'c1'.repeat(32),
    proof0.chainId,
    proof0.genesis,
    creatorActor,
    2, // StudioHandle
    creatorExtHash,
  );
  const bindEnc = wasm.signAndEncodeTx(
    {
      chain_id: proof0.chainId,
      genesis_id: proof0.genesis,
      sender_actor: creatorActor,
      signer_pubkey: creatorKey.pubkeyHex,
      seq: 1,
      valid_until_height: 0,
      payload: {
        kind: 'identity_bind',
        actor_id: creatorActor,
        ed25519_pk: creatorKey.pubkeyHex,
        provider: 2,
        external_id_hash: creatorExtHash,
        pop_signature: creatorPopHex,
      },
    },
    creatorKey.pkcs8Hex,
  );
  pendingIntents.push({ kind: 'signed_tx', intentId: 'creator-bind-1', txHex: bindEnc.txHex });
  await waitFor('creator IdentityBind commitment', 15_000, async () => {
    const r = await fetch(`${rpc0}/proof/tx/${bindEnc.txHashHex}`);
    return r.ok ? r.json() : null;
  });

  const payoutEnc = wasm.signAndEncodeTx(
    {
      chain_id: proof0.chainId,
      genesis_id: proof0.genesis,
      sender_actor: creatorActor,
      signer_pubkey: creatorKey.pubkeyHex,
      seq: 2,
      valid_until_height: 0,
      payload: {
        kind: 'simulated_payout',
        creator: creatorActor,
        amount_bits: 100,
        payout_request_id: sha256Hex('sim-payout-1'),
      },
    },
    creatorKey.pkcs8Hex,
  );
  pendingIntents.push({ kind: 'signed_tx', intentId: 'creator-payout-1', txHex: payoutEnc.txHex });
  const payoutReceipt = await waitFor('creator SimulatedPayout commitment', 15_000, async () => {
    const r = await fetch(`${rpc0}/proof/tx/${payoutEnc.txHashHex}`);
    return r.ok ? r.json() : null;
  });
  assert.equal(payoutReceipt.receipt.kind, 'simulated_payout');
  assert.equal(payoutReceipt.receipt.amount_bits, 100);

  // Verify final balances & exact conservation!
  const headAfter = await fetch(`${rpc0}/proof`).then((r) => r.json());
  assert.equal(headAfter.head.treasuryBalance, 50);
  assert.equal(headAfter.head.simulatedPayoutEscrow, 100);
  assert.equal(headAfter.head.grantPoolRemaining, 1_000_000_000 - 1_000);

  // 8. Hardening checks: 413 on >64 KiB POST body & <=100 /proof/blocks cap.
  const bigRes = await fetch(`${rpc0}/tx`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ tx_hex: 'aa'.repeat(40_000) }),
  });
  assert.equal(bigRes.status, 413, 'oversized POST body must return 413');
  const blocksPage = await fetch(`${rpc0}/proof/blocks?from=1&limit=999`).then((r) => r.json());
  assert.ok(blocksPage.blocks.length <= 100);

  // 9. Validator kill -> 3-of-4 continues finalizing -> cold restart catch-up via libp2p sync!
  const heightBeforeKill = headAfter.head.height;
  await killChild(nodeProcs[3]);
  const afterKill = await waitFor('3-of-4 finality while node-3 stopped', 15_000, async () => {
    const p = await fetch(`${rpc0}/proof`).then((r) => r.json());
    return p.head.height >= heightBeforeKill + 3 ? p : null;
  });
  const catchupTarget = afterKill.head.height;

  // Cold restart node-3!
  nodeProcs[3] = spawnManaged(nodeBin, [
    'run',
    '--config', path.join(workDir, 'node-3', 'node.json'),
    '--password-file', path.join(workDir, 'node-3', 'keystore.pass'),
  ]);
  const node3Caught = await waitFor('node-3 restart catch-up', 15_000, async () => {
    const r = await fetch(`${rpc3}/proof`);
    if (!r.ok) return null;
    const p = await r.json();
    return p.head?.height >= catchupTarget ? p : null;
  });
  assert.equal(node3Caught.head.treasuryBalance, 50);
  assert.equal(node3Caught.head.simulatedPayoutEscrow, 100);

  // 9b. Run Phase 2 health/invariant watchdog directly against the live 4-node cluster + relayer!
  const healthOut = execFileSync(
    process.execPath,
    [
      path.join(__dirname, 'check-bits-testnet-health.mjs'),
      '--rpc', rpc0,
      '--node-ports', [rpcBase, rpcBase + 1, rpcBase + 2, rpcBase + 3].join(','),
      '--relayer', `${relayerUrl}/health`,
      '--strict-relayer',
      '--advance-ms', '1500',
    ],
    { encoding: 'utf8' },
  );
  const healthJson = JSON.parse(healthOut);
  assert.equal(healthJson.ok, true, `check-bits-testnet-health failed: ${healthOut}`);
  assert.equal(healthJson.qcVerified, true);
  assert.equal(healthJson.conservationOk, true);
  assert.equal(healthJson.bodyCapStatus, 413);
  assert.equal(healthJson.relayerMode, 'active');

  // 10. Optional live headless Chrome `/l1.html` visual verification against running node-0!
  if (screenshotPath) {
    demoServer = http.createServer((req, res) => {
      const urlPath = (req.url || '/').split('?')[0];
      if (urlPath === '/' || urlPath === '/l1.html') {
        res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
        res.end(fs.readFileSync(VERIFIER_HTML));
        return;
      }
      if (urlPath === '/bitblock_light_wasm.wasm') {
        res.writeHead(200, { 'content-type': 'application/wasm' });
        res.end(fs.readFileSync(WASM_PATH));
        return;
      }
      res.writeHead(404);
      res.end('not found');
    });
    await new Promise((r) => demoServer.listen(0, '127.0.0.1', r));
    const demoPort = demoServer.address().port;
    const targetUrl =
      `http://127.0.0.1:${demoPort}/l1.html` +
      `?rpc=${encodeURIComponent(rpc0)}` +
      `&relayer=${encodeURIComponent(relayerUrl)}` +
      `&buyer=${buyerKey.pubkeyHex}` +
      `&creator=${creatorKey.pubkeyHex}` +
      `&orderTx=${buyEnc.txHashHex}` +
      `&payoutTx=${payoutEnc.txHashHex}`;

    const commonFlags = [
      '--headless=new',
      '--no-sandbox',
      '--disable-gpu',
      '--disable-dev-shm-usage',
      '--window-size=1280,760',
      '--virtual-time-budget=5000',
    ];
    const domRes = await runChromeWithTimeout([...commonFlags, '--dump-dom', targetUrl], CHROME_TIMEOUT_MS);
    assert.ok(
      domRes.stdout.includes('ALL 10 LIVE SHADOW TESTNET CHECKS VERIFIED GREEN'),
      'Chrome /l1.html verifier did not reach ALL 10 LIVE SHADOW TESTNET CHECKS VERIFIED GREEN',
    );
    fs.mkdirSync(path.dirname(path.resolve(screenshotPath)), { recursive: true });
    await runChromeWithTimeout(
      [...commonFlags, `--screenshot=${path.resolve(screenshotPath)}`, targetUrl],
      CHROME_TIMEOUT_MS,
    );
    const st = fs.statSync(screenshotPath);
    assert.ok(st.size > 5_000, `screenshot too small: ${st.size} B`);
    console.log(`SHADOW_L1_CHROME_SCREENSHOT=PASS png=${screenshotPath} bytes=${st.size}`);
  }

  console.log(
    `SHADOW_TESTNET_VERIFY=PASS height=${node3Caught.head.height} treasury=${node3Caught.head.treasuryBalance} escrow=${node3Caught.head.simulatedPayoutEscrow} grantPoolRemaining=${node3Caught.head.grantPoolRemaining}`,
  );
} finally {
  if (demoServer) demoServer.close();
  if (mockGlServer) mockGlServer.close();
  for (const c of children) {
    await killChild(c);
  }
  fs.rmSync(workDir, { recursive: true, force: true });
}
