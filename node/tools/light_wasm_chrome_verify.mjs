#!/usr/bin/env node
/**
 * Non-blocking headless Chrome visual verification harness for bitblock-light-wasm (PR #106).
 *
 * Serves `light_verifier_demo.html` + the compiled release `.wasm` over localhost,
 * launches headless Google Chrome via non-blocking `spawn` with an explicit hard
 * timeout (so the HTTP server event loop is never blocked and Chrome can never hang),
 * asserts all 12 browser verification checks pass, and saves a full PNG screenshot.
 */
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, '../..');
const WASM_PATH = path.join(
  REPO_ROOT,
  'node/target/wasm32-unknown-unknown/release/bitblock_light_wasm.wasm',
);
const DEMO_HTML = path.join(__dirname, 'light_verifier_demo.html');

const outPng = process.argv[2] || path.join(REPO_ROOT, 'node/target/pr106_light_wasm_chrome.png');
const HARD_TIMEOUT_MS = Number(process.env.CHROME_TIMEOUT_MS || 20_000);
const CHROME_BIN = process.env.CHROME_BIN || 'google-chrome';

if (!fs.existsSync(WASM_PATH)) {
  console.error(`Missing compiled WASM at ${WASM_PATH}. Run cargo build --release --target wasm32-unknown-unknown first.`);
  process.exit(1);
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

    child.stdout.on('data', (d) => {
      stdout += d.toString();
    });
    child.stderr.on('data', (d) => {
      stderr += d.toString();
    });
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

const server = http.createServer((req, res) => {
  const urlPath = (req.url || '/').split('?')[0];
  if (urlPath === '/' || urlPath === '/index.html' || urlPath === '/light_verifier_demo.html') {
    res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    res.end(fs.readFileSync(DEMO_HTML));
    return;
  }
  if (urlPath === '/bitblock_light_wasm.wasm') {
    res.writeHead(200, { 'Content-Type': 'application/wasm' });
    res.end(fs.readFileSync(WASM_PATH));
    return;
  }
  res.writeHead(404);
  res.end('not found');
});

await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const { port } = server.address();
const baseUrl = `http://127.0.0.1:${port}/`;

try {
  const commonFlags = [
    '--headless=new',
    '--no-sandbox',
    '--disable-gpu',
    '--disable-dev-shm-usage',
    '--window-size=1280,760',
    '--virtual-time-budget=4000',
  ];

  // 1. Capture DOM and confirm all 12 browser checks passed green.
  const domRes = await runChromeWithTimeout([...commonFlags, '--dump-dom', baseUrl], HARD_TIMEOUT_MS);
  if (!domRes.stdout.includes('ALL 12 CHECKS VERIFIED GREEN')) {
    throw new Error('Demo page did not report ALL 12 CHECKS VERIFIED GREEN');
  }

  // 2. Capture screenshot.
  fs.mkdirSync(path.dirname(path.resolve(outPng)), { recursive: true });
  await runChromeWithTimeout(
    [...commonFlags, `--screenshot=${path.resolve(outPng)}`, baseUrl],
    HARD_TIMEOUT_MS,
  );
  const stat = fs.statSync(outPng);
  if (stat.size < 5_000) {
    throw new Error(`Screenshot unexpectedly small: ${stat.size} bytes`);
  }
  console.log(`CHROME_VISUAL_VERIFICATION=PASS png=${outPng} bytes=${stat.size}`);
} finally {
  server.close();
}
