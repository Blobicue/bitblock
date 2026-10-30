    // ---- #play: native-window stand-in (rAF + DOM → AcceptedAction) ----
    const playEl = document.getElementById("play");
    const playCanvas = document.getElementById("playCanvas");
    const playCap = document.getElementById("playCap");
    const playStage = document.getElementById("playStage");
    const playFpsEl = document.getElementById("playFps");
    const playQuality = document.getElementById("playQuality");
    const playGameOnly = document.getElementById("playGameOnly");
    const playWorld = document.getElementById("playWorld");
    const PLAY_WORLD_EMPTY = 4;
    // Chem worlds ("chem:<preset>" in the world menu): genesis + planet6 tick, opt-in.
    const playChemOpts = document.getElementById("playChemOpts");
    const playChemSeed = document.getElementById("playChemSeed");
    const playChemRadius = document.getElementById("playChemRadius");
    const playChemEvery = document.getElementById("playChemEvery");
    const playChemStepBtn = document.getElementById("playChemStep");
    const playChemEl = document.getElementById("playChem");
    // Chem cross-section (render-only): mode 0 off · 1 wedge · 2/3/4 x/y/z half; offset in cells.
    const playChemCut = document.getElementById("playChemCut");
    const playChemCutOffset = document.getElementById("playChemCutOffset");
    const playChemCutVal = document.getElementById("playChemCutVal");
    const CHEM_CUT_NAMES = ["off", "wedge", "x", "y", "z"];
    const CHEM_CUT_MODES = CHEM_CUT_NAMES.length;
    const PLAY_CUT_KEYS = { KeyC: 1, BracketLeft: 1, BracketRight: 1 };
    // Chem tick dynamics (opt-in; defaults = planet6): energy 0 off · 1 M1 sun+core · 2 sun8
    // matched (full sweep; the gas planet's default);
    // water cycle 0 classic (calendar) · 1 driven (unscripted).
    const playChemEnergy = document.getElementById("playChemEnergy");
    const playChemCycle = document.getElementById("playChemCycle");
    const CHEM_ENERGY_NAMES = ["off", "m1", "sun8"];
    const CHEM_CYCLE_NAMES = ["classic", "driven"];
    // Custom element budget (12 lanes) — filled from the chosen preset, applied as "chem:custom".
    const CHEM_LANES = ["H", "O", "C", "N", "S", "Ca", "Fe", "Ni", "Si", "Mg", "Na", "K"];
    const playChemBudget = document.getElementById("playChemBudget");
    const playChemLaneEls = CHEM_LANES.map(function (_, k) { return document.getElementById("playChemLane" + k); });
    const playChemApply = document.getElementById("playChemApply");
    const playChemBudgetInfo = document.getElementById("playChemBudgetInfo");
    let playChemError = "";
    // Cached once: paint never re-queries the context. alpha:false = opaque fast path.
    const playCtx = playCanvas.getContext("2d", { alpha: false });
    // Physical codes match native winit KeyCode mapping. Space must stay distinct from Escape.
    const PLAY_KEY = { KeyW: 0, KeyS: 1, KeyA: 2, KeyD: 3, Space: 4, ShiftLeft: 5, ShiftRight: 5, KeyF: 6, F3: 7, Escape: 8 };
    const TICK_MS = 1000 / 60;
    // Ticks simulated per rAF are capped; beyond that the backlog is dropped (no spiral of death).
    const MAX_TICKS_PER_FRAME = 8;
    const MAX_FRAME_GAP_MS = 250;
    const STATUS_UI_MS = 100;   // ~10 Hz DOM status refresh while the loop runs
    const IDLE_HASH_MS = 250;   // hash the shown frame (frame=#bb.frame…) once idle this long
    const FPS_UI_MS = 500;
    let playAccum = 0;
    let playLastTs = 0;
    let playRunning = false;
    let PLAY_STATUS_LEN = 0;
    let playStatusUiAt = 0;
    let playLastPaintAt = 0;
    let playHeld = Object.create(null);
    // FPS / frame-ms readout (performance.now / rAF timestamps).
    const playStats = { since: performance.now(), raf: 0, painted: 0, ticks: 0, stepMs: 0, stepMax: 0, paintMs: 0, paintMax: 0, rafGapMax: 0, renders0: 0 };

    function f64bits(n) {
      const buf = new ArrayBuffer(8);
      new Float64Array(buf)[0] = n;
      return new BigUint64Array(buf)[0];
    }

    function playOwnsInput() {
      return document.activeElement === playCanvas || document.pointerLockElement === playCanvas;
    }

    function playCodeFromEvent(event) {
      if (PLAY_KEY[event.code] !== undefined) return event.code;
      // Fallback: some environments only populate key for Space.
      if (event.key === " " || event.key === "Spacebar") return "Space";
      return null;
    }

    function playSelectedLayout() {
      const [w, h] = playQuality.value.split(",").map(Number);
      return { w: w, h: h, sidebar: !playGameOnly.checked };
    }

    // Game window: CSS-fit the internal-resolution canvas into the full-viewport stage (contain: the whole
    // frame incl. the in-frame telemetry sidebar stays visible; leftover is black, no page scroll).
    function playFitCanvas() {
      const s = playStage.getBoundingClientRect();
      const w = playCanvas.width, h = playCanvas.height;
      const scale = Math.max(0.05, Math.min(s.width / w, s.height / h));
      playCanvas.style.width = Math.floor(w * scale) + "px";
      playCanvas.style.height = Math.floor(h * scale) + "px";
    }
    window.addEventListener("resize", playFitCanvas);

    // Match the canvas backing store to the session output; CSS scales it to the stage.
    function playSyncCanvasSize() {
      const w = instance.exports.bitblock_play_width();
      const h = instance.exports.bitblock_play_height();
      if (playCanvas.width !== w || playCanvas.height !== h) {
        playCanvas.width = w;
        playCanvas.height = h;
        playCanvas.style.aspectRatio = w + "/" + h;
        playFitCanvas();
      }
      return { w: w, h: h };
    }

    // Zero-copy: ImageData over a view of wasm memory (no slice); one putImageData per frame.
    function playPaint() {
      const t0 = performance.now();
      const { w, h } = playSyncCanvasSize();
      const len = instance.exports.bitblock_play_rgba();
      if (len === w * h * 4) {
        const ptr = instance.exports.bitblock_play_rgba_ptr();
        const view = new Uint8ClampedArray(instance.exports.memory.buffer, ptr, len);
        playCtx.putImageData(new ImageData(view, w, h), 0, 0);
      }
      instance.exports.bitblock_play_clear_dirty();
      const dt = performance.now() - t0;
      playStats.paintMs += dt;
      if (dt > playStats.paintMax) playStats.paintMax = dt;
      playStats.painted++;
      playLastPaintAt = performance.now();
    }

    function refreshPlay(note) {
      const status = readUtf8(instance, PLAY_STATUS_LEN, "bitblock_play_ptr");
      const w = instance.exports.bitblock_play_width();
      const h = instance.exports.bitblock_play_height();
      const ptrlock = document.pointerLockElement === playCanvas ? 1 : 0;
      // captured= is the session latch (AcceptedAction gate); ptrlock= is real browser lock.
      playEl.textContent = ["abi="+abi, "wasm_bytes="+bytes.byteLength, "note="+note, "ptrlock="+ptrlock, status].join("  ");
      playEl.className = status.indexOf("bitblock_browser_play ok") === 0 ? "ok" : "err";
      const L = playSelectedLayout();
      const rect = playCanvas.getBoundingClientRect();
      playCap.textContent = "play internal="+w+"x"+h+" (game "+(L.sidebar ? (w-400)+"x"+h+" + sidebar 400" : w+"x"+h+" only")+") css="+Math.round(rect.width)+"x"+Math.round(rect.height)+" upscale="+(rect.width / w).toFixed(2)+"x ptrlock="+ptrlock+" ("+note+")";
      return status;
    }

    // Status text only; hash=1 computes (and caches) the frame tag of the shown frame.
    function playPulseStatus(note, hash) {
      PLAY_STATUS_LEN = instance.exports.bitblock_play_status(hash ? 1 : 0);
      return refreshPlay(note);
    }

    function playKeyHandler(code, pressed) {
      const id = PLAY_KEY[code];
      if (id === undefined) return;
      PLAY_STATUS_LEN = instance.exports.bitblock_play_key(id, pressed ? 1 : 0);
      // Native Esc calls capture_mouse(false): release the OS pointer too, not only session capture.
      if (code === "Escape" && pressed && document.pointerLockElement) document.exitPointerLock();
    }
    function playLookHandler(dx, dy) {
      PLAY_STATUS_LEN = instance.exports.bitblock_play_look(f64bits(dx), f64bits(dy));
    }
    function playWheelHandler(lines) {
      PLAY_STATUS_LEN = instance.exports.bitblock_play_wheel(f64bits(lines));
    }
    function playCursorHandler(x, y) {
      PLAY_STATUS_LEN = instance.exports.bitblock_play_cursor(f64bits(x), f64bits(y));
    }
    function playPointerButtonHandler(button, pressed) {
      PLAY_STATUS_LEN = instance.exports.bitblock_play_pointer_button(button, pressed ? 1 : 0);
    }
    function playSetCaptureHandler(capture) {
      PLAY_STATUS_LEN = instance.exports.bitblock_play_set_capture(capture ? 1 : 0);
    }
    // Audited native-faithful tick (tape): render + palette audit + eager frame tags every tick.
    function playAdvanceTick() {
      PLAY_STATUS_LEN = instance.exports.bitblock_play_tick();
      return instance.exports.bitblock_play_dirty() !== 0;
    }
    // Apply the selected quality / sidebar layout to the live session (Resize like native).
    function playApplyLayout() {
      const L = playSelectedLayout();
      PLAY_STATUS_LEN = instance.exports.bitblock_play_set_layout(L.w, L.h, L.sidebar ? 1 : 0);
      // Canvas backing store is resized at the next paint (no blank flash in between).
    }
    // Live step: n sim ticks, then at most one frame; paint only if a frame was finalised.
    function playStepAndPaint(ticks) {
      const t0 = performance.now();
      const painted = instance.exports.bitblock_play_step(ticks) !== 0;
      const dt = performance.now() - t0;
      playStats.stepMs += dt;
      if (dt > playStats.stepMax) playStats.stepMax = dt;
      playStats.ticks += ticks;
      if (painted) playPaint();
      return painted;
    }

    function playUpdateFps(now) {
      const span = now - playStats.since;
      if (span < FPS_UI_MS) return;
      const renders = instance.exports.bitblock_play_renders();
      const presented = Math.max(0, renders - playStats.renders0);
      const rafFps = playStats.raf * 1000 / span;
      const frameFps = presented * 1000 / span;
      const stepAvg = playStats.raf ? playStats.stepMs / playStats.raf : 0;
      const paintAvg = playStats.painted ? playStats.paintMs / playStats.painted : 0;
      const busy = (playStats.stepMs + playStats.paintMs) / span * 100;
      playFpsEl.textContent = "raf_fps="+rafFps.toFixed(1)+" presented_fps="+frameFps.toFixed(1)+" tick_rate="+(playStats.ticks*1000/span).toFixed(1)+"/s"
        + "\nstep_ms avg="+stepAvg.toFixed(2)+" max="+playStats.stepMax.toFixed(2)
        + "  paint_ms avg="+paintAvg.toFixed(2)+" max="+playStats.paintMax.toFixed(2)
        + "  ms_per_presented_frame(step+paint)="+(playStats.painted ? (playStats.stepMs + playStats.paintMs) / playStats.painted : 0).toFixed(2)
        + "  raf_gap_max="+playStats.rafGapMax.toFixed(1)+"ms  main_thread_busy="+busy.toFixed(0)+"%"
        + "\noutput="+playCanvas.width+"x"+playCanvas.height+" (presented_fps counts only changed frames; idle view = 0 by design)";
      playFpsEl.className = rafFps >= 30 ? "ok" : "err";
      playStats.since = now; playStats.raf = 0; playStats.painted = 0; playStats.ticks = 0;
      playStats.stepMs = 0; playStats.stepMax = 0; playStats.paintMs = 0; playStats.paintMax = 0; playStats.rafGapMax = 0;
      playStats.renders0 = renders;
    }

    function playLoop(ts) {
      if (!playRunning) return;
      if (!playLastTs) playLastTs = ts;
      let gap = ts - playLastTs;
      if (gap > playStats.rafGapMax) playStats.rafGapMax = gap;
      if (gap > MAX_FRAME_GAP_MS) gap = MAX_FRAME_GAP_MS;
      playLastTs = ts;
      playAccum += gap;
      playStats.raf++;
      let ticks = Math.floor(playAccum / TICK_MS);
      if (ticks > MAX_TICKS_PER_FRAME) { ticks = MAX_TICKS_PER_FRAME; playAccum = 0; }
      else playAccum -= ticks * TICK_MS;
      if (ticks > 0) playStepAndPaint(ticks);
      const now = performance.now();
      if (now - playStatusUiAt >= STATUS_UI_MS) {
        // Hash only once the view has been still: hashing is a full-frame SHA-256 in wasm.
        playPulseStatus("live", now - playLastPaintAt >= IDLE_HASH_MS);
        playRefreshChem();
        playStatusUiAt = now;
      }
      playUpdateFps(now);
      requestAnimationFrame(playLoop);
    }

    function ensurePlayLoop() {
      if (!playRunning) {
        playRunning = true;
        playLastTs = 0;
        playAccum = 0;
        requestAnimationFrame(playLoop);
      }
    }

    // Chem world from the menu: "chem:0" ash, "chem:1" rocky, "chem:2" ocean → 0..2,
    // "chem:custom" → CHEM_CUSTOM (genesis from the budget inputs), else -1.
    const CHEM_CUSTOM = 3;
    const CHEM_GAS = 4; // §5y gas planet: switching to it selects energy sun8 + driven cycle
    let playLastChem = -1;
    function playSelectedChem() {
      const v = String(playWorld ? playWorld.value : "");
      if (v === "chem:custom") return CHEM_CUSTOM;
      const m = /^chem:([0-9]+)$/.exec(v);
      return m ? Number(m[1]) : -1;
    }
    function playNum(input, def, lo, hi) {
      if (!input || input.value === "" || input.value === undefined) return def;
      const n = Math.floor(Number(input.value));
      return Number.isFinite(n) ? Math.max(lo, Math.min(hi, n)) : def;
    }
    function playChemEveryTicks() { return playNum(playChemEvery, 4, 0, 600); }
    function playChemSeedVal() { return playNum(playChemSeed, 7, 0, 4294967295) >>> 0; }
    function playChemRadiusVal() { return playNum(playChemRadius, 12, 2, 32); }
    // The wasm-side 12 x i64 budget buffer (view re-made each use: memory may have grown).
    function playChemBudgetView() {
      return new BigInt64Array(instance.exports.memory.buffer, instance.exports.bitblock_play_chem_budget_ptr(), CHEM_LANES.length);
    }
    // Same mask as genesis (d² <= r²) — only for the atoms-per-cell hint.
    function playSphereCells(r) {
      let n = 0;
      for (let x = -r; x <= r; x++) for (let y = -r; y <= r; y++) for (let z = -r; z <= r; z++) if (x*x + y*y + z*z <= r*r) n++;
      return n;
    }
    // Budget inputs → BigInt lanes. Blank = 0. Signs pass through (wasm rejects negatives and
    // reports why); non-integers / out-of-i64 are caught here.
    function playChemReadLanes() {
      const lanes = [];
      for (let k = 0; k < CHEM_LANES.length; k++) {
        const el = playChemLaneEls[k];
        const raw = String(el && el.value !== undefined ? el.value : "").trim();
        if (raw === "") { lanes.push(0n); continue; }
        if (!/^-?[0-9]+$/.test(raw)) return { err: "budget lane " + CHEM_LANES[k] + ": '" + raw + "' is not a whole number" };
        const v = BigInt(raw);
        if (v > 9223372036854775807n || v < -9223372036854775808n) return { err: "budget lane " + CHEM_LANES[k] + ": out of range" };
        lanes.push(v);
      }
      return { lanes: lanes };
    }
    function playChemWriteInputs(lanes) {
      for (let k = 0; k < CHEM_LANES.length; k++) if (playChemLaneEls[k]) playChemLaneEls[k].value = String(lanes[k]);
      playChemUpdateInfo();
    }
    // Copy preset p's budget (for the current radius) into the inputs.
    function playChemFillFromPreset(p) {
      if (!instance.exports.bitblock_play_chem_preset_budget(p, playChemRadiusVal())) return false;
      playChemWriteInputs(Array.from(playChemBudgetView()));
      return true;
    }
    function playChemUpdateInfo() {
      if (!playChemBudgetInfo) return;
      const r = playChemReadLanes();
      const cells = playSphereCells(playChemRadiusVal());
      if (r.err) { playChemBudgetInfo.textContent = r.err; playChemBudgetInfo.className = "err"; return; }
      let total = 0n;
      for (const v of r.lanes) total += v;
      let text = "atoms=" + total + " sphere_cells=" + cells + " atoms/cell=" + (Number(total) / cells).toFixed(1);
      if (playChemError) text += "  error=" + playChemError;
      playChemBudgetInfo.textContent = text;
      playChemBudgetInfo.className = playChemError ? "err" : "";
    }
    // Genesis from the budget inputs. Returns the status length, or 0 (session unchanged,
    // playChemError says why).
    function playChemLoadCustom() {
      const r = playChemReadLanes();
      if (r.err) { playChemError = r.err; return 0; }
      const view = playChemBudgetView();
      for (let k = 0; k < CHEM_LANES.length; k++) view[k] = r.lanes[k];
      const len = instance.exports.bitblock_play_load_chem_budget(playChemSeedVal(), playChemRadiusVal());
      if (!len) {
        const n = instance.exports.bitblock_play_chem_error();
        playChemError = n ? readUtf8(instance, n, "bitblock_play_chem_ptr") : "genesis failed";
      }
      return len;
    }
    // Debug readout: lanes / lane drift / bag / vapor / chem tick (own wasm text slot).
    function playRefreshChem() {
      if (!playChemEl) return;
      const len = instance.exports.bitblock_play_chem_status();
      const text = len ? readUtf8(instance, len, "bitblock_play_chem_ptr") : "";
      const err = playChemError ? "chem_load_error=" + playChemError + "\n" : "";
      playChemEl.textContent = err + (len ? text.split(" ").join("\n") : "chem: off");
      playChemEl.className = err ? "err" : !len ? "" : (/ books=ok /.test(text) ? "ok" : "err");
    }
    function playChemCutMode() {
      const m = Number(playChemCut ? playChemCut.value : 0);
      return Number.isInteger(m) && m >= 0 && m < CHEM_CUT_MODES ? m : 0;
    }
    // Push the menu's cross-section to wasm (chem sessions only); offset range follows the radius.
    function playApplyChemCut() {
      if (!playChemCut || !instance.exports.bitblock_play_chem_cut) return;
      const r = playChemRadiusVal();
      if (playChemCutOffset) {
        playChemCutOffset.min = String(-r);
        playChemCutOffset.max = String(r);
      }
      const off = Math.max(-r, Math.min(r, Math.trunc(Number(playChemCutOffset ? playChemCutOffset.value : 0)) || 0));
      if (playChemCutOffset) playChemCutOffset.value = String(off);
      if (playChemCutVal) playChemCutVal.textContent = String(off);
      if (playSelectedChem() < 0) return;
      instance.exports.bitblock_play_chem_cut(playChemCutMode(), off);
      ensurePlayLoop();
    }
    function playChemSelVal(el, n) {
      const m = Number(el ? el.value : 0);
      return Number.isInteger(m) && m >= 0 && m < n ? m : 0;
    }
    function playChemEnergyMode() { return playChemSelVal(playChemEnergy, CHEM_ENERGY_NAMES.length); }
    function playChemCycleMode() { return playChemSelVal(playChemCycle, CHEM_CYCLE_NAMES.length); }
    // Push the menu's energy / water-cycle choice to the live chem world (chem sessions only).
    function playApplyChemDynamics() {
      if (!instance.exports.bitblock_play_chem_dynamics || playSelectedChem() < 0) return;
      instance.exports.bitblock_play_chem_dynamics(playChemEnergyMode(), playChemCycleMode());
    }
    // In-game keys (chem worlds): C cycles the cut mode, [ / ] move the cut plane.
    function playChemCutKey(code) {
      if (code === "KeyC") {
        playChemCut.value = String((playChemCutMode() + 1) % CHEM_CUT_MODES);
      } else if (playChemCutOffset) {
        const step = code === "BracketRight" ? 1 : -1;
        playChemCutOffset.value = String(Number(playChemCutOffset.value || 0) + step);
      }
      playApplyChemCut();
      playPulseStatus("cut:" + CHEM_CUT_NAMES[playChemCutMode()] + ":" + (playChemCutOffset ? playChemCutOffset.value : 0), false);
    }
    // Start world: gallery program 0..3 under the gallery camera, or 4 = empty spawn.
    function playSelectedWorld() {
      const id = Number(playWorld ? playWorld.value : PLAY_WORLD_EMPTY);
      return Number.isInteger(id) && id >= 0 && id <= PLAY_WORLD_EMPTY ? id : PLAY_WORLD_EMPTY;
    }
    function playResetLive(note) {
      const chem = playSelectedChem();
      let len = 0;
      playChemError = "";
      if (chem === CHEM_GAS && playLastChem !== CHEM_GAS && playChemRadius && playChemRadiusVal() <= 12) {
        // §5y: the gas demo opens at r20 (~4.6x the r12 cells; measured ~50 fps raf / full
        // chem cadence at Low in Chrome). r24 ~30 fps; r32 is allowed but ~6 fps (CPU render).
        playChemRadius.value = "20";
      }
      if (chem === CHEM_CUSTOM) {
        len = playChemLoadCustom();
        if (len) instance.exports.bitblock_play_chem_auto(playChemEveryTicks());
      } else if (chem >= 0) {
        len = instance.exports.bitblock_play_load_chem(chem, playChemSeedVal(), playChemRadiusVal());
        if (len) instance.exports.bitblock_play_chem_auto(playChemEveryTicks());
        playChemFillFromPreset(chem); // show / tweak this preset's lanes
      } else {
        len = instance.exports.bitblock_play_load_world(playSelectedWorld());
      }
      if (chem === CHEM_GAS && playLastChem !== CHEM_GAS) {
        // the gas planet's baseline; later menu changes stick across reseeds
        if (playChemEnergy) playChemEnergy.value = "2";
        if (playChemCycle) playChemCycle.value = "1";
      }
      playLastChem = chem;
      if (playChemOpts) playChemOpts.hidden = chem < 0;
      if (playChemBudget) playChemBudget.hidden = chem < 0;
      playChemUpdateInfo();
      if (len && chem >= 0) playApplyChemCut(); // a fresh chem session keeps the menu's cut
      if (len && chem >= 0) playApplyChemDynamics(); // ... and its energy / water-cycle choice
      // A rejected custom budget keeps the running session (error shown); else reset fallback.
      PLAY_STATUS_LEN = len || (chem === CHEM_CUSTOM ? instance.exports.bitblock_play_status(0) : instance.exports.bitblock_play_reset());
      playHeld = Object.create(null);
      playApplyLayout();
      playStepAndPaint(1);
      playStats.renders0 = instance.exports.bitblock_play_renders();
      playPulseStatus(note, true);
      playRefreshChem();
      playStatusUiAt = performance.now();
    }

    playChemFillFromPreset(0); // custom starts from the ash numbers until a preset is picked
    playResetLive("reset");
    ensurePlayLoop();

    function onPlayLayoutChange() {
      // Like a native window resize: the next fixed tick refits (if top-saturated) and renders.
      playApplyLayout();
      playPulseStatus("layout", false);
      if (playCanvas.focus) playCanvas.focus({ preventScroll: true });
    }
    playQuality.addEventListener("change", onPlayLayoutChange);
    if (playWorld) playWorld.addEventListener("change", function () {
      // Load that program into a fresh session (same as reset, with the chosen world).
      playResetLive("world");
      if (playCanvas.focus) playCanvas.focus({ preventScroll: true });
    });
    playGameOnly.addEventListener("change", onPlayLayoutChange);
    // Chem world options: seed / radius rebuild the chem world; cadence and manual step are live.
    function onPlayChemRebuild() {
      if (playSelectedChem() < 0) return;
      playResetLive("chem");
      if (playCanvas.focus) playCanvas.focus({ preventScroll: true });
    }
    if (playChemSeed) playChemSeed.addEventListener("change", onPlayChemRebuild);
    if (playChemRadius) playChemRadius.addEventListener("change", onPlayChemRebuild);
    // Apply (button or Enter in a lane): genesis "chem · custom budget" from the inputs.
    function onPlayChemApply() {
      if (playWorld) playWorld.value = "chem:custom";
      playResetLive("chem-budget");
      if (!playChemError && playCanvas.focus) playCanvas.focus({ preventScroll: true });
    }
    if (playChemApply) playChemApply.addEventListener("click", onPlayChemApply);
    for (const el of playChemLaneEls) {
      if (!el) continue;
      el.addEventListener("input", playChemUpdateInfo);
      el.addEventListener("keydown", function (event) {
        if (event.key === "Enter") { event.preventDefault(); onPlayChemApply(); }
      });
    }
    if (playChemEvery) playChemEvery.addEventListener("change", function () {
      instance.exports.bitblock_play_chem_auto(playChemEveryTicks());
      ensurePlayLoop();
    });
    if (playChemStepBtn) playChemStepBtn.addEventListener("click", function () {
      instance.exports.bitblock_play_chem_step(1);
      ensurePlayLoop();
      if (playCanvas.focus) playCanvas.focus({ preventScroll: true });
    });

    function onPlayChemCutMenu() {
      playApplyChemCut();
      if (playSelectedChem() >= 0) playPulseStatus("cut:" + CHEM_CUT_NAMES[playChemCutMode()] + ":" + playChemCutOffset.value, false);
    }
    if (playChemCut) playChemCut.addEventListener("change", function () {
      onPlayChemCutMenu();
      if (playCanvas.focus) playCanvas.focus({ preventScroll: true });
    });
    if (playChemCutOffset) playChemCutOffset.addEventListener("input", onPlayChemCutMenu);

    function onPlayChemDynamicsMenu() {
      playApplyChemDynamics();
      if (playSelectedChem() >= 0) {
        playPulseStatus("dynamics:energy=" + CHEM_ENERGY_NAMES[playChemEnergyMode()] + ":cycle=" + CHEM_CYCLE_NAMES[playChemCycleMode()], false);
        playRefreshChem();
      }
      if (playCanvas.focus) playCanvas.focus({ preventScroll: true });
    }
    if (playChemEnergy) playChemEnergy.addEventListener("change", onPlayChemDynamicsMenu);
    if (playChemCycle) playChemCycle.addEventListener("change", onPlayChemDynamicsMenu);

    function onPlayKeyDown(event) {
      if (!playOwnsInput()) return;
      if (PLAY_CUT_KEYS[event.code] && playChemCut && playSelectedChem() >= 0) {
        event.preventDefault();
        event.stopPropagation();
        if (!(event.repeat && event.code === "KeyC")) playChemCutKey(event.code);
        return;
      }
      const code = playCodeFromEvent(event);
      if (!code) return;
      if (event.repeat && (code === "F3" || code === "KeyF")) return;
      // F1–F12 / Space: stop browser chrome (find-next, page scroll, button activate).
      event.preventDefault();
      event.stopPropagation();
      if (playHeld[code]) return; // native ignores synthetic repeats for movement latch
      playHeld[code] = true;
      playKeyHandler(code, true);
      ensurePlayLoop();
    }
    function onPlayKeyUp(event) {
      const code = playCodeFromEvent(event);
      if (!code) return;
      if (!playHeld[code] && !playOwnsInput()) return;
      event.preventDefault();
      event.stopPropagation();
      playHeld[code] = false;
      playKeyHandler(code, false);
    }
    // Document-level so Space/F3 reach us under pointer lock and cannot activate focused buttons.
    document.addEventListener("keydown", onPlayKeyDown, true);
    document.addEventListener("keyup", onPlayKeyUp, true);

    playCanvas.addEventListener("mousemove", function (event) {
      const { px, py } = canvasPixelFromEvent(playCanvas, event);
      playCursorHandler(px + 0.5, py + 0.5);
      if (document.pointerLockElement === playCanvas) {
        playLookHandler(event.movementX, event.movementY);
      }
    });
    playCanvas.addEventListener("wheel", function (event) {
      if (!playOwnsInput() && document.activeElement !== playCanvas) return;
      event.preventDefault();
      const lines = event.deltaMode === 1 ? -event.deltaY : -event.deltaY / 120;
      playWheelHandler(lines);
      ensurePlayLoop();
    }, { passive: false });
    playCanvas.addEventListener("mousedown", function (event) {
      // Take focus off buttons so Space cannot activate play tape / reset.
      if (document.activeElement && document.activeElement !== playCanvas && document.activeElement.blur) {
        document.activeElement.blur();
      }
      playCanvas.focus({ preventScroll: true });
      const { px, py } = canvasPixelFromEvent(playCanvas, event);
      playCursorHandler(px + 0.5, py + 0.5);
      playPointerButtonHandler(event.button, true);
      if (document.pointerLockElement !== playCanvas && event.button === 0) {
        if (playCanvas.requestPointerLock) playCanvas.requestPointerLock();
      }
      ensurePlayLoop();
      refreshPlay("mousedown");
    });
    playCanvas.addEventListener("contextmenu", function (event) { event.preventDefault(); });
    // Pause menu = HTML overlay shown only while the mouse is unlocked (load, Esc); playing is chrome-less.
    function playSetMenuHidden(hidden) {
      if (document.body && document.body.classList) document.body.classList.toggle("locked", hidden);
    }
    document.addEventListener("pointerlockchange", function () {
      playSetMenuHidden(document.pointerLockElement === playCanvas);
      if (document.pointerLockElement === playCanvas) {
        playSetCaptureHandler(true);
        refreshPlay("pointerlock-on");
      } else {
        playSetCaptureHandler(false);
        // Release any stuck movement latches when lock drops (matches native capture_mouse(false) → ClearMotion).
        for (const code of Object.keys(playHeld)) {
          if (playHeld[code]) {
            playHeld[code] = false;
            playKeyHandler(code, false);
          }
        }
        refreshPlay("pointerlock-off");
      }
    });
    document.getElementById("playReset").addEventListener("click", function () {
      playResetLive("reset");
      playCanvas.focus({ preventScroll: true });
    });

