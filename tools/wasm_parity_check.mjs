#!/usr/bin/env node
/**
 * Automated V8 WebAssembly byte-exact parity verifier for bitblock-wasm.
 *
 * Extracts all 35 pinned known-answer constants directly from
 * `crates/wasm/www/bitblock-lab.html` (single source of truth; no duplicated
 * `#bb.*` literals here), instantiates a `wasm32-unknown-unknown` binary under
 * V8, and asserts:
 *   1. ABI 17 and core viewport/demo/edit/live/camera byte-exact parity.
 *   2. All 6 gallery renders, 1201x801 sidebar composite, and program runner
 *      (including invalid-program rejection).
 *   3. Zero-copy linear-memory RGBA framebuffer view (`Uint8ClampedArray` over
 *      `memory.buffer`) across the 5-tick audited play tape, verified against
 *      both `PLAY_TAPE_KNOWN` and `bitblock_play_input_tape()`, plus a second
 *      reset+replay SHA-256 determinism check.
 *   4. All 3 browser storage carriers (`journal`, `history`, `display`) clean
 *      roundtrips plus torn-tail truncation and header-corruption negative
 *      controls.
 *   5. All 13 `bitblock_research_*` known-answer digests.
 *   6. Chem genesis/step invariant check (`books=ok`, `energy_drift=0`,
 *      `error=none`).
 *   7. With `--check-embedded`: byte-exact & SHA-256 identity between the
 *      `.wasm` file and the `WASM_B64` payloads embedded in `play.html` and
 *      `bitblock-lab.html`.
 *
 * Usage:
 *   node tools/wasm_parity_check.mjs [path/to/bitblock_wasm.wasm] [--check-embedded] [--lab <path>] [--play <path>]
 */

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const ROOT = path.resolve(path.dirname(__filename), "..");

const EXPECTED_ABI = 17;

const REQUIRED_CONSTANTS = [
  "DEMO_EXPECTED",
  "EDIT_EXPECTED",
  "LIVE_AFTER_ONE",
  "LIVE_AFTER_TWO",
  "CAMERA_BASE",
  "CAMERA_DOLLY_IN",
  "CAMERA_PITCH_UP",
  "GALLERY_PREVIEW",
  "GALLERY_SYMBOL",
  "GALLERY_RECOGNITION",
  "GALLERY_ARCHITECTURE",
  "GALLERY_RECURSION",
  "GALLERY_RECURSION_FRONTAL",
  "STORAGE_JOURNAL_WRITE",
  "STORAGE_JOURNAL_VERIFY",
  "STORAGE_HISTORY_WRITE",
  "STORAGE_HISTORY_VERIFY",
  "STORAGE_DISPLAY_WRITE",
  "STORAGE_DISPLAY_VERIFY",
  "PLAY_TAPE_KNOWN",
  "SIDEBAR_KNOWN",
  "PROGRAM_SYMBOL",
  "RESEARCH_STORAGE",
  "RESEARCH_SEQUENTIAL",
  "RESEARCH_EPISODE",
  "RESEARCH_LADDER",
  "RESEARCH_REPLAY",
  "RESEARCH_DISPLAY",
  "RESEARCH_PRESENTATION",
  "RESEARCH_OUTPUT_COLOR",
  "RESEARCH_SENSOR",
  "RESEARCH_TOPOLOGY",
  "RESEARCH_OPTIMIZATION",
  "RESEARCH_PRESENTATION_OUTPUT",
  "RESEARCH_PRESENTATION_OUTPUT_V2",
];

function usage() {
  console.log(
    "Usage: node tools/wasm_parity_check.mjs [wasm_path] [--check-embedded] [--lab <path>] [--play <path>]"
  );
}

function parseArgs(argv) {
  let wasmPath = null;
  let checkEmbedded = false;
  let labPath = path.join(ROOT, "crates/wasm/www/bitblock-lab.html");
  let playPath = path.join(ROOT, "crates/wasm/www/play.html");

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "-h" || arg === "--help") {
      usage();
      process.exit(0);
    } else if (arg === "--check-embedded") {
      checkEmbedded = true;
    } else if (arg === "--lab") {
      if (i + 1 >= argv.length) {
        console.error("wasm_parity_check: --lab requires a path argument");
        process.exit(2);
      }
      labPath = path.resolve(argv[++i]);
    } else if (arg === "--play") {
      if (i + 1 >= argv.length) {
        console.error("wasm_parity_check: --play requires a path argument");
        process.exit(2);
      }
      playPath = path.resolve(argv[++i]);
    } else if (arg.startsWith("-")) {
      console.error(`wasm_parity_check: unknown option: ${arg}`);
      usage();
      process.exit(2);
    } else if (wasmPath === null) {
      wasmPath = path.resolve(arg);
    } else {
      console.error(`wasm_parity_check: unexpected positional argument: ${arg}`);
      usage();
      process.exit(2);
    }
  }

  if (wasmPath === null) {
    wasmPath = path.join(ROOT, "crates/wasm/www/bitblock_wasm.wasm");
  }
  return { wasmPath, checkEmbedded, labPath, playPath };
}

function sha256Hex(bytes) {
  return crypto.createHash("sha256").update(bytes).digest("hex");
}

function parseLabConstants(labHtml, labPath) {
  const found = new Map();
  const re = /^\s*const\s+([A-Z0-9_]+)\s*=\s*'([^']*)';\s*$/gm;
  let match;
  while ((match = re.exec(labHtml)) !== null) {
    const [, name, value] = match;
    if (found.has(name) && found.get(name) !== value) {
      throw new Error(`Conflicting duplicate constant ${name} in ${labPath}`);
    }
    found.set(name, value);
  }
  const missing = REQUIRED_CONSTANTS.filter((name) => !found.has(name));
  if (missing.length > 0) {
    throw new Error(
      `Missing ${missing.length} required known-answer constant(s) in ${labPath}: ${missing.join(", ")}`
    );
  }
  return Object.fromEntries(REQUIRED_CONSTANTS.map((name) => [name, found.get(name)]));
}

function extractEmbeddedWasm(htmlText, htmlPath) {
  const m = htmlText.match(/const WASM_B64 =\r?\n([\s\S]*?);\r?\n/);
  if (!m) {
    throw new Error(`Could not locate const WASM_B64 block in ${htmlPath}`);
  }
  const chunks = [];
  const chunkRe = /"([A-Za-z0-9+/=]*)"/g;
  let cm;
  while ((cm = chunkRe.exec(m[1])) !== null) {
    chunks.push(cm[1]);
  }
  if (chunks.length === 0) {
    throw new Error(`Empty WASM_B64 block in ${htmlPath}`);
  }
  return Buffer.from(chunks.join(""), "base64");
}

function f64Bits(n) {
  const buf = new ArrayBuffer(8);
  new Float64Array(buf)[0] = n;
  return new BigUint64Array(buf)[0];
}

function assertEq(label, actual, expected) {
  if (actual !== expected) {
    throw new Error(
      `MISMATCH [${label}]\n  expected: ${JSON.stringify(expected)}\n  actual:   ${JSON.stringify(actual)}`
    );
  }
}

function assertCond(label, condition, detail = "") {
  if (!condition) {
    throw new Error(`ASSERTION FAILED [${label}]${detail ? ": " + detail : ""}`);
  }
}

function parseNonBackground(statusLine) {
  const m = statusLine.match(/\bnon_background=(\d+)\b/);
  return m ? Number(m[1]) : null;
}

async function main() {
  const { wasmPath, checkEmbedded, labPath, playPath } = parseArgs(process.argv.slice(2));
  const started = performance.now();

  if (!fs.existsSync(wasmPath)) {
    throw new Error(`WASM file not found: ${wasmPath}`);
  }
  if (!fs.existsSync(labPath)) {
    throw new Error(`Lab HTML not found: ${labPath}`);
  }

  const wasmBytes = fs.readFileSync(wasmPath);
  const wasmSha = sha256Hex(wasmBytes);
  const labHtml = fs.readFileSync(labPath, "utf8");
  const K = parseLabConstants(labHtml, labPath);

  const module = await WebAssembly.compile(wasmBytes);
  const instance = await WebAssembly.instantiate(module);
  const ex = instance.exports;
  const decoder = new TextDecoder("utf-8", { fatal: true });
  const encoder = new TextEncoder();

  let checks = 0;
  function pass(label) {
    checks += 1;
    return label;
  }

  function readUtf8(lenOrFn, ptrFnName) {
    const len = typeof lenOrFn === "function" ? lenOrFn() : lenOrFn;
    assertCond(`${ptrFnName} len > 0`, len > 0, `got ${len}`);
    const ptr = ex[ptrFnName]();
    assertCond(`${ptrFnName} ptr > 0`, ptr > 0, `got ${ptr}`);
    // Always view ex.memory.buffer fresh after any WASM call (memory.grow safety).
    const slice = new Uint8Array(ex.memory.buffer, ptr, len);
    return decoder.decode(slice);
  }

  function verifyZeroCopyRgba(label, lenFnName, ptrFnName, width, height, expectedNonBg = null) {
    const len = ex[lenFnName]();
    const expectedLen = width * height * 4;
    assertEq(`${label} rgba len`, len, expectedLen);
    const ptr = ex[ptrFnName]();
    assertCond(`${label} rgba ptr > 0`, ptr > 0, `got ${ptr}`);
    const view = new Uint8ClampedArray(ex.memory.buffer, ptr, len);
    assertEq(`${label} zero-copy view byteLength`, view.byteLength, expectedLen);
    let nonBg = 0;
    for (let i = 0; i < len; i += 4) {
      if (view[i + 3] !== 255) {
        throw new Error(`${label}: pixel ${i / 4} alpha=${view[i + 3]} (expected 255)`);
      }
      if ((view[i] | view[i + 1] | view[i + 2]) !== 0) {
        nonBg += 1;
      }
    }
    assertCond(`${label} non-black pixels > 0`, nonBg > 0, `got ${nonBg}`);
    if (expectedNonBg !== null) {
      assertEq(`${label} non_background pixel count`, nonBg, expectedNonBg);
    }
    return { view, nonBg };
  }

  // 1. ABI & viewport
  assertEq("bitblock_wasm_abi", ex.bitblock_wasm_abi(), EXPECTED_ABI);
  assertEq("bitblock_frame_width", ex.bitblock_frame_width(), 32);
  assertEq("bitblock_frame_height", ex.bitblock_frame_height(), 32);
  pass("abi-17");

  // 2. Demo & Edit + RGBA buffers
  const demoText = readUtf8(ex.bitblock_browser_demo(), "bitblock_browser_demo_ptr");
  assertEq("DEMO_EXPECTED", demoText, K.DEMO_EXPECTED);
  verifyZeroCopyRgba("demo_rgba", "bitblock_demo_frame_rgba", "bitblock_demo_frame_rgba_ptr", 32, 32);
  pass("demo");

  const editText1 = readUtf8(ex.bitblock_browser_edit(), "bitblock_browser_edit_ptr");
  const editText2 = readUtf8(ex.bitblock_browser_edit(), "bitblock_browser_edit_ptr");
  assertEq("EDIT_EXPECTED", editText1, K.EDIT_EXPECTED);
  assertEq("edit idempotent", editText2, editText1);
  assertCond("edit differs from demo", editText1 !== demoText);
  verifyZeroCopyRgba("edit_rgba", "bitblock_edit_frame_rgba", "bitblock_edit_frame_rgba_ptr", 32, 32);
  pass("edit");

  // 3. Live click/toggle/restore & Camera moves
  const resetLen = ex.bitblock_live_reset();
  const liveReady = readUtf8(resetLen, "bitblock_live_ptr");
  assertCond("live ready status", liveReady.startsWith("bitblock_browser_live ready "));
  assertEq("CAMERA_BASE", readUtf8(ex.bitblock_camera_len(), "bitblock_camera_ptr"), K.CAMERA_BASE);

  const click1Len = ex.bitblock_live_click(17, 15);
  assertEq("LIVE_AFTER_ONE", readUtf8(click1Len, "bitblock_live_ptr"), K.LIVE_AFTER_ONE);
  const click2Len = ex.bitblock_live_click(17, 15);
  assertEq("LIVE_AFTER_TWO", readUtf8(click2Len, "bitblock_live_ptr"), K.LIVE_AFTER_TWO);
  verifyZeroCopyRgba("live_rgba", "bitblock_live_frame_rgba", "bitblock_live_frame_rgba_ptr", 32, 32);
  pass("live-click-restore");

  ex.bitblock_live_reset();
  ex.bitblock_live_dolly_in();
  assertEq(
    "CAMERA_DOLLY_IN",
    readUtf8(ex.bitblock_camera_len(), "bitblock_camera_ptr"),
    K.CAMERA_DOLLY_IN
  );
  ex.bitblock_live_reset();
  ex.bitblock_live_pitch_up();
  assertEq(
    "CAMERA_PITCH_UP",
    readUtf8(ex.bitblock_camera_len(), "bitblock_camera_ptr"),
    K.CAMERA_PITCH_UP
  );
  pass("camera-dolly-pitch");

  // 4. All 6 Gallery renders + out-of-range rejection
  const galleryCases = [
    ["GALLERY_PREVIEW", () => ex.bitblock_gallery_symbol_preview(), 81, 81, K.GALLERY_PREVIEW],
    ["GALLERY_SYMBOL", () => ex.bitblock_gallery(0), 801, 801, K.GALLERY_SYMBOL],
    ["GALLERY_RECOGNITION", () => ex.bitblock_gallery(1), 801, 801, K.GALLERY_RECOGNITION],
    ["GALLERY_ARCHITECTURE", () => ex.bitblock_gallery(2), 801, 801, K.GALLERY_ARCHITECTURE],
    ["GALLERY_RECURSION", () => ex.bitblock_gallery(3), 801, 801, K.GALLERY_RECURSION],
    [
      "GALLERY_RECURSION_FRONTAL",
      () => ex.bitblock_gallery_recursion_frontal(),
      801,
      801,
      K.GALLERY_RECURSION_FRONTAL,
    ],
  ];
  for (const [label, invoke, w, h, expected] of galleryCases) {
    const len = invoke();
    const status = readUtf8(len, "bitblock_gallery_ptr");
    assertEq(label, status, expected);
    assertEq(`${label} width`, ex.bitblock_gallery_width(), w);
    assertEq(`${label} height`, ex.bitblock_gallery_height(), h);
    verifyZeroCopyRgba(
      label,
      "bitblock_gallery_rgba",
      "bitblock_gallery_rgba_ptr",
      w,
      h,
      parseNonBackground(expected)
    );
    pass(label);
  }
  assertEq("gallery invalid id rejected", ex.bitblock_gallery(99), 0);
  pass("gallery-invalid-id-negative");

  // 5. 1201x801 Sidebar composite
  const sidebarLen = ex.bitblock_sidebar_composite();
  assertEq("SIDEBAR_KNOWN", readUtf8(sidebarLen, "bitblock_sidebar_ptr"), K.SIDEBAR_KNOWN);
  assertEq("sidebar width", ex.bitblock_sidebar_width(), 1201);
  assertEq("sidebar height", ex.bitblock_sidebar_height(), 801);
  verifyZeroCopyRgba(
    "SIDEBAR_KNOWN",
    "bitblock_sidebar_rgba",
    "bitblock_sidebar_rgba_ptr",
    1201,
    801,
    parseNonBackground(K.SIDEBAR_KNOWN)
  );
  pass("sidebar-composite");

  // 6. Program symbol + invalid-program negative control
  const progSymLen = ex.bitblock_program_symbol();
  assertEq("PROGRAM_SYMBOL", readUtf8(progSymLen, "bitblock_program_ptr"), K.PROGRAM_SYMBOL);
  assertEq("program width", ex.bitblock_program_width(), 81);
  assertEq("program height", ex.bitblock_program_height(), 81);
  verifyZeroCopyRgba(
    "PROGRAM_SYMBOL",
    "bitblock_program_rgba",
    "bitblock_program_rgba_ptr",
    81,
    81,
    parseNonBackground(K.PROGRAM_SYMBOL)
  );
  const badProg = encoder.encode("invalid_opcode 1 2 3\n");
  const badProgPtr = ex.bitblock_program_buf(badProg.length);
  assertCond("program buf allocated", badProgPtr > 0);
  new Uint8Array(ex.memory.buffer, badProgPtr, badProg.length).set(badProg);
  const badProgStatus = readUtf8(ex.bitblock_program_run(badProg.length), "bitblock_program_ptr");
  assertCond(
    "invalid program rejected cleanly",
    !badProgStatus.startsWith("bitblock_browser_program ok "),
    badProgStatus
  );
  pass("program-symbol-and-negative");

  // 7. Audited 5-tick play tape + zero-copy RGBA framebuffer + replay determinism
  function runAuditedPlayTape() {
    ex.bitblock_play_reset();
    ex.bitblock_play_set_capture(1);
    ex.bitblock_play_key(0, 1); // KeyW down
    ex.bitblock_play_look(f64Bits(80), f64Bits(0));
    ex.bitblock_play_tick();

    ex.bitblock_play_key(0, 0); // KeyW up
    ex.bitblock_play_wheel(f64Bits(-8));
    ex.bitblock_play_pointer_button(2, 1);
    ex.bitblock_play_tick();

    ex.bitblock_play_pointer_button(0, 1);
    ex.bitblock_play_tick();

    ex.bitblock_play_key(7, 1); // F3
    ex.bitblock_play_key(7, 0);
    ex.bitblock_play_tick();

    ex.bitblock_play_key(6, 1); // KeyF
    ex.bitblock_play_key(6, 0);
    const statusLen = ex.bitblock_play_tick();
    const status = readUtf8(statusLen, "bitblock_play_ptr");
    const w = ex.bitblock_play_width();
    const h = ex.bitblock_play_height();
    assertEq("play tape width", w, 1201);
    assertEq("play tape height", h, 801);
    const { view } = verifyZeroCopyRgba("play_tape_rgba", "bitblock_play_rgba", "bitblock_play_rgba_ptr", w, h);
    const rgbaSha = sha256Hex(view);
    ex.bitblock_play_clear_dirty();
    return { status, rgbaSha };
  }

  const playRun1 = runAuditedPlayTape();
  assertEq("PLAY_TAPE_KNOWN (handler sequence)", playRun1.status, K.PLAY_TAPE_KNOWN);
  const hostTape = readUtf8(ex.bitblock_play_input_tape(), "bitblock_play_input_tape_ptr");
  assertEq("PLAY_TAPE_KNOWN (host tape)", hostTape, K.PLAY_TAPE_KNOWN);
  assertEq("handler tape === host tape", playRun1.status, hostTape);

  const playRun2 = runAuditedPlayTape();
  assertEq("play tape replay status deterministic", playRun2.status, playRun1.status);
  assertEq("play tape zero-copy RGBA deterministic", playRun2.rgbaSha, playRun1.rgbaSha);
  pass("play-tape-zero-copy-determinism");

  // 8. Browser storage write/verify roundtrips + torn-tail & corruption negative controls
  const storageCases = [
    [
      "journal",
      "bitblock_storage_journal_write",
      "bitblock_storage_journal_verify",
      K.STORAGE_JOURNAL_WRITE,
      K.STORAGE_JOURNAL_VERIFY,
    ],
    [
      "history",
      "bitblock_storage_history_write",
      "bitblock_storage_history_verify",
      K.STORAGE_HISTORY_WRITE,
      K.STORAGE_HISTORY_VERIFY,
    ],
    [
      "display",
      "bitblock_storage_display_write",
      "bitblock_storage_display_verify",
      K.STORAGE_DISPLAY_WRITE,
      K.STORAGE_DISPLAY_VERIFY,
    ],
  ];

  function verifyStorageBytes(verifyFn, bytes) {
    const ptr = ex.bitblock_storage_buf(bytes.length);
    assertCond(`${verifyFn} buf ptr > 0`, ptr > 0, `len=${bytes.length}`);
    if (bytes.length > 0) {
      new Uint8Array(ex.memory.buffer, ptr, bytes.length).set(bytes);
    }
    const len = ex[verifyFn](bytes.length);
    return readUtf8(len, "bitblock_storage_ptr");
  }

  for (const [store, writeFn, verifyFn, expectedWrite, expectedVerify] of storageCases) {
    const writeStatus = readUtf8(ex[writeFn](), "bitblock_storage_ptr");
    assertEq(`STORAGE_${store.toUpperCase()}_WRITE`, writeStatus, expectedWrite);

    const byteLen = ex.bitblock_storage_bytes();
    const bytePtr = ex.bitblock_storage_bytes_ptr();
    assertCond(`${store} storage bytes > 16`, byteLen > 16 && bytePtr > 0);
    const carrier = new Uint8Array(ex.memory.buffer.slice(bytePtr, bytePtr + byteLen));

    const verifyStatus = verifyStorageBytes(verifyFn, carrier);
    assertEq(`STORAGE_${store.toUpperCase()}_VERIFY`, verifyStatus, expectedVerify);

    // Torn-tail truncation negative control: must NOT report scan=clean.
    const torn = carrier.subarray(0, carrier.length - 5);
    const tornStatus = verifyStorageBytes(verifyFn, torn);
    assertCond(
      `${store} torn-tail rejected`,
      tornStatus !== expectedVerify &&
        !tornStatus.includes("scan=clean") &&
        (tornStatus.includes("scan=recovery") || tornStatus.includes(" err ")) &&
        tornStatus.includes("rejected="),
      tornStatus
    );

    // Header corruption negative control (flip first magic byte): must NOT report scan=clean.
    const corrupted = new Uint8Array(carrier);
    corrupted[0] ^= 0xff;
    const corruptStatus = verifyStorageBytes(verifyFn, corrupted);
    assertCond(
      `${store} corrupted header rejected`,
      corruptStatus !== expectedVerify && !corruptStatus.includes("scan=clean"),
      corruptStatus
    );

    pass(`storage-${store}-and-negatives`);
  }

  // 9. All 13 bitblock_research_* exports
  const researchCases = [
    ["RESEARCH_STORAGE", "bitblock_research_storage", K.RESEARCH_STORAGE],
    ["RESEARCH_SEQUENTIAL", "bitblock_research_sequential", K.RESEARCH_SEQUENTIAL],
    ["RESEARCH_EPISODE", "bitblock_research_episode", K.RESEARCH_EPISODE],
    ["RESEARCH_LADDER", "bitblock_research_ladder", K.RESEARCH_LADDER],
    ["RESEARCH_REPLAY", "bitblock_research_replay", K.RESEARCH_REPLAY],
    ["RESEARCH_DISPLAY", "bitblock_research_display_artifact", K.RESEARCH_DISPLAY],
    ["RESEARCH_PRESENTATION", "bitblock_research_presentation", K.RESEARCH_PRESENTATION],
    ["RESEARCH_OUTPUT_COLOR", "bitblock_research_output_color", K.RESEARCH_OUTPUT_COLOR],
    ["RESEARCH_SENSOR", "bitblock_research_physical_sensor", K.RESEARCH_SENSOR],
    ["RESEARCH_TOPOLOGY", "bitblock_research_output_topology", K.RESEARCH_TOPOLOGY],
    ["RESEARCH_OPTIMIZATION", "bitblock_research_optimization", K.RESEARCH_OPTIMIZATION],
    [
      "RESEARCH_PRESENTATION_OUTPUT",
      "bitblock_research_presentation_output",
      K.RESEARCH_PRESENTATION_OUTPUT,
    ],
    [
      "RESEARCH_PRESENTATION_OUTPUT_V2",
      "bitblock_research_presentation_output_v2",
      K.RESEARCH_PRESENTATION_OUTPUT_V2,
    ],
  ];
  for (const [label, fnName, expected] of researchCases) {
    const status = readUtf8(ex[fnName](), "bitblock_research_ptr");
    assertEq(label, status, expected);
    pass(label);
  }

  // 10. Chem invariants smoke check (books=ok, energy_drift=0, error=none)
  const chemLoadLen = ex.bitblock_play_load_chem(0, 1, 8);
  assertCond("chem load non-empty", chemLoadLen > 0);
  assertEq("chem load error len === 0", ex.bitblock_play_chem_error(), 0);
  ex.bitblock_play_chem_step(2);
  const chemStatus = readUtf8(ex.bitblock_play_chem_status(), "bitblock_play_chem_ptr");
  assertCond("chem books=ok", chemStatus.includes(" books=ok "), chemStatus);
  assertCond("chem energy_drift=0", chemStatus.includes(" energy_drift=0 "), chemStatus);
  assertCond("chem error=none", chemStatus.includes(" error=none "), chemStatus);
  pass("chem-invariants");

  // 11. Optional embedded HTML artifact byte/SHA-256 identity
  let embeddedNote = "";
  if (checkEmbedded) {
    for (const htmlFile of [playPath, labPath]) {
      if (!fs.existsSync(htmlFile)) {
        throw new Error(`Embedded HTML file not found: ${htmlFile}`);
      }
      const htmlText = fs.readFileSync(htmlFile, "utf8");
      const decoded = extractEmbeddedWasm(htmlText, htmlFile);
      const decodedSha = sha256Hex(decoded);
      assertEq(`embedded byte length (${path.basename(htmlFile)})`, decoded.length, wasmBytes.length);
      assertEq(`embedded sha256 (${path.basename(htmlFile)})`, decodedSha, wasmSha);
      assertCond(
        `embedded bytes exact (${path.basename(htmlFile)})`,
        Buffer.compare(decoded, wasmBytes) === 0
      );
      pass(`embedded-${path.basename(htmlFile)}`);
    }
    embeddedNote = ", embedded=play.html+bitblock-lab.html exact";
  }

  const elapsedMs = (performance.now() - started).toFixed(0);
  console.log(
    `wasm_parity_check PASS: abi=${EXPECTED_ABI}, 35/35 known-answer constants, ` +
      `${checks} check groups, wasm=${path.relative(ROOT, wasmPath)} (${wasmBytes.length} bytes, ` +
      `sha256=${wasmSha.slice(0, 16)}…${embeddedNote}) in ${elapsedMs}ms`
  );
}

main().catch((err) => {
  console.error(`wasm_parity_check FAIL: ${err && err.message ? err.message : err}`);
  process.exit(1);
});
