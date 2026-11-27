#!/usr/bin/env node
/**
 * Watchdog & soak health/invariant checker for the merged Bitblock L1 Bits testnet cluster,
 * Caddy HA LB, and Node 22 Bits relayer sidecar (Phase 2).
 *
 * Verifies:
 *  1. >= 3/4 (warns if < 4/4) local validators online with peers >= 2, protocol=bitblock-l1-v1, lab=true
 *  2. Finalized height strictly advances over `--advance-ms`
 *  3. Live Ed25519 HotStuff QC passes local cryptographic verification via `bitblock_light_wasm.wasm`
 *  4. SIGNER-OPS-1 watermark counter (`bitblock_wal_writes_total`) active and supply conservation holds
 *  5. HTTP hardening caps hold (`413` on >64 KiB POST /tx; <=100 blocks on `/proof/blocks?limit=999`)
 *  6. Relayer `/health` reports `active` (warns on `standby_gate` unless `--strict-relayer`; fails on `degraded`)
 *
 * Exits 0 with structured JSON when healthy; exits 1 with explicit `issues[]` & `rollbackRecommended`.
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadLightWasm, resolveDefaultWasmPath } from '../relayer/lib.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

function getArg(flag, fallback = null) {
  const idx = process.argv.indexOf(flag);
  return idx >= 0 && idx + 1 < process.argv.length ? process.argv[idx + 1] : fallback;
}

const RPC_URL = (getArg('--rpc', process.env.BITS_RPC_URL || 'http://127.0.0.1:8640')).replace(/\/+$/, '');
const RELAYER_URL = getArg('--relayer', process.env.BITS_RELAYER_HEALTH_URL || 'http://127.0.0.1:8690/health');
const NODE_PORTS_CSV = getArg('--node-ports', process.env.BITS_NODE_PORTS || '8645,8646,8647,8648');
const NODE_PORTS = NODE_PORTS_CSV.split(',').map((s) => Number(s.trim())).filter((n) => Number.isFinite(n) && n > 0);
const ADVANCE_WAIT_MS = Number(getArg('--advance-ms', process.env.ADVANCE_WAIT_MS || '4500'));
const WASM_PATH = getArg('--wasm', process.env.WASM_PATH || resolveDefaultWasmPath());
const STRICT_RELAYER = process.argv.includes('--strict-relayer');
const SKIP_RELAYER = process.argv.includes('--skip-relayer');

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function fetchJson(url, opts = {}, timeoutMs = 5000) {
  const ctl = new AbortController();
  const t = setTimeout(() => ctl.abort(), timeoutMs);
  try {
    const res = await fetch(url, { ...opts, signal: ctl.signal });
    const text = await res.text();
    let json = null;
    try {
      json = JSON.parse(text);
    } catch {
      /* non-json body */
    }
    return { ok: res.ok, status: res.status, json, text };
  } catch (e) {
    return { ok: false, status: 0, error: String(e.message || e), json: null, text: '' };
  } finally {
    clearTimeout(t);
  }
}

const issues = [];
const warnings = [];

let wasm = null;
if (fs.existsSync(WASM_PATH)) {
  try {
    wasm = await loadLightWasm(WASM_PATH);
  } catch (e) {
    issues.push(`failed loading bitblock_light_wasm.wasm: ${e.message || e}`);
  }
} else {
  issues.push(`bitblock_light_wasm.wasm missing at ${WASM_PATH}`);
}

const [rpcHealth1, relRes, ...nodeHealths] = await Promise.all([
  fetchJson(`${RPC_URL}/health`),
  SKIP_RELAYER ? Promise.resolve({ skipped: true }) : fetchJson(RELAYER_URL),
  ...NODE_PORTS.map((p) => fetchJson(`http://127.0.0.1:${p}/health`)),
]);

const onlineNodes = nodeHealths.filter((n) => n.ok && n.json?.ok);
if (onlineNodes.length < Math.min(3, NODE_PORTS.length)) {
  issues.push(`only ${onlineNodes.length}/${NODE_PORTS.length} validators online (need >= 3 for BFT quorum)`);
} else if (onlineNodes.length < NODE_PORTS.length) {
  warnings.push(`${onlineNodes.length}/${NODE_PORTS.length} validators online (degraded redundancy)`);
}

for (let i = 0; i < nodeHealths.length; i++) {
  const n = nodeHealths[i];
  if (!n.ok || !n.json?.ok) continue;
  if (n.json.protocol !== 'bitblock-l1-v1') {
    issues.push(`node-${i} unexpected protocol=${n.json.protocol}`);
  }
  if (n.json.lab !== true) {
    issues.push(`node-${i} lab=${n.json.lab} (expected lab=true testnet boundary)`);
  }
  if (NODE_PORTS.length >= 4 && (n.json.peers ?? 0) < 2) {
    issues.push(`node-${i} peers=${n.json.peers} (< 2)`);
  }
}

if (!rpcHealth1.ok || !rpcHealth1.json?.ok) {
  issues.push(`RPC ${RPC_URL}/health failed: ${rpcHealth1.error || `HTTP ${rpcHealth1.status}`}`);
} else {
  if (rpcHealth1.json.protocol !== 'bitblock-l1-v1') {
    issues.push(`RPC protocol=${rpcHealth1.json.protocol} (expected bitblock-l1-v1)`);
  }
  if (rpcHealth1.json.lab !== true) {
    issues.push(`RPC lab=${rpcHealth1.json.lab} (expected lab=true)`);
  }
}

if (!SKIP_RELAYER) {
  if (!relRes.ok || !relRes.json?.ok) {
    issues.push(`relayer unreachable at ${RELAYER_URL}: ${relRes.error || `HTTP ${relRes.status}`}`);
  } else if (relRes.json.mode === 'standby_gate') {
    const msg = `relayer in standby_gate (awaiting Owner gate-key sync): ${relRes.json.lastError || 'HTTP 401'}`;
    if (STRICT_RELAYER) issues.push(msg);
    else warnings.push(msg);
  } else if (relRes.json.mode !== 'active') {
    issues.push(`relayer degraded: mode=${relRes.json.mode || 'unknown'} err=${relRes.json.lastError || 'none'}`);
  }
}

const hBefore = Number(rpcHealth1.json?.height || 0);
const walBefore = Number(rpcHealth1.json?.walWrites || 0);
await sleep(ADVANCE_WAIT_MS);

const [proof2, valsRes, metricsRes, blocksRes, bigPostRes] = await Promise.all([
  fetchJson(`${RPC_URL}/proof`),
  fetchJson(`${RPC_URL}/proof/validators`),
  fetchJson(`${RPC_URL}/metrics`),
  fetchJson(`${RPC_URL}/proof/blocks?from=1&limit=999`),
  fetchJson(`${RPC_URL}/tx`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ tx_hex: 'aa'.repeat(40_000) }),
  }),
]);

const hAfter = Number(proof2.json?.head?.height || 0);
let qcVerified = false;
let conservationOk = false;
let walWritesMetric = null;

if (!proof2.ok || !proof2.json?.ok) {
  issues.push(`RPC ${RPC_URL}/proof failed on second sample: ${proof2.error || `HTTP ${proof2.status}`}`);
} else {
  if (hAfter <= hBefore) {
    issues.push(`chain height stalled: ${hBefore} -> ${hAfter} over ${ADVANCE_WAIT_MS}ms`);
  }
  const head = proof2.json.head;
  const gp = Number(head?.grantPoolRemaining ?? -1);
  const tr = Number(head?.treasuryBalance ?? -1);
  const es = Number(head?.simulatedPayoutEscrow ?? -1);
  const sup = Number(head?.genesisSupply ?? 0);
  conservationOk = sup > 0 && gp >= 0 && tr >= 0 && es >= 0 && gp + tr + es <= sup;
  if (!conservationOk) {
    issues.push(`supply conservation bound violated: gp=${gp} treasury=${tr} escrow=${es} supply=${sup}`);
  }

  if (wasm && valsRes.ok && valsRes.json?.committee && head?.header && head?.qc) {
    const code = wasm.verifyHeaderQc({
      header: head.header,
      qc: head.qc,
      committee: valsRes.json.committee,
    });
    qcVerified = code === 1;
    if (!qcVerified) {
      issues.push(`live Ed25519 HotStuff QC failed bitblock-light-wasm verification (code=${code})`);
    }
  } else if (wasm) {
    issues.push('missing header/qc/committee for WASM QC verification');
  }
}

if (metricsRes.ok) {
  const m = metricsRes.text.match(/^bitblock_wal_writes_total\s+(\d+)/m);
  walWritesMetric = m ? Number(m[1]) : null;
  if (walWritesMetric === null || walWritesMetric < walBefore || walWritesMetric === 0) {
    issues.push(`SIGNER-OPS-1 bitblock_wal_writes_total anomaly: before=${walBefore} metric=${walWritesMetric}`);
  }
} else {
  issues.push(`RPC ${RPC_URL}/metrics failed: HTTP ${metricsRes.status}`);
}

const blocksReturned = Array.isArray(blocksRes.json?.blocks) ? blocksRes.json.blocks.length : -1;
if (!blocksRes.ok || blocksReturned < 0 || blocksReturned > 100) {
  issues.push(`/proof/blocks?limit=999 cap check failed: status=${blocksRes.status} count=${blocksReturned}`);
}

if (bigPostRes.status !== 413) {
  issues.push(`oversized >64 KiB POST /tx returned HTTP ${bigPostRes.status} (expected 413)`);
}

const report = {
  ok: issues.length === 0,
  rollbackRecommended: issues.length > 0,
  ts: new Date().toISOString(),
  rpcUrl: RPC_URL,
  protocol: proof2.json?.protocol ?? null,
  chainId: proof2.json?.chainId ?? null,
  lab: proof2.json?.lab ?? null,
  validatorsOnline: onlineNodes.length,
  validatorsExpected: NODE_PORTS.length,
  heightBefore: hBefore,
  heightAfter: hAfter,
  qcVerified,
  conservationOk,
  walWritesMetric,
  blocksPageCount: blocksReturned,
  bodyCapStatus: bigPostRes.status,
  relayerMode: SKIP_RELAYER ? 'skipped' : (relRes.json?.mode ?? null),
  relayerSignupGrants: SKIP_RELAYER ? null : (relRes.json?.signupGrantsRelayed ?? null),
  relayerAnchoredCount: SKIP_RELAYER ? null : (relRes.json?.anchoredCount ?? null),
  warnings,
  issues,
};

console.log(JSON.stringify(report, null, 2));
if (issues.length > 0) process.exit(1);
