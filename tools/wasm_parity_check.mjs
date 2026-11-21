#!/usr/bin/env node
/**
 * Automated V8 WebAssembly byte-exact parity verifier for bitblock-wasm.
 *
 * Extracts all 39 pinned known-answer constants directly from
 * `crates/wasm/www/bitblock-lab.html` (single source of truth; no duplicated
 * `#bb.*` literals here), instantiates a `wasm32-unknown-unknown` binary under
 * V8, and asserts:
 *   1. ABI 17 and core viewport/demo/edit/live/camera byte-exact parity.
 *   2. Independent canonical `bitblock/frame/xrgb8888-row-major/v1` ContentTag
 *      reconstruction directly from every zero-copy linear-memory RGBA
 *      `Uint8ClampedArray` (`demo`, `edit`, `live` clicks & camera moves,
 *      all 6 gallery renders, 1201x801 sidebar composite, program preview, and
 *      the 5-tick audited play tape), catching any RGB channel swap, stride
 *      bug, or stale linear-memory framebuffer pointer.
 *   3. All 6 gallery renders (+ invalid-id rejection), 1201x801 sidebar
 *      composite, and program runner (+ invalid-program rejection).
 *   4. Zero-copy linear-memory RGBA framebuffer + dirty-flag lifecycle across
 *      the 5-tick audited play tape, verified against `PLAY_TAPE_KNOWN` and
 *      `bitblock_play_input_tape()`, plus a second reset+replay determinism
 *      check.
 *   5. All 3 browser storage carriers (`journal`, `history`, `display`) clean
 *      roundtrips plus numeric torn-tail truncation, header-corruption, and
 *      empty-carrier negative controls.
 *   6. All 17 `bitblock_research_*` known-answer digests (covering 20/20
 *      portable `bitblock-research` modules together with the 3 storage stores).
 *   7. Chem simulation step execution (`bitblock_play_chem_step` drained via
 *      `bitblock_play_tick`), asserting `tick >= 2`, `queued=0`, `space_q > 0`,
 *      `books=ok`, `energy_drift=0`, all 12 element lane drifts == 0,
 *      `error=none`, zero-copy chem RGBA rendering, and invalid-preset
 *      rejection.
 *   8. With `--check-embedded`: strict `WASM_B64` syntax & byte/SHA-256
 *      identity across `play.html` and `bitblock-lab.html`, plus launcher
 *      non-embed check on `bitblock.html`.
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
const BASE32_ALPHABET = "abcdefghijklmnopqrstuvwxyz234567";
const TAG_DOMAIN_BYTES = Buffer.from("bitblock/content-tag/v1", "utf8");
const FRAME_KIND_BYTES = Buffer.from("frame", "utf8");
const HASH_SUITE_BYTES = Buffer.from("sha256", "utf8");
const CANONICAL_FRAME_DOMAIN_BYTES = Buffer.from("bitblock/frame/xrgb8888-row-major/v1", "utf8");
const ZERO_LANE_DRIFT = "H:0,O:0,C:0,N:0,S:0,Ca:0,Fe:0,Ni:0,Si:0,Mg:0,Na:0,K:0";

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
  "RESEARCH_COMMIT_R1",
  "RESEARCH_OCTREE_COMMIT",
  "RESEARCH_SCALE_ASSAY",
  "RESEARCH_LOCAL_BOUNDS",
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
  let customPlay = false;

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
      customPlay = true;
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
  return { wasmPath, checkEmbedded, labPath, playPath, customPlay };
}

function sha256Hex(bytes) {
  return crypto.createHash("sha256").update(bytes).digest("hex");
}

function base32Digest(digestBytes) {
  let out = "";
  let acc = 0;
  let bits = 0;
  for (const byte of digestBytes) {
    acc = (acc << 8) | byte;
    bits += 8;
    while (bits >= 5) {
      bits -= 5;
      out += BASE32_ALPHABET[(acc >>> bits) & 31];
    }
  }
  if (bits > 0) {
    out += BASE32_ALPHABET[(acc << (5 - bits)) & 31];
  }
  return out;
}

function u16Be(n) {
  const b = Buffer.allocUnsafe(2);
  b.writeUInt16BE(n, 0);
  return b;
}

function u32Be(n) {
  const b = Buffer.allocUnsafe(4);
  b.writeUInt32BE(n, 0);
  return b;
}

function u64Be(n) {
  const b = Buffer.allocUnsafe(8);
  b.writeBigUInt64BE(BigInt(n), 0);
  return b;
}

/**
 * Independently reconstructs `#bb.frame.sha256.<base32>` from a zero-copy
 * RGBA `Uint8ClampedArray` view over WASM linear memory, matching
 * `bitblock_core::frame::FrameRef::content_tag()` (`bitblock/frame/xrgb8888-row-major/v1`).
 */
function rgbaToFrameTag(width, height, rgbaView) {
  const pixelCount = BigInt(width) * BigInt(height);
  const payloadLen =
    BigInt(2 + CANONICAL_FRAME_DOMAIN_BYTES.length + 4 + 4 + 8) + pixelCount * 4n;

  const hasher = crypto.createHash("sha256");
  hasher.update(u16Be(TAG_DOMAIN_BYTES.length));
  hasher.update(TAG_DOMAIN_BYTES);
  hasher.update(u16Be(FRAME_KIND_BYTES.length));
  hasher.update(FRAME_KIND_BYTES);
  hasher.update(u16Be(HASH_SUITE_BYTES.length));
  hasher.update(HASH_SUITE_BYTES);
  hasher.update(u16Be(CANONICAL_FRAME_DOMAIN_BYTES.length));
  hasher.update(CANONICAL_FRAME_DOMAIN_BYTES);
  hasher.update(u64Be(payloadLen));

  hasher.update(u16Be(CANONICAL_FRAME_DOMAIN_BYTES.length));
  hasher.update(CANONICAL_FRAME_DOMAIN_BYTES);
  hasher.update(u32Be(width));
  hasher.update(u32Be(height));
  hasher.update(u64Be(pixelCount));

  // Batch convert RGBA [R, G, B, 255] -> big-endian XRGB8888 [0x00, R, G, B].
  const batch = Buffer.allocUnsafe(4096);
  let bi = 0;
  for (let i = 0; i < rgbaView.length; i += 4) {
    batch[bi] = 0;
    batch[bi + 1] = rgbaView[i];
    batch[bi + 2] = rgbaView[i + 1];
    batch[bi + 3] = rgbaView[i + 2];
    bi += 4;
    if (bi === batch.length) {
      hasher.update(batch);
      bi = 0;
    }
  }
  if (bi > 0) {
    hasher.update(batch.subarray(0, bi));
  }
  return `#bb.frame.sha256.${base32Digest(hasher.digest())}`;
}

function parseLabConstants(labHtml, labPath) {
  const found = new Map();
  const re = /^\s*const\s+([A-Z0-9_]+)\s*=\s*(?:'([^']*)'|"([^"]*)");\s*$/gm;
  let match;
  while ((match = re.exec(labHtml)) !== null) {
    const name = match[1];
    const value = match[2] !== undefined ? match[2] : match[3];
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
  const body = m[1].trim();
  if (body.length === 0) {
    throw new Error(`Empty WASM_B64 block in ${htmlPath}`);
  }
  const chunks = [];
  const tokenRe = /"([A-Za-z0-9+/=]*)"/y;
  const sepRe = /\s*\+\s*/y;
  let pos = 0;
  while (pos < body.length) {
    tokenRe.lastIndex = pos;
    const chunkMatch = tokenRe.exec(body);
    if (!chunkMatch) {
      throw new Error(
        `Malformed WASM_B64 chunk #${chunks.length} in ${htmlPath} at offset ${pos}: ${body.slice(pos, pos + 64)}`
      );
    }
    chunks.push(chunkMatch[1]);
    pos = tokenRe.lastIndex;
    if (pos < body.length) {
      sepRe.lastIndex = pos;
      const sepMatch = sepRe.exec(body);
      if (!sepMatch) {
        throw new Error(
          `Expected '+' separator after WASM_B64 chunk #${chunks.length - 1} in ${htmlPath} at offset ${pos}`
        );
      }
      pos = sepRe.lastIndex;
      if (pos >= body.length) {
        throw new Error(`Trailing '+' in WASM_B64 block of ${htmlPath}`);
      }
    }
  }
  const b64 = chunks.join("");
  if (b64.length === 0 || b64.length % 4 !== 0 || !/^[A-Za-z0-9+/]*={0,2}$/.test(b64)) {
    throw new Error(`Invalid base64 payload in WASM_B64 of ${htmlPath}`);
  }
  return Buffer.from(b64, "base64");
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

function parseIntField(statusLine, key) {
  const m = statusLine.match(new RegExp(`(?:^|\\s)${key}=(-?\\d+)(?:\\s|$)`));
  return m ? Number(m[1]) : null;
}

function parseFrameTag(statusLine) {
  const m = statusLine.match(/(?:^|\s)frame=(#bb\.frame\.sha256\.[a-z2-7]{52})(?:\s|$)/);
  return m ? m[1] : null;
}

async function main() {
  const { wasmPath, checkEmbedded, labPath, playPath, customPlay } = parseArgs(process.argv.slice(2));
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

  function verifyZeroCopyRgba(
    label,
    lenFnName,
    ptrFnName,
    width,
    height,
    { expectedNonBg = null, expectedFrameTag = null } = {}
  ) {
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
    if (expectedFrameTag !== null) {
      const actualFrameTag = rgbaToFrameTag(width, height, view);
      assertEq(`${label} zero-copy RGBA canonical frame tag`, actualFrameTag, expectedFrameTag);
    }
    return { view, nonBg };
  }

  // 1. ABI & viewport
  assertEq("bitblock_wasm_abi", ex.bitblock_wasm_abi(), EXPECTED_ABI);
  assertEq("bitblock_frame_width", ex.bitblock_frame_width(), 32);
  assertEq("bitblock_frame_height", ex.bitblock_frame_height(), 32);
  pass("abi-17");

  // 2. Demo & Edit + zero-copy RGBA canonical frame-tag parity
  const demoText = readUtf8(ex.bitblock_browser_demo(), "bitblock_browser_demo_ptr");
  assertEq("DEMO_EXPECTED", demoText, K.DEMO_EXPECTED);
  verifyZeroCopyRgba("demo_rgba", "bitblock_demo_frame_rgba", "bitblock_demo_frame_rgba_ptr", 32, 32, {
    expectedFrameTag: parseFrameTag(K.DEMO_EXPECTED),
  });
  pass("demo");

  const editText1 = readUtf8(ex.bitblock_browser_edit(), "bitblock_browser_edit_ptr");
  const editText2 = readUtf8(ex.bitblock_browser_edit(), "bitblock_browser_edit_ptr");
  assertEq("EDIT_EXPECTED", editText1, K.EDIT_EXPECTED);
  assertEq("edit idempotent", editText2, editText1);
  assertCond("edit differs from demo", editText1 !== demoText);
  verifyZeroCopyRgba("edit_rgba", "bitblock_edit_frame_rgba", "bitblock_edit_frame_rgba_ptr", 32, 32, {
    expectedFrameTag: parseFrameTag(K.EDIT_EXPECTED),
  });
  pass("edit");

  // 3. Live click/toggle/restore & Camera moves (+ zero-copy RGBA canonical tags for every state)
  const resetLen = ex.bitblock_live_reset();
  const liveReady = readUtf8(resetLen, "bitblock_live_ptr");
  assertCond("live ready status", liveReady.startsWith("bitblock_browser_live ready "));
  assertEq("CAMERA_BASE", readUtf8(ex.bitblock_camera_len(), "bitblock_camera_ptr"), K.CAMERA_BASE);
  verifyZeroCopyRgba("live_base_rgba", "bitblock_live_frame_rgba", "bitblock_live_frame_rgba_ptr", 32, 32, {
    expectedFrameTag: parseFrameTag(K.CAMERA_BASE),
  });

  const click1Len = ex.bitblock_live_click(17, 15);
  assertEq("LIVE_AFTER_ONE", readUtf8(click1Len, "bitblock_live_ptr"), K.LIVE_AFTER_ONE);
  verifyZeroCopyRgba("live_click1_rgba", "bitblock_live_frame_rgba", "bitblock_live_frame_rgba_ptr", 32, 32, {
    expectedFrameTag: parseFrameTag(K.LIVE_AFTER_ONE),
  });

  const click2Len = ex.bitblock_live_click(17, 15);
  assertEq("LIVE_AFTER_TWO", readUtf8(click2Len, "bitblock_live_ptr"), K.LIVE_AFTER_TWO);
  verifyZeroCopyRgba("live_click2_rgba", "bitblock_live_frame_rgba", "bitblock_live_frame_rgba_ptr", 32, 32, {
    expectedFrameTag: parseFrameTag(K.LIVE_AFTER_TWO),
  });
  pass("live-click-restore");

  ex.bitblock_live_reset();
  ex.bitblock_live_dolly_in();
  assertEq(
    "CAMERA_DOLLY_IN",
    readUtf8(ex.bitblock_camera_len(), "bitblock_camera_ptr"),
    K.CAMERA_DOLLY_IN
  );
  verifyZeroCopyRgba("camera_dolly_rgba", "bitblock_live_frame_rgba", "bitblock_live_frame_rgba_ptr", 32, 32, {
    expectedFrameTag: parseFrameTag(K.CAMERA_DOLLY_IN),
  });

  ex.bitblock_live_reset();
  ex.bitblock_live_pitch_up();
  assertEq(
    "CAMERA_PITCH_UP",
    readUtf8(ex.bitblock_camera_len(), "bitblock_camera_ptr"),
    K.CAMERA_PITCH_UP
  );
  verifyZeroCopyRgba("camera_pitch_rgba", "bitblock_live_frame_rgba", "bitblock_live_frame_rgba_ptr", 32, 32, {
    expectedFrameTag: parseFrameTag(K.CAMERA_PITCH_UP),
  });
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
    verifyZeroCopyRgba(label, "bitblock_gallery_rgba", "bitblock_gallery_rgba_ptr", w, h, {
      expectedNonBg: parseIntField(expected, "non_background"),
      expectedFrameTag: parseFrameTag(expected),
    });
    pass(label);
  }
  assertEq("gallery invalid id rejected", ex.bitblock_gallery(99), 0);
  pass("gallery-invalid-id-negative");

  // 5. 1201x801 Sidebar composite
  const sidebarLen = ex.bitblock_sidebar_composite();
  assertEq("SIDEBAR_KNOWN", readUtf8(sidebarLen, "bitblock_sidebar_ptr"), K.SIDEBAR_KNOWN);
  assertEq("sidebar width", ex.bitblock_sidebar_width(), 1201);
  assertEq("sidebar height", ex.bitblock_sidebar_height(), 801);
  verifyZeroCopyRgba("SIDEBAR_KNOWN", "bitblock_sidebar_rgba", "bitblock_sidebar_rgba_ptr", 1201, 801, {
    expectedNonBg: parseIntField(K.SIDEBAR_KNOWN, "non_background"),
    expectedFrameTag: parseFrameTag(K.SIDEBAR_KNOWN),
  });
  pass("sidebar-composite");

  // 6. Program symbol + invalid-program negative control
  const progSymLen = ex.bitblock_program_symbol();
  assertEq("PROGRAM_SYMBOL", readUtf8(progSymLen, "bitblock_program_ptr"), K.PROGRAM_SYMBOL);
  assertEq("program width", ex.bitblock_program_width(), 81);
  assertEq("program height", ex.bitblock_program_height(), 81);
  verifyZeroCopyRgba("PROGRAM_SYMBOL", "bitblock_program_rgba", "bitblock_program_rgba_ptr", 81, 81, {
    expectedNonBg: parseIntField(K.PROGRAM_SYMBOL, "non_background"),
    expectedFrameTag: parseFrameTag(K.PROGRAM_SYMBOL),
  });
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

  // 7. Audited 5-tick play tape + zero-copy RGBA canonical frame-tag parity + dirty lifecycle + replay determinism
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
    assertEq("play dirty before clear", ex.bitblock_play_dirty(), 1);
    const { view } = verifyZeroCopyRgba("play_tape_rgba", "bitblock_play_rgba", "bitblock_play_rgba_ptr", w, h, {
      expectedFrameTag: parseFrameTag(K.PLAY_TAPE_KNOWN),
    });
    const rgbaSha = sha256Hex(view);
    ex.bitblock_play_clear_dirty();
    assertEq("play dirty after clear", ex.bitblock_play_dirty(), 0);
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

  // 8. Browser storage write/verify roundtrips + numeric torn-tail, corruption & empty negative controls
  const storageCases = [
    [
      "journal",
      "bitblock_storage_journal_write",
      "bitblock_storage_journal_verify",
      K.STORAGE_JOURNAL_WRITE,
      K.STORAGE_JOURNAL_VERIFY,
      false,
    ],
    [
      "history",
      "bitblock_storage_history_write",
      "bitblock_storage_history_verify",
      K.STORAGE_HISTORY_WRITE,
      K.STORAGE_HISTORY_VERIFY,
      true,
    ],
    [
      "display",
      "bitblock_storage_display_write",
      "bitblock_storage_display_verify",
      K.STORAGE_DISPLAY_WRITE,
      K.STORAGE_DISPLAY_VERIFY,
      false,
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

  for (const [store, writeFn, verifyFn, expectedWrite, expectedVerify, emptyMustError] of storageCases) {
    const writeStatus = readUtf8(ex[writeFn](), "bitblock_storage_ptr");
    assertEq(`STORAGE_${store.toUpperCase()}_WRITE`, writeStatus, expectedWrite);

    const byteLen = ex.bitblock_storage_bytes();
    const bytePtr = ex.bitblock_storage_bytes_ptr();
    assertCond(`${store} storage bytes > 16`, byteLen > 16 && bytePtr > 0);
    const carrier = new Uint8Array(ex.memory.buffer.slice(bytePtr, bytePtr + byteLen));

    const verifyStatus = verifyStorageBytes(verifyFn, carrier);
    assertEq(`STORAGE_${store.toUpperCase()}_VERIFY`, verifyStatus, expectedVerify);
    const cleanRecords = parseIntField(expectedVerify, "records");
    assertCond(`${store} clean records > 0`, cleanRecords !== null && cleanRecords > 0, expectedVerify);

    // Torn-tail truncation negative control: must enter recovery with rejected > 0 and fewer admitted records.
    const torn = carrier.subarray(0, carrier.length - 5);
    const tornStatus = verifyStorageBytes(verifyFn, torn);
    const tornRecords = parseIntField(tornStatus, "records");
    const tornValidPrefix = parseIntField(tornStatus, "valid_prefix");
    const tornRejected = parseIntField(tornStatus, "rejected");
    assertCond(
      `${store} torn-tail recovery`,
      tornStatus !== expectedVerify &&
        !tornStatus.includes("scan=clean") &&
        tornStatus.includes("scan=recovery") &&
        tornRecords !== null &&
        tornRecords < cleanRecords &&
        tornRejected !== null &&
        tornRejected >= 5 &&
        tornValidPrefix !== null &&
        tornValidPrefix < torn.length &&
        tornValidPrefix + tornRejected === torn.length,
      tornStatus
    );

    // Header corruption negative control (flip first magic byte): must reject the entire carrier.
    const corrupted = new Uint8Array(carrier);
    corrupted[0] ^= 0xff;
    const corruptStatus = verifyStorageBytes(verifyFn, corrupted);
    assertCond(
      `${store} corrupted header rejected`,
      corruptStatus !== expectedVerify &&
        !corruptStatus.includes("scan=clean") &&
        corruptStatus.includes("scan=recovery") &&
        parseIntField(corruptStatus, "records") === 0 &&
        parseIntField(corruptStatus, "valid_prefix") === 0 &&
        parseIntField(corruptStatus, "rejected") === carrier.length,
      corruptStatus
    );

    // Empty-carrier boundary check: history requires genesis (`err`), journal/display admit 0 records.
    const emptyStatus = verifyStorageBytes(verifyFn, new Uint8Array(0));
    if (emptyMustError) {
      assertCond(`${store} empty carrier errors`, emptyStatus.includes(" err "), emptyStatus);
    } else {
      assertCond(
        `${store} empty carrier reports 0 records/bytes`,
        parseIntField(emptyStatus, "records") === 0 && parseIntField(emptyStatus, "bytes") === 0,
        emptyStatus
      );
    }

    pass(`storage-${store}-and-negatives`);
  }

  // 9. All 17 bitblock_research_* exports (20/20 portable bitblock-research modules with storage)
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
    ["RESEARCH_COMMIT_R1", "bitblock_research_commit_r1", K.RESEARCH_COMMIT_R1],
    ["RESEARCH_OCTREE_COMMIT", "bitblock_research_octree_commit", K.RESEARCH_OCTREE_COMMIT],
    ["RESEARCH_SCALE_ASSAY", "bitblock_research_scale_assay", K.RESEARCH_SCALE_ASSAY],
    ["RESEARCH_LOCAL_BOUNDS", "bitblock_research_local_bounds", K.RESEARCH_LOCAL_BOUNDS],
  ];
  for (const [label, fnName, expected] of researchCases) {
    const status = readUtf8(ex[fnName](), "bitblock_research_ptr");
    assertEq(label, status, expected);
    pass(label);
  }

  // 10. Chem simulation execution, conservation invariants, §6ah worker parity & negative control
  assertEq("chem invalid preset rejected", ex.bitblock_play_load_chem(99, 1, 8), 0);
  assertCond("chem invalid preset sets error", ex.bitblock_play_chem_error() > 0);

  const chemLoadLen = ex.bitblock_play_load_chem(0, 1, 8);
  assertCond("chem load non-empty", chemLoadLen > 0);
  assertEq("chem load error len === 0", ex.bitblock_play_chem_error(), 0);
  assertEq("chem auto off accepted", ex.bitblock_play_chem_auto(0), 1);
  assertEq("chem queue accepted", ex.bitblock_play_chem_step(2), 1);
  // §6ah lagged chem schedule: queued steps are sent on tick 1 and published on tick 2.
  ex.bitblock_play_tick();
  const chemPlayStatus = readUtf8(ex.bitblock_play_tick(), "bitblock_play_ptr");
  const chemStatus = readUtf8(ex.bitblock_play_chem_status(), "bitblock_play_chem_ptr");
  const chemTick = parseIntField(chemStatus, "tick");
  const chemQueued = parseIntField(chemStatus, "queued");
  const chemSpaceQ = parseIntField(chemStatus, "space_q");
  assertCond("chem simulation stepped (tick >= 2)", chemTick !== null && chemTick >= 2, chemStatus);
  assertEq("chem queue drained (queued === 0)", chemQueued, 0);
  assertCond("chem heat radiated (space_q > 0)", chemSpaceQ !== null && chemSpaceQ > 0, chemStatus);
  assertCond("chem books=ok", /(?:^|\s)books=ok(?:\s|$)/.test(chemStatus), chemStatus);
  assertEq("chem energy_drift === 0", parseIntField(chemStatus, "energy_drift"), 0);
  assertCond("chem error=none", /(?:^|\s)error=none(?:\s|$)/.test(chemStatus), chemStatus);
  assertCond(
    "chem all 12 lane drifts === 0",
    chemStatus.includes(` drift=${ZERO_LANE_DRIFT} `),
    chemStatus
  );
  verifyZeroCopyRgba(
    "chem_play_rgba",
    "bitblock_play_rgba",
    "bitblock_play_rgba_ptr",
    ex.bitblock_play_width(),
    ex.bitblock_play_height(),
    { expectedFrameTag: parseFrameTag(chemPlayStatus) }
  );
  pass("chem-step-invariants-and-negative");

  // §6ah lagged worker protocol parity: remote worker path matches local fallback byte-for-byte.
  if (typeof ex.bitblock_play_step_lagged === "function") {
    function runLaggedChem(remote, ticks) {
      assertCond("lagged chem load", ex.bitblock_play_load_chem(4, 7, 10) > 0);
      ex.bitblock_play_set_layout(320, 180, 0);
      if (remote) {
        assertEq("lagged chem remote enable", ex.bitblock_play_chem_remote(1), 1);
        assertEq("lagged chem host load", ex.bitblock_chem_host_load(4, 7, 10), 1);
      }
      const seen = [];
      let done = 0;
      while (done < ticks) {
        const r = ex.bitblock_play_step_lagged(1) >>> 0;
        const ran = r >>> 2;
        done += ran;
        if ((r & 2) !== 0) {
          assertCond("lagged send only in remote mode", remote);
          const n = ex.bitblock_play_chem_outbox();
          assertCond("lagged outbox non-empty", n > 0);
          const outboxPtr = ex.bitblock_play_chem_outbox_ptr();
          const batch = new Uint8Array(ex.memory.buffer.slice(outboxPtr, outboxPtr + n));
          const inPtr = ex.bitblock_chem_host_in_ptr(n);
          new Uint8Array(ex.memory.buffer, inPtr, n).set(batch);
          const m = ex.bitblock_chem_host_process(n);
          assertCond("lagged host reply non-empty", m > 0);
          assertEq("lagged host books=ok", ex.bitblock_chem_host_books(), 1);
          const hostOutPtr = ex.bitblock_chem_host_out_ptr();
          const reply = new Uint8Array(ex.memory.buffer.slice(hostOutPtr, hostOutPtr + m));
          const inboxPtr = ex.bitblock_play_chem_inbox_ptr(m);
          new Uint8Array(ex.memory.buffer, inboxPtr, m).set(reply);
          assertEq("lagged inbox commit", ex.bitblock_play_chem_inbox_commit(m), 1);
          assertEq("lagged double commit rejected", ex.bitblock_play_chem_inbox_commit(0), 0);
          ex.bitblock_play_step_lagged(0);
        } else if (ran === 0) {
          throw new Error(`lagged chem stalled at tick ${done} (blocked=${ex.bitblock_play_chem_blocked()})`);
        }
        if (ran === 1) {
          seen.push(readUtf8(ex.bitblock_play_status(1), "bitblock_play_ptr"));
          seen.push(readUtf8(ex.bitblock_play_chem_status(), "bitblock_play_chem_ptr"));
        }
      }
      assertEq("lagged remote toggle rejected after first batch", ex.bitblock_play_chem_remote(0), 0);
      return { seen, chemTick: ex.bitblock_play_chem_tick() };
    }

    const localRun = runLaggedChem(false, 12);
    const remoteRun = runLaggedChem(true, 12);
    assertCond("lagged chem tick > 0", localRun.chemTick > 0);
    assertEq("lagged chem tick matches remote", remoteRun.chemTick, localRun.chemTick);
    assertEq("lagged trace len matches", remoteRun.seen.length, localRun.seen.length);
    for (let i = 0; i < localRun.seen.length; i++) {
      assertEq(`lagged worker vs fallback trace[${i}]`, remoteRun.seen[i], localRun.seen[i]);
    }
    const lastChem = remoteRun.seen[remoteRun.seen.length - 1];
    assertCond("lagged chem books=ok", /(?:^|\s)books=ok(?:\s|$)/.test(lastChem), lastChem);
    assertEq("lagged chem energy_drift === 0", parseIntField(lastChem, "energy_drift"), 0);
    assertCond(
      "lagged chem all 12 lane drifts === 0",
      lastChem.includes(` drift=${ZERO_LANE_DRIFT} `),
      lastChem
    );
    pass("chem-lagged-worker-parity");
  }

  // 11. Optional embedded HTML artifact byte/SHA-256 identity + launcher non-embed check
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
    if (!customPlay) {
      const launcherPath = path.join(path.dirname(playPath), "bitblock.html");
      if (!fs.existsSync(launcherPath)) {
        throw new Error(`Launcher HTML file not found: ${launcherPath}`);
      }
      const launcherText = fs.readFileSync(launcherPath, "utf8");
      assertCond("bitblock.html launcher has no embedded WASM_B64", !launcherText.includes("WASM_B64"));
      pass("launcher-no-embed");
    }
    embeddedNote = ", embedded=play.html+bitblock-lab.html exact";
  }

  const elapsedMs = (performance.now() - started).toFixed(0);
  console.log(
    `wasm_parity_check PASS: abi=${EXPECTED_ABI}, ${REQUIRED_CONSTANTS.length}/${REQUIRED_CONSTANTS.length} known-answer constants, ` +
      `${checks} check groups, wasm=${path.relative(ROOT, wasmPath)} (${wasmBytes.length} bytes, ` +
      `sha256=${wasmSha.slice(0, 16)}…${embeddedNote}) in ${elapsedMs}ms`
  );
}

main().catch((err) => {
  console.error(`wasm_parity_check FAIL: ${err && err.message ? err.message : err}`);
  process.exit(1);
});
