#!/usr/bin/env node
/**
 * Read-only Phase 2 -> Phase 3 testnet soak scorecard evaluator.
 *
 * Reads the JSONL telemetry produced by the 15-minute systemd soak timer
 * (default: `node/.runtime/soak/soak-health.jsonl`) without mutating any
 * runtime state or touching any container, and computes a dual-gate verdict:
 *
 *   1. `chainSoakReady`:
 *      - Elapsed soak window >= 168h (7 full days) & >= 600 probes
 *      - Probe pass rate >= 99.5%
 *      - 0 Ed25519 HotStuff QC verification failures
 *      - 0 closed supply conservation failures
 *      - 0 HTTP 413 body-cap or <= 100 /proof/blocks cap failures
 *      - 0 validator availability shortfalls (< 3/4 online)
 *      - 0 `rollbackRecommended === true` triggers
 *      - Strictly non-decreasing finalized height across all passing probes
 *      - Hot fallback `rng-node-0..3` online (`4/4`) across >= 99.5% of probes
 *
 *   2. `fullPhase3Ready`:
 *      - `chainSoakReady === true`, AND
 *      - `relayerActiveSamples >= 1` and `relayerActiveHours >= 24`
 *        (verifying `bb-relayer` flipped from `standby_gate` to `active` after
 *        Owen runs `~/rngcoin-node/scripts/sync-relayer-gate-key.sh`).
 *
 * Strictly testnet/devnet (`lab: true`). Zero external dependencies (Node 22).
 */

import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const DEFAULT_INPUT_PATH = 'node/.runtime/soak/soak-health.jsonl';
export const DEFAULT_MANIFEST_PATH = 'node/.runtime/soak/soak-manifest.json';

export const DEFAULT_THRESHOLDS = Object.freeze({
  minElapsedHours: 168,
  minProbes: 600,
  minUptimePct: 99.5,
  minRngFallbackPct: 99.5,
  minRelayerActiveSamples: 1,
  minRelayerActiveHours: 24,
  expectedChainId: 1112101971,
  expectedProtocol: 'bitblock-l1-v1',
  minValidatorsOnline: 3,
  maxBlocksPageCount: 100,
  expectedBodyCapStatus: 413,
});

function parseOnlineCount(raw) {
  if (typeof raw === 'number' && Number.isFinite(raw)) return raw;
  if (typeof raw === 'string') {
    const m = raw.match(/^(\d+)\s*\/\s*(\d+)$/);
    if (m) return Number(m[1]);
  }
  return 0;
}

/**
 * Evaluates parsed soak probe records against the dual-gate Phase 3 criteria.
 */
export function evaluateSoakRecords(records, opts = {}) {
  const standalone = Boolean(opts.standalone);
  const t = { ...DEFAULT_THRESHOLDS, ...(opts.thresholds || {}) };
  const rows = Array.isArray(records) ? records.slice() : [];
  rows.sort((a, b) => Date.parse(a.ts || 0) - Date.parse(b.ts || 0));

  const totalProbes = rows.length;
  let passCount = 0;
  let failCount = 0;
  let qcFailures = 0;
  let conservationFailures = 0;
  let bodyCapFailures = 0;
  let blocksCapFailures = 0;
  let validatorShortfalls = 0;
  let rollbackFlags = 0;
  let protocolMismatches = 0;
  let rngFallbackHealthySamples = 0;
  let relayerActiveSamples = 0;

  let firstTsMs = null;
  let lastTsMs = null;
  let firstRelayerActiveMs = null;
  let lastRelayerActiveMs = null;

  let heightStart = null;
  let heightEnd = null;
  let prevHeight = null;
  let heightMonotonic = true;
  let heightRegressions = 0;

  let walWritesStart = null;
  let walWritesEnd = null;

  for (const row of rows) {
    const tsMs = Date.parse(row.ts || '');
    if (Number.isFinite(tsMs)) {
      if (firstTsMs === null || tsMs < firstTsMs) firstTsMs = tsMs;
      if (lastTsMs === null || tsMs > lastTsMs) lastTsMs = tsMs;
    }

    if (row.ok === true && (row.probeExit === undefined || row.probeExit === 0)) {
      passCount++;
    } else {
      failCount++;
    }

    if (row.rollbackRecommended === true) rollbackFlags++;
    if (row.qcVerified !== true) qcFailures++;
    if (row.conservationOk !== true) conservationFailures++;
    if (row.bodyCapStatus !== t.expectedBodyCapStatus) bodyCapFailures++;
    if (
      typeof row.blocksPageCount !== 'number' ||
      row.blocksPageCount <= 0 ||
      row.blocksPageCount > t.maxBlocksPageCount
    ) {
      blocksCapFailures++;
    }
    if (typeof row.validatorsOnline !== 'number' || row.validatorsOnline < t.minValidatorsOnline) {
      validatorShortfalls++;
    }
    if (row.protocol !== null && row.protocol !== undefined && row.protocol !== t.expectedProtocol) {
      protocolMismatches++;
    }
    if (row.chainId !== null && row.chainId !== undefined && Number(row.chainId) !== t.expectedChainId) {
      protocolMismatches++;
    }

    const rngOnline = parseOnlineCount(row.rngHotFallbackOnline);
    if (rngOnline >= 4) rngFallbackHealthySamples++;

    const relayerActive = row.relayerActive === true || row.relayerMode === 'active';
    if (relayerActive) {
      relayerActiveSamples++;
      if (Number.isFinite(tsMs)) {
        if (firstRelayerActiveMs === null || tsMs < firstRelayerActiveMs) firstRelayerActiveMs = tsMs;
        if (lastRelayerActiveMs === null || tsMs > lastRelayerActiveMs) lastRelayerActiveMs = tsMs;
      }
    }

    if (typeof row.heightAfter === 'number' && Number.isFinite(row.heightAfter)) {
      if (heightStart === null) heightStart = row.heightAfter;
      heightEnd = row.heightAfter;
      if (prevHeight !== null && row.heightAfter < prevHeight) {
        heightMonotonic = false;
        heightRegressions++;
      }
      prevHeight = row.heightAfter;
    }
    if (typeof row.heightBefore === 'number' && typeof row.heightAfter === 'number') {
      if (row.heightAfter < row.heightBefore) {
        heightMonotonic = false;
        heightRegressions++;
      }
    }

    if (typeof row.walWritesMetric === 'number' && Number.isFinite(row.walWritesMetric)) {
      if (walWritesStart === null) walWritesStart = row.walWritesMetric;
      walWritesEnd = row.walWritesMetric;
    }
  }

  const startedAtMs = opts.manifestStartedAtUtc ? Date.parse(opts.manifestStartedAtUtc) : firstTsMs;
  const effectiveStartMs =
    Number.isFinite(startedAtMs) && Number.isFinite(firstTsMs)
      ? Math.min(startedAtMs, firstTsMs)
      : firstTsMs;
  const elapsedHours =
    effectiveStartMs !== null && lastTsMs !== null && lastTsMs >= effectiveStartMs
      ? Number(((lastTsMs - effectiveStartMs) / 3_600_000).toFixed(2))
      : 0;

  const relayerActiveHours =
    firstRelayerActiveMs !== null &&
    lastRelayerActiveMs !== null &&
    lastRelayerActiveMs >= firstRelayerActiveMs
      ? Number(((lastRelayerActiveMs - firstRelayerActiveMs) / 3_600_000).toFixed(2))
      : 0;

  const uptimePct = totalProbes > 0 ? Number(((passCount / totalProbes) * 100).toFixed(3)) : 0;
  const rngFallbackPct =
    totalProbes > 0 ? Number(((rngFallbackHealthySamples / totalProbes) * 100).toFixed(3)) : 0;

  const chainBlockers = [];
  if (totalProbes === 0) chainBlockers.push('no soak probes recorded');
  if (elapsedHours < t.minElapsedHours) {
    chainBlockers.push(`elapsed window ${elapsedHours}h < ${t.minElapsedHours}h target`);
  }
  if (totalProbes < t.minProbes) {
    chainBlockers.push(`probe count ${totalProbes} < ${t.minProbes} minimum`);
  }
  if (uptimePct < t.minUptimePct) {
    chainBlockers.push(`uptime ${uptimePct}% < ${t.minUptimePct}% threshold`);
  }
  if (qcFailures > 0) chainBlockers.push(`${qcFailures} Ed25519 HotStuff QC verification failure(s)`);
  if (conservationFailures > 0) {
    chainBlockers.push(`${conservationFailures} closed supply conservation failure(s)`);
  }
  if (bodyCapFailures > 0) chainBlockers.push(`${bodyCapFailures} HTTP 413 body-cap failure(s)`);
  if (blocksCapFailures > 0) {
    chainBlockers.push(`${blocksCapFailures} /proof/blocks <= 100 cap failure(s)`);
  }
  if (validatorShortfalls > 0) {
    chainBlockers.push(`${validatorShortfalls} validator availability shortfall(s) (< ${t.minValidatorsOnline}/4)`);
  }
  if (rollbackFlags > 0) chainBlockers.push(`${rollbackFlags} rollbackRecommended trigger(s)`);
  if (protocolMismatches > 0) chainBlockers.push(`${protocolMismatches} protocol/chainId mismatch(es)`);
  if (!heightMonotonic) chainBlockers.push(`${heightRegressions} finalized height regression(s)`);
  if (!standalone && rngFallbackPct < t.minRngFallbackPct) {
    chainBlockers.push(`hot rng-node-0..3 fallback ${rngFallbackPct}% < ${t.minRngFallbackPct}%`);
  }

  const relayerBlockers = [];
  if (relayerActiveSamples < t.minRelayerActiveSamples) {
    const syncHint = standalone
      ? 'awaiting Owen GATE_PASSWORD sync into node/.runtime/relayer/relayer.env'
      : 'awaiting Owen ~/rngcoin-node/scripts/sync-relayer-gate-key.sh';
    relayerBlockers.push(
      `relayer active criterion open (${relayerActiveSamples} active sample(s); ${syncHint})`
    );
  } else if (relayerActiveHours < t.minRelayerActiveHours) {
    relayerBlockers.push(
      `relayer active window ${relayerActiveHours}h < ${t.minRelayerActiveHours}h minimum`
    );
  }

  const chainSoakReady = chainBlockers.length === 0;
  const fullPhase3Ready = chainSoakReady && relayerBlockers.length === 0;

  return {
    schema: 'bitblock/phase3-soak-scorecard/v1',
    testnetOnly: true,
    standalone,
    windowStartUtc: effectiveStartMs !== null ? new Date(effectiveStartMs).toISOString() : null,
    windowEndUtc: lastTsMs !== null ? new Date(lastTsMs).toISOString() : null,
    elapsedHours,
    targetHours: t.minElapsedHours,
    totalProbes,
    passCount,
    failCount,
    uptimePct,
    invariants: {
      qcFailures,
      conservationFailures,
      bodyCapFailures,
      blocksCapFailures,
      validatorShortfalls,
      rollbackFlags,
      protocolMismatches,
      heightStart,
      heightEnd,
      heightMonotonic,
      heightRegressions,
      walWritesStart,
      walWritesEnd,
      rngFallbackRequired: !standalone,
      rngFallbackHealthySamples,
      rngFallbackPct,
    },
    relayer: {
      activeSamples: relayerActiveSamples,
      activeHours: relayerActiveHours,
      latestMode: rows.length > 0 ? rows[rows.length - 1].relayerMode ?? null : null,
    },
    gates: {
      chainSoakReady,
      fullPhase3Ready,
    },
    chainBlockers,
    relayerBlockers,
    blockers: [...chainBlockers, ...relayerBlockers],
  };
}

export function parseJsonlText(text) {
  return String(text || '')
    .split(/\r?\n/)
    .map(l => l.trim())
    .filter(Boolean)
    .map((line, idx) => {
      try {
        return JSON.parse(line);
      } catch (err) {
        throw new Error(`invalid JSONL on line ${idx + 1}: ${err.message}`);
      }
    });
}

export function renderMarkdown(report) {
  const lines = [
    '# Bitblock L1 Phase 2 → Phase 3 Testnet Soak Scorecard',
    '',
    `- **Window:** \`${report.windowStartUtc ?? 'n/a'}\` → \`${report.windowEndUtc ?? 'n/a'}\` (**${report.elapsedHours}h / ${report.targetHours}h**)`,
    `- **Probes:** **${report.passCount}/${report.totalProbes} PASS** (${report.uptimePct}%, \`failCount=${report.failCount}\`)`,
    `- **Gate 1 (\`chainSoakReady\`):** **${report.gates.chainSoakReady ? 'READY' : 'NOT READY'}**`,
    `- **Gate 2 (\`fullPhase3Ready\`):** **${report.gates.fullPhase3Ready ? 'READY' : 'NOT READY'}**`,
    '',
    '| Invariant | Observed | Status |',
    '|---|---:|---|',
    `| Ed25519 HotStuff QC failures | ${report.invariants.qcFailures} | ${report.invariants.qcFailures === 0 ? 'PASS' : 'FAIL'} |`,
    `| Closed supply conservation failures | ${report.invariants.conservationFailures} | ${report.invariants.conservationFailures === 0 ? 'PASS' : 'FAIL'} |`,
    `| HTTP 413 body-cap failures | ${report.invariants.bodyCapFailures} | ${report.invariants.bodyCapFailures === 0 ? 'PASS' : 'FAIL'} |`,
    `| /proof/blocks <= 100 cap failures | ${report.invariants.blocksCapFailures} | ${report.invariants.blocksCapFailures === 0 ? 'PASS' : 'FAIL'} |`,
    `| Validator shortfalls (< 3/4) | ${report.invariants.validatorShortfalls} | ${report.invariants.validatorShortfalls === 0 ? 'PASS' : 'FAIL'} |`,
    `| Rollback triggers | ${report.invariants.rollbackFlags} | ${report.invariants.rollbackFlags === 0 ? 'PASS' : 'FAIL'} |`,
    `| Finalized height monotonicity | ${report.invariants.heightStart ?? '—'} → ${report.invariants.heightEnd ?? '—'} | ${report.invariants.heightMonotonic ? 'PASS' : 'FAIL'} |`,
    report.standalone
      ? '| Hot rng-node-0..3 fallback (4/4) | N/A (standalone host) | SKIP |'
      : `| Hot rng-node-0..3 fallback (4/4) | ${report.invariants.rngFallbackPct}% | ${report.invariants.rngFallbackPct >= 99.5 ? 'PASS' : 'FAIL'} |`,
    `| Relayer active window | ${report.relayer.activeSamples} probes (${report.relayer.activeHours}h, latest=\`${report.relayer.latestMode ?? 'unknown'}\`) | ${report.relayerBlockers.length === 0 ? 'PASS' : 'OPEN'} |`,
  ];
  if (report.blockers.length > 0) {
    lines.push('', '## Open Blockers');
    for (const b of report.blockers) lines.push(`- ${b}`);
  }
  return lines.join('\n') + '\n';
}

function makeSyntheticSeries(count, opts = {}) {
  const startMs = Date.parse('2026-11-25T04:37:21Z');
  const stepMs = (opts.stepMinutes ?? 15) * 60_000;
  const rows = [];
  for (let i = 0; i < count; i++) {
    const relayerActive = opts.relayerActiveFromIndex !== undefined && i >= opts.relayerActiveFromIndex;
    rows.push({
      ts: new Date(startMs + i * stepMs).toISOString(),
      probeExit: 0,
      ok: true,
      rollbackRecommended: false,
      protocol: 'bitblock-l1-v1',
      chainId: 1112101971,
      heightBefore: 10000 + i * 100,
      heightAfter: 10010 + i * 100,
      validatorsOnline: 4,
      qcVerified: true,
      conservationOk: true,
      walWritesMetric: 12000 + i * 120,
      bodyCapStatus: 413,
      blocksPageCount: 100,
      relayerMode: relayerActive ? 'active' : 'standby_gate',
      relayerActive,
      rngHotFallbackOnline: '4/4',
      rngHotFallbackHeight: 119000 + i * 40,
      issues: [],
    });
  }
  return rows;
}

export function runSelfTest() {
  const assert = (cond, msg) => {
    if (!cond) throw new Error(`soak_report self-test failed: ${msg}`);
  };

  // 1. Full 168h (673 probes at 15m) + 48h active relayer => BOTH gates READY
  const fullPass = evaluateSoakRecords(makeSyntheticSeries(673, { relayerActiveFromIndex: 480 }));
  assert(fullPass.elapsedHours === 168, `expected 168h, got ${fullPass.elapsedHours}`);
  assert(fullPass.gates.chainSoakReady === true, 'fullPass chainSoakReady must be true');
  assert(fullPass.gates.fullPhase3Ready === true, 'fullPass fullPhase3Ready must be true');
  assert(fullPass.blockers.length === 0, 'fullPass must have 0 blockers');

  // 2. Full 168h green chain + standby_gate only => dual-gate split (chain READY, Phase 3 OPEN)
  const standbyOnly = evaluateSoakRecords(makeSyntheticSeries(673));
  assert(standbyOnly.gates.chainSoakReady === true, 'standbyOnly chainSoakReady must be true');
  assert(standbyOnly.gates.fullPhase3Ready === false, 'standbyOnly fullPhase3Ready must be false');
  assert(standbyOnly.relayerBlockers.length === 1, 'standbyOnly must report 1 relayer blocker');

  // 3. Single conservation fault => BOTH gates FAIL
  const badConsRows = makeSyntheticSeries(673, { relayerActiveFromIndex: 480 });
  badConsRows[300].conservationOk = false;
  badConsRows[300].ok = false;
  badConsRows[300].rollbackRecommended = true;
  const badCons = evaluateSoakRecords(badConsRows);
  assert(badCons.invariants.conservationFailures === 1, 'conservationFailures must be 1');
  assert(badCons.gates.chainSoakReady === false, 'badCons chainSoakReady must be false');
  assert(badCons.gates.fullPhase3Ready === false, 'badCons fullPhase3Ready must be false');

  // 4. Height regression => heightMonotonic false & gate FAIL
  const regRows = makeSyntheticSeries(673, { relayerActiveFromIndex: 480 });
  regRows[200].heightAfter = regRows[199].heightAfter - 5;
  const reg = evaluateSoakRecords(regRows);
  assert(reg.invariants.heightMonotonic === false, 'heightMonotonic must be false');
  assert(reg.gates.chainSoakReady === false, 'reg chainSoakReady must be false');

  // 5. Short 12h window => NOT READY
  const shortWin = evaluateSoakRecords(makeSyntheticSeries(49, { relayerActiveFromIndex: 0 }));
  assert(shortWin.elapsedHours === 12, `expected 12h, got ${shortWin.elapsedHours}`);
  assert(shortWin.gates.chainSoakReady === false, 'shortWin chainSoakReady must be false');

  // 6. Hot fallback degraded below 99.5% => chainSoakReady FAIL
  const badRngRows = makeSyntheticSeries(673, { relayerActiveFromIndex: 480 });
  for (let i = 0; i < 10; i++) badRngRows[i].rngHotFallbackOnline = '2/4';
  const badRng = evaluateSoakRecords(badRngRows);
  assert(badRng.gates.chainSoakReady === false, 'badRng chainSoakReady must be false');

  // 7. Markdown rendering smoke check
  const md = renderMarkdown(standbyOnly);
  assert(md.includes('**Gate 1 (`chainSoakReady`):** **READY**'), 'markdown missing Gate 1');
  assert(md.includes('**Gate 2 (`fullPhase3Ready`):** **NOT READY**'), 'markdown missing Gate 2');

  // 8. Standalone host mode: passes 168h without legacy rng-node-0..3 fallback, still fails on QC fault
  const standaloneRows = makeSyntheticSeries(673, { relayerActiveFromIndex: 480 }).map((r) => {
    const copy = { ...r };
    delete copy.rngHotFallbackOnline;
    delete copy.rngHotFallbackHeight;
    return copy;
  });
  const standaloneNoFlag = evaluateSoakRecords(standaloneRows);
  assert(standaloneNoFlag.gates.chainSoakReady === false, 'default mode must still require rng fallback');
  const standalonePass = evaluateSoakRecords(standaloneRows, { standalone: true });
  assert(standalonePass.standalone === true, 'standalone flag must be reflected in report');
  assert(standalonePass.gates.chainSoakReady === true, 'standalonePass chainSoakReady must be true');
  assert(standalonePass.gates.fullPhase3Ready === true, 'standalonePass fullPhase3Ready must be true');
  assert(renderMarkdown(standalonePass).includes('N/A (standalone host)'), 'standalone markdown must mark fallback N/A');

  const standaloneBadQc = standaloneRows.map((r) => ({ ...r }));
  standaloneBadQc[100].qcVerified = false;
  standaloneBadQc[100].ok = false;
  const standaloneBad = evaluateSoakRecords(standaloneBadQc, { standalone: true });
  assert(standaloneBad.gates.chainSoakReady === false, 'standalone mode must still fail on QC error');

  return {
    ok: true,
    testsPassed: 9,
    scenarios: [
      'fullPass',
      'standbyOnlyDualGate',
      'conservationFault',
      'heightRegression',
      'shortWindow',
      'hotFallbackDegraded',
      'markdownRender',
      'standalonePass',
      'standaloneQcFault',
    ],
  };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.includes('--self-test')) {
    const res = runSelfTest();
    console.log(JSON.stringify(res, null, 2));
    process.exit(0);
  }
  const get = (flag, fallback) => {
    const i = args.indexOf(flag);
    return i >= 0 && i + 1 < args.length ? args[i + 1] : fallback;
  };
  const inputPath = get('--input', DEFAULT_INPUT_PATH);
  const manifestPath = get('--manifest', DEFAULT_MANIFEST_PATH);
  if (!existsSync(inputPath)) {
    console.error(`soak telemetry file not found: ${inputPath}`);
    process.exit(2);
  }
  let manifestStartedAtUtc = null;
  let manifestStandalone = false;
  if (existsSync(manifestPath)) {
    try {
      const m = JSON.parse(readFileSync(manifestPath, 'utf8'));
      manifestStartedAtUtc = m.startedAtUtc ?? null;
      manifestStandalone = m.mode === 'standalone';
    } catch {
      manifestStartedAtUtc = null;
      manifestStandalone = false;
    }
  }
  const standalone = args.includes('--standalone') || manifestStandalone;
  const records = parseJsonlText(readFileSync(inputPath, 'utf8'));
  const report = evaluateSoakRecords(records, { manifestStartedAtUtc, standalone });
  if (args.includes('--markdown')) {
    process.stdout.write(renderMarkdown(report));
  } else {
    console.log(JSON.stringify(report, null, 2));
  }
}
