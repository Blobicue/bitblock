#!/usr/bin/env node
/**
 * Cross-platform 15-minute soak probe runner for a standalone Bitblock L1 Bits testnet host.
 *
 * Runs `node/scripts/check-bits-testnet-health.mjs`, normalizes the structured JSON record
 * expected by `tools/soak_report.mjs`, ensures `soak-manifest.json` exists without overwriting
 * an active soak's `startedAtUtc`, and appends one JSONL line to `soak-health.jsonl`.
 *
 * Strictly testnet/devnet (`lab: true`). Zero external dependencies (Node 22).
 */
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolveDefaultWasmPath } from '../relayer/lib.mjs';
import { evaluateSoakRecords, parseJsonlText } from '../../tools/soak_report.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, '../..');
const HEALTH_SCRIPT = path.join(__dirname, 'check-bits-testnet-health.mjs');
const DEFAULT_OUT = path.join(REPO_ROOT, 'node/.runtime/soak/soak-health.jsonl');
const DEFAULT_MANIFEST = path.join(REPO_ROOT, 'node/.runtime/soak/soak-manifest.json');

export function buildSoakProbeRecord(healthJson, probeExit, opts = {}) {
  const standalone = opts.standalone !== false;
  const nowIso = opts.nowIso || new Date().toISOString();
  if (!healthJson || typeof healthJson !== 'object') {
    return {
      ts: nowIso,
      probeExit: Number.isInteger(probeExit) ? probeExit : 1,
      ok: false,
      rollbackRecommended: true,
      standalone,
      protocol: null,
      chainId: null,
      heightBefore: null,
      heightAfter: null,
      validatorsOnline: 0,
      qcVerified: false,
      conservationOk: false,
      walWritesMetric: null,
      bodyCapStatus: 0,
      blocksPageCount: -1,
      relayerMode: null,
      relayerActive: false,
      issues: [opts.parseError || 'health probe emitted non-JSON output'],
    };
  }
  const relayerMode = healthJson.relayerMode ?? null;
  return {
    ts: healthJson.ts || nowIso,
    probeExit: Number.isInteger(probeExit) ? probeExit : (healthJson.ok ? 0 : 1),
    ok: healthJson.ok === true && probeExit === 0,
    rollbackRecommended: healthJson.rollbackRecommended === true || probeExit !== 0,
    standalone,
    protocol: healthJson.protocol ?? null,
    chainId: healthJson.chainId ?? null,
    heightBefore: healthJson.heightBefore ?? null,
    heightAfter: healthJson.heightAfter ?? null,
    validatorsOnline: healthJson.validatorsOnline ?? 0,
    qcVerified: healthJson.qcVerified === true,
    conservationOk: healthJson.conservationOk === true,
    walWritesMetric: healthJson.walWritesMetric ?? null,
    bodyCapStatus: healthJson.bodyCapStatus ?? 0,
    blocksPageCount: healthJson.blocksPageCount ?? -1,
    relayerMode,
    relayerActive: relayerMode === 'active',
    warnings: Array.isArray(healthJson.warnings) ? healthJson.warnings : [],
    issues: Array.isArray(healthJson.issues) ? healthJson.issues : [],
  };
}

export function ensureSoakManifest(manifestPath, opts = {}) {
  const force = Boolean(opts.force);
  if (!force && fs.existsSync(manifestPath)) {
    return JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  }
  fs.mkdirSync(path.dirname(manifestPath), { recursive: true });
  const manifest = {
    schema: 'bitblock/soak-manifest/v1',
    testnetOnly: true,
    mode: opts.standalone !== false ? 'standalone' : 'dual-stack',
    startedAtUtc: opts.startedAtUtc || new Date().toISOString(),
    rpcUrl: opts.rpcUrl || 'https://testnet-rpc.gameladder.io',
    chainId: 1112101971,
    blockMs: Number(opts.blockMs || 2000),
    host: os.hostname(),
  };
  fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n', 'utf8');
  return manifest;
}

export function appendSoakRecord(outPath, record) {
  fs.mkdirSync(path.dirname(outPath), { recursive: true });
  fs.appendFileSync(outPath, JSON.stringify(record) + '\n', 'utf8');
}

export function runSelfTest() {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'bb-soak-probe-test-'));
  try {
    const manifestPath = path.join(tmp, 'soak-manifest.json');
    const outPath = path.join(tmp, 'soak-health.jsonl');
    const t0 = '2026-11-26T16:00:00.000Z';

    const m1 = ensureSoakManifest(manifestPath, {
      startedAtUtc: t0,
      rpcUrl: 'https://testnet-rpc.gameladder.io',
      blockMs: 2000,
      standalone: true,
    });
    assert.equal(m1.mode, 'standalone');
    assert.equal(m1.startedAtUtc, t0);

    // Calling ensureSoakManifest again without force MUST preserve original startedAtUtc.
    const m2 = ensureSoakManifest(manifestPath, {
      startedAtUtc: '2026-11-27T00:00:00.000Z',
      standalone: true,
    });
    assert.equal(m2.startedAtUtc, t0);

    const rec = buildSoakProbeRecord(
      {
        ok: true,
        rollbackRecommended: false,
        ts: t0,
        protocol: 'bitblock-l1-v1',
        chainId: 1112101971,
        validatorsOnline: 4,
        heightBefore: 100,
        heightAfter: 103,
        qcVerified: true,
        conservationOk: true,
        walWritesMetric: 200,
        blocksPageCount: 100,
        bodyCapStatus: 413,
        relayerMode: 'active',
        warnings: [],
        issues: [],
      },
      0,
      { standalone: true },
    );
    appendSoakRecord(outPath, rec);

    const parsed = parseJsonlText(fs.readFileSync(outPath, 'utf8'));
    assert.equal(parsed.length, 1);
    assert.equal(parsed[0].relayerActive, true);
    assert.equal(parsed[0].chainId, 1112101971);

    const evalRes = evaluateSoakRecords(parsed, {
      manifestStartedAtUtc: m1.startedAtUtc,
      standalone: true,
    });
    assert.equal(evalRes.standalone, true);
    assert.equal(evalRes.passCount, 1);
    assert.equal(evalRes.invariants.qcFailures, 0);
    assert.equal(evalRes.invariants.conservationFailures, 0);

    // Non-JSON error output fails closed.
    const bad = buildSoakProbeRecord(null, 1, { parseError: 'timeout', standalone: true });
    assert.equal(bad.ok, false);
    assert.equal(bad.rollbackRecommended, true);

    return { ok: true, testsPassed: 4 };
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.includes('--self-test')) {
    console.log(JSON.stringify(runSelfTest(), null, 2));
    process.exit(0);
  }
  const get = (flag, fallback) => {
    const i = args.indexOf(flag);
    return i >= 0 && i + 1 < args.length ? args[i + 1] : fallback;
  };
  const rpcUrl = get('--rpc', process.env.BITS_RPC_URL || 'https://testnet-rpc.gameladder.io');
  const relayerUrl = get('--relayer', process.env.BITS_RELAYER_HEALTH_URL || 'http://127.0.0.1:8690/health');
  const nodePorts = get('--node-ports', process.env.BITS_NODE_PORTS || '8645,8646,8647,8648');
  const advanceMs = get('--advance-ms', process.env.ADVANCE_WAIT_MS || '4500');
  const wasmPath = get('--wasm', process.env.WASM_PATH || resolveDefaultWasmPath());
  const outPath = path.resolve(get('--out', DEFAULT_OUT));
  const manifestPath = path.resolve(get('--manifest', DEFAULT_MANIFEST));
  const blockMs = Number(get('--block-ms', '2000'));
  const standalone = !args.includes('--with-rng-fallback');

  if (args.includes('--init-manifest')) {
    const m = ensureSoakManifest(manifestPath, {
      force: true,
      rpcUrl,
      blockMs,
      standalone,
    });
    console.log(JSON.stringify(m, null, 2));
    process.exit(0);
  }

  ensureSoakManifest(manifestPath, { rpcUrl, blockMs, standalone });

  const childArgs = [
    HEALTH_SCRIPT,
    '--rpc', rpcUrl,
    '--relayer', relayerUrl,
    '--node-ports', nodePorts,
    '--advance-ms', advanceMs,
    '--wasm', wasmPath,
  ];
  if (args.includes('--strict-relayer')) childArgs.push('--strict-relayer');
  if (args.includes('--skip-relayer')) childArgs.push('--skip-relayer');

  const res = spawnSync(process.execPath, childArgs, {
    encoding: 'utf8',
    timeout: 60_000,
  });
  let parsed = null;
  let parseError = '';
  try {
    parsed = JSON.parse(String(res.stdout || '').trim());
  } catch (e) {
    parseError = `health probe stdout parse error (exit=${res.status}): ${String(res.stderr || e.message || '').trim()}`;
  }

  const exitCode = typeof res.status === 'number' ? res.status : 1;
  const record = buildSoakProbeRecord(parsed, exitCode, { standalone, parseError });
  appendSoakRecord(outPath, record);
  console.log(JSON.stringify(record, null, 2));
  process.exit(exitCode);
}
