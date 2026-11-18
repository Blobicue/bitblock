# BitBlock: emergent scale (chem + time → Minecraft-scale world)

Date: 2026-10-27 (PDT). Scope: measurement + design only. `/workspace/bitblock` untouched (no source edits,
no commit/push, no wasm rebuild; play wasm stays `6aa17b7930de48496a93d7cf62c0a385`, abi 17; play
`CHEM_RADIUS_MAX` stays 20, default r=12). Soft DDA / bcc-voxel-demo untouched. Probes live in
`/workspace/scale_measure/` (separate crate, path-depends on core + wasm).

Related: `BITBLOCK_GENESIS_BUDGET.md` (budget → plan → shells → bag), `BITBLOCK_P6_TICK.md` (tick laws, books),
`BITBLOCK_CHEM_PERF.md` (per-stage perf, render), `LAZY-CHEM-SPEC.md` (lazy *reaction derivation* for 118
elements). LAZY-CHEM is about *which reactions exist*; this note is about *which cells are ticked*. They are
orthogonal: lazy derivation caps per-cell reaction cost, sparse-active domains cap the number of cells paid for.

## 1. North star (Owen)
> "continue scaling so we get to the minecraft level scale, but instead of relying on algorithms like minecraft
> uses for terrain generation, our terrain generation would simply be a result of time and chemical diffusion
> existing. So naturally formed clouds, atmospheres, etc."

Operationally:
- **Genesis places atoms, not landforms.** Budget → radial shells / ash is the only initial condition. No
  heightmap, no noise field, no biome table is ever an authority over what is where.
- **Time sculpts.** Relief, seas, ice caps, sediment fans, caves, atmosphere and clouds are whatever heat,
  reactions, the water cycle, erosion, settling and flow leave behind.
- **The books are law at every scale.** Lane drift = 0 and energy drift = 0 globally, every tick, no matter
  how the world is partitioned, frozen or streamed.

## 2. M0 measurement: dense whole-domain step cost vs radius

Native release (x86_64 Xeon box, single thread, `opt-level=3`, thin LTO), play preset budgets
(`preset_budget`), seed 1, trimmed ChemNet, full `step()` (heat, phase, combine, combine2, water, erode,
dissolve, settle, flow). Timings after 3 warm steps; steps per point = clamp(3M cell-steps / cells, 5, 100).
Run 2 (isolated) shown; run 1 agrees within ±7% (`curve_run1.csv`). Books: energy drift 0 and lane drift 0
in every run.

| r | domain cells | rocky ms/step | ocean ms/step | rocky ms/1k cells | ocean ms/1k cells | comb2 share | RSS (MB) |
|---|---:|---:|---:|---:|---:|---:|---:|
| 2 | 33 | 0.005 | 0.004 | 0.159 | 0.132 | — | 3 |
| 4 | 257 | 0.033 | 0.034 | 0.127 | 0.133 | 71% | 3 |
| 8 | 2,109 | 0.392 | 0.460 | 0.186 | 0.218 | 78% | 3 |
| **12 (play default)** | 7,153 | **1.229** | **1.399** | 0.172 | 0.196 | 78% | 4 |
| 16 | 17,077 | 2.800 | 3.119 | 0.164 | 0.183 | 76% | 5 |
| **20 (play max)** | 33,401 | **5.107** | **5.839** | 0.153 | 0.175 | 75% | 8 |
| 24 | 57,777 | 8.768 | 9.819 | 0.152 | 0.170 | 72% | 11 |
| 32 | 137,065 | 20.34 | 22.38 | 0.148 | 0.163 | 70% | 21–24 |
| 48 | 462,781 | 70.28 | 71.90 | 0.152 | 0.155 | 70% | 62–68 |
| 64 | 1,097,917 | 174.4 | 185.7 | 0.159 | 0.169 | 70% | 162 |
| 80 | 2,143,641 | 361.6 | 380.3 | 0.169 | 0.177 | 70% | 275 |
| **103 (core max)** | 4,576,475 | **771.9** | **786.3** | 0.169 | 0.172 | 70% | 583 |

Per-stage (r103 rocky, ms): heat 124.5, comb2 540.8, erode 58.5 (storm ticks), dissolve 16.5, water 14.0,
flow 13.6, settle 5.4, phase 3.6, combine 1.7. Genesis itself: 1.4–2.1 s at r103 (≈ one step's worth ×2).

Headlines:
- **Cost is linear in domain cells: 0.15–0.20 ms per 1k cells per step**, flat from r8 to r103 (no cache
  cliff yet; slight rise past 1M cells). combine2 (network law) is ~70–78%, heat ~15–17%.
- **Memory ≈ 127 B/cell** at r103 (includes the bench's two 8 B/cell snapshots; core alone is ~110 B/cell:
  24 B `Coord` + 1 B mat + 4 B q + topology slot box ≤8×cells×4 B + keys + ysort + rise).
- wasm in the browser runs this path ~1.75× slower (CHEM_PERF: comb2 2.0 ms native vs ~3.5 ms wasm at r12).

What that buys:
| budget | native cells | ≈ r | wasm cells | ≈ r |
|---|---:|---:|---:|---:|
| 2 ms per frame on main thread | ~12k | 14 | ~7k | 12 |
| 66 ms (one step per 15 Hz tick, on a worker) | ~400k | 45 | ~230k | 38 |
| 1 s per step (slow-time soak) | ~6M | 113 | ~3.4M | 93 |

Minecraft for scale: one 16×16×384 chunk = 98,304 cells → **~16 ms/step dense native**. Render distance 8 =
17×17 chunks = 28.4M cells → ~4.8 s/step and ~3.6 GB; render distance 12 = 61.4M cells → **~10 s/step and
~7.8 GB**. That is 4–5 orders of magnitude off a 15 Hz browser tick.

### Activity: almost nothing that costs money is changing
Same bench, sampled one-step diffs (fraction of domain cells per step):

| r | material changed (rocky / ocean) | (material, Q) changed |
|---|---|---|
| 12 | 0.07% / 0.29% | 83% / 85% |
| 32 | 0.05% / 0.90% | 86% / 86% |
| 48 | 0.06% / 1.48% | 67% / 65% |
| 64 | 0.07% / 1.39% | 54% / 51% |
| 103 | 0.12% / 1.19% | 38% / 36% |

Material changes touch ≤1.5% of cells per step; the Q (heat) field touches most of them early on, and the
fraction falls as the planet grows (the interior hasn't been reached by the heat front yet).

### Soak at r12 (30k ticks ≈ 33 min of play at the 15 Hz default cadence) — `chem_soak`
| tick | rocky surf-relief σ (cells) | rocky Q-active/step | ocean surf-relief σ | ocean Q-active/step | ocean vapor / evap / rained |
|---|---:|---:|---:|---:|---|
| 0 | 0.246 | — | 0.246 | — | 0 / 0 / 0 |
| 3,000 | 0.714 | 72% | 1.089 | 71% | 339 / 2075 / 1736 |
| 6,000 | 0.737 | 60% | 0.975 | 57% | 333 / 2181 / 1848 |
| 9,000 | 0.720 | 6.6% | 0.975 | 2.8% | 333 / 2181 / 1848 |
| 12,000–30,000 | 0.720 | **0%** | 0.975 | **0%** | frozen |

Findings from the soak:
1. **Relief does emerge from time alone**: outer-surface σ triples (rocky, sediment fans) to quadruples
   (ocean at rest: 1,038 ice + 872 water + 972 sediment, 47 enclosed-air cells; rocky: 1,728 sediment, 17). This is the north
   star working, at small scale.
2. **Then the world dies.** By ~tick 9–12k all genesis heat has leaked to `space_q` (rocky 9.45M, ocean 7.58M),
   water freezes or sits below the `q > 200` evaporation gate, rain finds no `AIR`-next-to-`WATER` cell, and
   every cell is bit-identical tick after tick. **The dense step still costs 1.1–1.3 ms per tick to do nothing**
   (18k+ ticks of pure waste in a 30k soak).
3. Some "weather" is calendar-scripted, not emergent: `SEASON=240`, storm window `tick % 240 < 48`, rain window
   `96..144`; rain lands at the **lowest-index** eligible cell (planet6 restart scan, O(n) per drop). These are
   planet6 parity laws; keep them for parity mode, but the emergent mode needs an energy-driven replacement.
4. Atmosphere has **no volume**: genesis domain = the ball (solid cells = domain cells at t0); vapor is a scalar
   pool (`vapor: i64`), not cells. Clouds cannot be seen because they do not exist spatially.

## 3. Why the dense whole-world tick cannot reach MC scale
- Linear cost × cubic cell count: 10 s/step at MC render distance 12, before render or wasm overhead.
- Memory: ~110–127 B/cell dense → GBs; the topology slot box alone is 4 B × bounding box.
- Most of the cost is spent on cells that cannot change (≤1.5% material activity; 0% after quiescence).
- Several laws scan the whole domain sequentially by index (rain restart scan, gas-rise / settle / flow in
  global y-order). Their *semantics* are global orderings, so naive partitioning changes results.
- The roll RNG keys on `cell_key` low 16 bits and `tick & 0xffff`: beyond ~65k cells (r≈25) roll keys alias
  spatially (period-48 lattice), and after 65,536 ticks the roll pattern repeats in time. At MC scale that
  imprints a repeating grid onto "emergent" terrain — a fake-terrain artifact we must not ship.

## 4. Target architecture
**World = sparse occupancy + material field; chem = active pockets; books = global.**

1. **Sparse occupancy store.** Chunked (e.g. 16³ or 32³ bricks) `mat` + `q` arrays, allocated only where
   anything other than "empty space at background Q" exists. Uniform bricks (all rock at equilibrium, all
   void) collapse to one value. Render keeps reading the occupancy `World` as today.
2. **Activity / dirty set.** A brick is *active* if any cell in it or its 1-cell halo changed material, or Q
   differs from neighbours beyond what the integer heat gather would move (i.e. heat would not be a no-op), or
   it borders vapor/fluid, or the player touched it. Only active bricks (+halo) step. A brick that produced an
   identical step goes dormant; any write into its halo wakes it. Dormancy must be an *exact* no-op proof per
   law (heat gather moves 0, no reaction intent can fire, no fluid/sediment can move, no roll-gated law has an
   eligible cell) — the time-keyed rolls mean "nothing happened this tick" is not by itself proof.
3. **Books stay global.** Lane totals and energy are a sum over all bricks + bag + `space_q` + `vapor`, kept
   incrementally (each brick holds its partial sums; dormant bricks contribute frozen partials). `check_books`
   = Σ partials. Freezing never removes atoms or energy from the books.
4. **Genesis = initial atom placement only.** Same budget → plan → leftovers → radial shells → bag. At MC scale:
   regional ash/shell placement per macro-region with the global budget split deterministically (e.g. by
   region volume), so Σ regions == budget exactly. Nothing in genesis decides landforms.
5. **Time does the sculpting.** A world is "made" by soaking: fast-forward N ticks headless (native/worker),
   with only active bricks paid for; the player arrives into whatever the chemistry left.
6. **Energy input (needed for anything to keep happening).** A booked source — insolation on sky-facing
   cells and/or a radiogenic core term — with a matching ledger entry (like `space_q` but inbound), so
   books still close: `E_total(t) = E_total(t0) + sun_in − space_out`. Without it, every world reaches the
   r12 soak's heat death.
7. **Atmosphere volume.** A shell of domain cells above the surface (air with Q, gas species, and vapor as
   cells or per-cell vapor counts), sparse-allocated like everything else. Clouds = cells where vapor
   condenses against cold air — rendered from chem state, not from a cloud generator.
8. **Determinism under partitioning.** Replace planet6's global-index orderings with brick-local orderings +
   a deterministic claim protocol across halos (the v5 claim rule already resolves contested targets by lowest
   index; generalise to (brick, index)). Keep the planet6-parity dense path as the golden reference for r ≤ 24.
9. **RNG.** Widen roll keys to full 64-bit cell coordinates and tick for the scaled path (parity path keeps
   planet6's 16-bit lattice).

## 5. Milestones

| M | What | Exit criteria |
|---|---|---|
| **M0 measure** (done, this note) | dense cost curve r2–103, activity fractions, r12 soak | table above; `run_curve.sh` re-runnable; books 0 drift |
| **M1 longer-time emergence at current size** (energy input **shipped 2026-10-27**, uncommitted; see §5a) | headless soak harness at r12–24 with (a) a param-gated, booked energy input (default off, parity unchanged), (b) emergence metrics (surface σ, enclosed air, water/ice/vapor partition, sediment area), (c) dumps viewable with the existing cross-section/lit cut faces | at r16, 100k ticks: books drift 0 every tick; parity tests unchanged with the param off; relief σ and a nonzero water cycle still changing at 100k ticks (no heat death); measurably different worlds from different budgets with identical genesis code. **Met:** (a) + drift 0 / parity / water cycle at 100k for r12 and r16. **Open:** relief σ is roughly flat late (erosion ≈ 0), budget-sweep comparison and cut-face dumps (c) not done |
| **M2 sparse-active domain** | brick store + active set + dormancy proofs, dense path kept as oracle | bit-identical fingerprints vs dense at r12/r20 for 10k ticks (rocky/ocean/ash); after quiescence cost ∝ active bricks (target: <5% of dense at the soak's tick 12k state); books computed from partials == dense books |
| **M3 multi-region + atmosphere volume** | atmosphere shell cells, spatial vapor/clouds, regional genesis, cross-brick claims | clouds visible as chem cells in play; vapor ledger == Σ cells; r48–64 world soaked headless to quasi-steady state in < 10 min native; play window streams a region of it |
| **M4 MC-scale occupancy with local chem** | 10⁷–10⁸ occupied cells, chem active only in pockets around change/player/weather; worker-threaded | 15 Hz chem tick with ≤4 ms/frame main-thread budget in browser around the player; total memory ∝ non-uniform bricks; global books exact |

## 5a. M1 shipped: booked energy input (2026-10-27, uncommitted)

**Mechanism** (`crates/core/src/chem/p6.rs`, `chem.rs`):
- `P6Params::energy_in: EnergyInput { sun: i32, day: u32, core: i32 }`. Default / `planet6_defaults` =
  `EnergyInput::OFF` (all 0) → the pass is skipped entirely, so every existing planet6 test and golden is untouched.
  Preset `EnergyInput::M1 = { sun: 16, day: 600, core: 40 }` (600-tick day = 40 s at 15 Hz).
- New first pass of `step()` / `step_profiled()` (before heat), only when `!energy_in.is_off()`:
  - **Sun**: integer triangle-wave direction `s = (sx, sy) ∈ [-1024,1024]²`, rotating about z once per `day`
    ticks (`day == 0` → fixed zenith). Every space-facing face d of a domain cell (no domain neighbour in that
    NB6 direction; precomputed in `Topology::space_faces`) gets `sun * max(0, d·s) / 1024` Q. No shadowing;
    the night side gets nothing.
  - **Core heat**: `core` Q/tick into the domain cell nearest the bounding-box centre (`Topology::core`, lowest
    index on ties) — a mantle-heat source that diffuses out by the existing heat law.
  - Deposits use `checked_add`; a deposit that would overflow i32 is skipped and not booked.
  - The tick's total goes into `ChemWorld::input_q` (new i64 ledger, accessor `input_q()`) and
    `StepReport::injected`.
- **Energy book**: `energy_total = ΣQ − ΣBIND(non-air) + space_q + vapor·(LATENT_PAIR − BIND[WATER]) − input_q`.
  The invariant `energy_total == t0_energy` therefore says `E_world(t) = E_world(0) + input_q − space_q`
  exactly: every injected quantum is booked in, every leaked one is booked out. Lane books are unchanged
  (the input is heat only, no atoms).
- The existing surface leak (`leak_q(3) + Q·2/1000` per cell to `space_q`) is the sink, so the world reaches a
  driven quasi-steady state instead of heat death (r12: q_sum ≈ 2.87M with M1 vs 0.67M off at 100k).

**Tests** (`crates/core/src/chem/tests_p6.rs`, +3): `m1_energy_input_off_by_default`, `m1_sun_dir_is_a_day`,
`m1_energy_input_is_booked_every_tick` (80 ticks, every law + trimmed net, books/drift 0 every tick,
`input_q == Σ injected`, day and night ticks both seen, fingerprint differs from the off run).
- `cargo test --release -p bitblock-core chem` → 47 passed, 3 ignored (baseline 44 + 3 new);
  `-- --include-ignored` → 50 passed incl. `p6_golden_c_full_table`, `p6_golden_g_full_table_every_law`.
- `cargo test --release -p bitblock-wasm` → 72 passed (= baseline); full `cargo test --release -p bitblock-core` → 217 passed, 3 ignored. Play/wasm not changed and
  not rebuilt (play uses default params, energy off); www wasm still md5 `6aa17b7930de48496a93d7cf62c0a385`.

**Proof run** (`/workspace/scale_measure`, outputs in `m1/`):
```
cd /workspace/scale_measure && cargo build --release --bin chem_soak
target/release/chem_soak 12 ocean 100000 5000 off   # or m1, or sun=S,day=D,core=K
target/release/chem_soak 12 ocean 100000 5000 m1
```
`check_books()` runs every tick; each 5k window prints `W ... window_mat_changed laws[...] injected
book_fail_total max_abs_driftE`, and the run ends with `END ... driftL=[..]`.

| 100k ticks, seed 1 | r12 rocky off | r12 rocky M1 | r12 ocean off | r12 ocean M1 | r16 rocky M1 | r16 ocean M1 |
|---|---|---|---|---|---|---|
| book failures / max abs driftE / driftL | 0 / 0 / all 0 | 0 / 0 / all 0 | 0 / 0 / all 0 | 0 / 0 / all 0 | 0 / 0 / all 0 | 0 / 0 / all 0 |
| last window (95k–100k): froze / melted | 0 / 0 | 88 / 81 | 0 / 0 | 2,332 / 2,427 | 169 / 165 | 4,135 / 4,327 |
| last window: evaporated / rained | 0 / 0 | 16 / 17 | 0 / 0 | 2,999 / 3,024 | 88 / 88 | 3,016 / 3,024 |
| last window: eroded / dissolved / settled | 0 | 0 | 0 | 0 (2/1/4 in 90–95k) | 0 | 1/0/0 (33/2/19 in 90–95k) |
| last window: flowed | 0 | 0 | 0 | 447,776 | 2,376 | 1,060,024 |
| last window: cells changing material | 0 | 7 | 0 | 208 | 13 | 270 |
| cumulative evaporated / rained | 105 / 42 | 531 / 407 | 2,181 / 1,848 | 61,557 / 60,048 | 2,592 / 2,284 | 63,515 / 60,048 |
| vapor at 100k | 63 | 124 | 333 | 1,509 | 308 | 3,467 |
| surface σ at 100k | 0.720 | 0.753 | 0.975 | 1.251 | 1.142 | 1.564 |
| enclosed air | 17 | 19 | 47 | 606 | 57 | 2,069 |
| cells ever changed material | 26.06% | 26.31% | 32.74% | 39.90% | 27.28% | 32.55% |
| input_q / space_q | 0 / 9.45M | 334.8M / 342.1M | 0 / 7.58M | 334.8M / 340.2M | 601.9M / 618.7M | 601.9M / 614.1M |
| ms/step (native) | 1.14 | 1.26 | 1.29 | 1.38 | 2.93 | 3.02 |

Off runs are fully static from the 10k–15k window on (every window from 15k to 100k is all 0). With M1 every window to
100k has non-zero freeze/thaw and evaporation/rain, in both presets and at both radii. Energy overhead is
~5–10% per step.

**Residual limits (feed M2/M3):** (first, second and 16-bit-repeat items addressed in Driven mode, §5b)
- Rain is calendar-capped: 3,024 rained per 5k window is exactly the scripted rain-window budget, so the
  water cycle is driven but the rate is still set by `SEASON=240` storm/rain clocks, not by vapor.
- Rain lands at the lowest-index AIR cell next to WATER, so the same cells recycle; `ever_mat_changed`
  plateaus (ocean ~40%). Rocky has little exposed water, so its vapor is mostly stranded.
- Terrain relief is roughly flat late: erosion/dissolve/settle fire only a few times per window (ocean) or
  never (rocky has no exposed rock left after sediment covers it). The huge `flowed` counts are water
  sloshing, not new landforms. The "relief σ still changing" part of the M1 exit is **not** met yet; the
  water-cycle part is.
- The sun has no shadowing or albedo and the day axis is fixed (z); no seasons from tilt.
- The 16-bit roll tick key repeats after 65,536 ticks, inside a 100k soak (aliasing risk unchanged).
- Vapor is still a scalar pool: no atmosphere cells, so no clouds in space. That is **M3** (atmosphere shell
  cells + a conservative integer vapor advection law); no vapor→cell bridge was added in M1.
- Not tuned against play visuals; the M1 preset is only reachable from Rust (`params.energy_in`), not exposed
  in the play UI or wasm ABI. *(Superseded 2026-10-27: play can now opt in, see §8b.)*

## 5b. Unscripted water cycle: `WaterCycle::Driven` (2026-10-27, uncommitted; Owen pick t221s1348)

**Switch.** `P6Params.cycle: WaterCycle { Classic (default), Driven }`. `planet6_defaults()` and `Default`
are Classic, so play, wasm and every golden run the planet6 path bit-for-bit. Driven is Rust-only (no wasm ABI
field yet).

**Mechanism (Driven only; no `SEASON`, `STORM_LEN` or rain-window clock is read):**
- *Rolls.* `roll_at` takes a 64-bit time key. Classic still uses `lattice(seed^salt, key16, tick)`; Driven uses
  `wide_roll(seed^salt, coord, tick)` (splitmix64 over seed, x, y, z, full tick), so there is no 65,536-tick
  or 65,536-cell-key repeat. The net law also goes through `roll_at` (unchanged result in Classic).
- *Motion marks.* A per-tick `marks: Vec<u8>` (allocated only in Driven, zeroed each step) records where water
  actually moved: FALL (flow moved down), SIDE (sideways), RAIN (a drop landed), PHASE (freeze/melt).
  `RUN = FALL | RAIN`.
- *Evaporation* (`driven_water_pass`): every surface WATER cell with Q > 200 evaporates with odds
  `min(48 + (Q − 200), 1024) / 65,536` per tick, paying 30 Q into the vapor pool (same book terms as planet6).
  Rate follows heat, not `tick>>3`.
- *Rain*: drops per tick = `vapor / 512` plus one with probability `(vapor mod 512)/512` (rolled), capped by
  vapor and 64. Rain falls on sky-open AIR resting on a non-AIR cell (fallback: AIR beside WATER/ICE),
  weighted `1 + min(max(256 − Q, 0), 240) >> 4` (cold air gets up to 16×), chosen by a 64-bit hash of
  (drop, tick) over the cumulative weights with up to 4 probes. A drop: AIR→WATER, Q += 30, vapor −= 1.
  Rate follows the vapor inventory; placement follows surface state and temperature, not index order.
- *Erosion* (`driven_erode_pass`): ROCK/MIN → SED at 40‰ per tick beside RUN water, 20‰ beside a PHASE cell
  (frost), 1‰ beside any other water. Same BIND, so books are untouched.
- *Dissolve* (`driven_dissolve_pass`): the planet6 claim law, but only for a cell touching RUN water (60‰) or
  SIDE water (4‰).
- *Entrain* (`entrain_pass`, new, gated by `params.settle`): SED swaps with a marked WATER neighbour at
  y ≤ its own (120‰ RUN, 8‰ SIDE), carrying sediment downhill with moving water. Pure material swap.
- Driven order after combine2: water → flow → erode → dissolve → entrain → settle (flow first so its marks
  drive the surface laws in the same tick). Freeze/melt stays the planet6 heat-gated phase law, now fed by the
  rain/evaporation heat.

**Tests** (`chem/tests_p6.rs`, 4 new): `driven_cycle_is_off_by_default`,
`driven_wide_roll_has_no_16bit_repeat`, `driven_rain_follows_vapor_not_the_calendar_and_spreads` (24×24 dry
rock plate with 4,000 vapor: classic rains 0, driven rains outside the planet6 rain window, lands on >100
distinct cells, fills all 36 cold-corner cells first, books exact every tick),
`driven_running_water_erodes_dissolves_entrains_and_conserves` (mineral fixture + M1 for 240 ticks:
eroded 860, dissolved 700, entrained 1,607, evaporated 7,105, rained 1,456, erosion outside the storm window,
books exact every tick). `cargo test --release -p bitblock-core chem -- --include-ignored`: 54 passed
(all goldens A–G and M unchanged); whole core crate 224 passed; `bitblock-wasm` 72 passed.

**Soak** (`chem_soak 12 <preset> 100000 5000 m1 classic|driven`, books checked every tick; logs in
`/workspace/scale_measure/unscript/`). All runs: `book_fail_total=0`, `max_abs_driftE=0`, `driftL` all 0.
The classic+M1 runs reproduce the §5a M1 numbers exactly (harness parity).

| last 5k window (95k–100k) | ocean classic+M1 | ocean driven+M1 | rocky classic+M1 | rocky driven+M1 |
|---|---:|---:|---:|---:|
| froze / melted | 2,332 / 2,427 | 974 / 975 | 88 / 81 | 0 / 0 |
| evaporated / rained | 2,999 / 3,024 (clock cap) | 14,027 / 13,936 | 16 / 17 | 958 / 955 |
| distinct rain cells (footprint) | 270 | 534 | 3 | 59 |
| eroded / dissolved / settled / entrained | 0 / 0 / 0 / 0 | 2 / 0 / 11 / 166 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 280 |
| cells changing material | 208 | 184 | 7 | 22 |
| surface σ at 100k | 1.251 | 1.210 | 0.753 | 0.375 |
| ever changed material | 39.90% | 44.16% | 26.31% | 6.04% |
| vapor at 100k | 1,509 | 1,480 | 124 | 114 |
| ms/step (window) | 1.37 | 1.50 | 1.24 | 1.27 |

Surface σ by tick (ocean): classic 1.234 at 5k then flat 1.237–1.252 to 100k; driven 0.939 (5k) → 1.039
(10k) → 1.142 (20k) → 1.22 (40k), then moving between 1.17 and 1.22 to 100k. Rocky: classic 0.783 → 0.753
flat from 10k; driven 0.333 → 0.375 by 25k, then 0.373–0.382.
Off+driven reference (no energy input): both presets go fully static by ~15k, as in §5a (heat death).

**What this meets.** The water cycle is no longer calendar-scripted: rain is 4.6× the old 3,024 cap in ocean
and 56× classic in rocky, and tracks vapor (it equals evaporation within 1%). Rain spreads over 2× (ocean) and
20× (rocky) as many distinct cells as classic. Freeze/thaw keeps cycling in ocean. Late windows have
non-zero erosion, settle and entrainment in ocean, and entrainment in rocky (classic: all zero). No 16-bit
roll repeat in Driven.

**What it does not meet (honest).** Relief does not keep rising. Late erosion is near zero, and σ is not above
the M1 classic baseline (ocean 1.21 vs 1.25; rocky 0.375 vs 0.753). Why, from the counts:
1. Classic gets most of its σ in the first 5k ticks from scripted storms that erode *every* exposed rock face,
   wet or dry (rocky: 1,685 eroded in 0–5k with only 110 rained). Driven only erodes where water moves, and
   rocky has ~110 vapor and a 59-cell rain footprint, so most rock never weathers.
2. Sediment armouring: rock along the runoff paths turns to SED in the first 10–20k ticks (ocean erosion
   286 → 42 per window by 20k, then ≤ 7). The SED covers the rock below, and entrainment only moves SED
   downhill within water. Nothing exposes fresh rock again, and there is no uplift.
3. Rocky has no freeze/thaw late (0/0), so frost weathering never triggers there.

**Minimal follow-up** (still Driven-only, books unchanged): (a) a dry thermal-weathering law, ROCK/MIN facing AIR
weathers at a small rate scaled by its day/night Q swing (sun-driven, replacing the dry part of the old
storm); (b) let fast RUN water entrain the SED that covers rock (scour), so armour can be stripped;
(c) a source of relief: core-heat uplift or isostasy, a slow rise of rock under thick sediment, so that
erosion has something to work on. (c) is the real long-term σ driver and belongs with M2/M3.

## 5c. Driven relief laws: dry weathering, scour, core-heat uplift (2026-10-27, uncommitted; Owen pick t221s1355)

Driven-only (`WaterCycle::Driven`); Classic code path untouched (goldens A–G, M unchanged; a 100k classic+M1
ocean soak reproduces `unscript/` bit-for-bit, relief counters 0). No `P6Params` field and no wasm/play ABI
change: the laws reuse the existing flags. New `StepReport` counters `weathered`, `scoured`, `uplifted`.
Driven order: water → flow → erode → dissolve → entrain → **scour → weather → uplift** → settle.
All three are book-neutral by construction: ROCK, MIN and SED have identical COMP and BIND, and every move is
a material permutation (lanes and BIND unchanged; Q stays with its cell, so the energy book is untouched).

- *Dry weathering* (`weather_pass`, gated by `params.erode`): ROCK/MIN with any face on AIR or space → SED, no
  water needed, odds `(WEATHER_AIR + WEATHER_SUN × lit × sun/16) / 65,536` per tick, `WEATHER_AIR = 2`,
  `WEATHER_SUN = 3`, `lit` = space faces on the sun's day track (−x, +x, +y; the sun turns in the x–y plane,
  so those faces get the day/night heat swing). Covered rock (incl. by its own SED) stops: self-limiting.
- *Scour* (`scour_pass`, gated by `params.settle`): SED lying directly on ROCK/MIN with a moving WATER
  neighbour (any face) swaps one cell sideways into a fluid (AIR/WATER) side cell, preferring a side whose
  floor is fluid (a drop, so the grain settles downhill); side order rotated by the roll. Odds 250‰ beside
  RUN water (fall/rain), 20‰ beside SIDE (slosh). Dest mark cleared (spent). Bares the rock under runoff.
- *Core-heat uplift* (`uplift_pass`, gated by `params.settle && energy_in.core > 0`): a convection roll. A
  candidate is a column top `t` (rock-like = not AIR/WATER/rising gas, with AIR/WATER `a` above) whose
  `UPLIFT_DEPTH = 4` cells down to the root `h` are rock-like. A roll picks a side; `h'` = side of `h` must be
  rock-like; the neighbour column is walked up to its top `n`. If `dQ = Q[h] − Q[h'] > 0` it fires with odds
  `min(dQ, 400) × 4096 / 2³²`, if the risen top would stand ≤ `UPLIFT_MAX_STEP = 3` above the lowered one;
  ≤ 1 roll per tick. Firing rotates materials one step along `n → … → h' → h → … → t → a → n`: the hot
  column rises one cell (its top moves into `a`), the cooler neighbour sinks one cell, and `a`'s fluid
  fills `n`. Heat is not moved. Mechanism note: this is a heat-gradient-driven permutation, not a physical
  buoyancy/density law and not isostasy; the driver is the M1 core input keeping deep Q contrasts alive.

**Tests** (3 new in `chem/tests_p6.rs`, on an 8×8 rock plate with hot roots on x < 4): dry weathering fires
with no water while classic gives 0/0/0 (`driven_dry_rock_weathers_without_water_and_classic_does_not`);
SED is scoured off rock under a flowing water heap (`driven_moving_water_scours_sediment_off_rock`); hot
columns lift, rock-family count is preserved, and no uplift with `core = 0`
(`driven_core_heat_lifts_hot_columns_by_permutation`). Books, energy drift and lane drift checked every tick.
`cargo test --release -p bitblock-core chem -- --include-ignored`: 57 passed. Core crate 227 passed (3
ignored); `bitblock-wasm` 72 passed. (`bitblock-research` display_store has 13 pre-existing
`UnsupportedExclusivePlatform` failures on this box, unrelated.)

**Soak** (`chem_soak 12 <preset> 100000 5000 m1 driven`, logs `/workspace/scale_measure/relief/`). Both:
`book_fail_total=0`, `max_abs_driftE=0`, `driftL` all 0, every tick.

| last 5k window (95k–100k) | ocean driven (prior) | ocean driven + relief | rocky driven (prior) | rocky driven + relief |
|---|---:|---:|---:|---:|
| weathered / scoured / uplifted | – | 7 / 8 / 14 | – | 12 / 33 / 1 |
| eroded / dissolved / settled / entrained | 2 / 0 / 11 / 166 | 5 / 2 / 28 / 82 | 0 / 0 / 0 / 280 | 2 / 0 / 11 / 13 |
| cells changing material | 184 | 234 | 22 | 41 |
| surface σ at 100k | 1.210 | 1.390 | 0.375 | 0.711 |
| ever changed material | 44.16% | 57.92% | 6.04% | 25.86% |
| bare rock at 100k | – | 123 | – | 96 |
| ms/step (window, 2 soaks in parallel) | 1.50 | 1.77 | 1.27 | 1.70 |

Surface σ by tick (prior → relief): ocean 5k 0.939 → 1.018, 10k 1.039 → 1.136, 20k 1.142 → 1.176,
40k 1.221 → 1.305, 60k 1.195 → 1.357, 80k 1.170 → 1.363, 95k 1.183 → 1.370, 100k 1.210 → 1.390.
Rocky 5k 0.333 → 0.338, 10k 0.352 → 0.369, 20k 0.360 → 0.407, 40k 0.375 → 0.507, 60k 0.373 → 0.573,
80k 0.382 → 0.644, 95k 0.373 → 0.700, 100k 0.375 → 0.711. Prior driven σ was flat after ~40k (ocean) /
~25k (rocky); with the relief laws it still rises at 100k in both, ocean now above classic+M1 (1.251) and
rocky approaching it (0.753). Relief counters per window stay non-zero throughout: ocean uplift 10–24,
weathering 7–21, scour 1,095 at 5k decaying to 8–20 late; rocky weathering 252 → 12–24, scour 17–203
(33–99 late), uplift 1–13.

**Honest limits.** Weathering and scour both decay with time (bare rock ocean 226 → 123, rocky 829 → 96) as
SED blankets the open rock; rocky uplift is down to 1 per window late (contrasts shrink as Q flattens). σ is
still climbing at 100k, so this is "still moving", not yet a steady relief balance; a longer soak (300k+) is
needed to see where it saturates. Cost +18% (ocean) / +34% (rocky) per step, mostly the full-domain scans of
weather/uplift (cheap to restrict to surface cells later, M2 active sets).

## 5d. 300k saturation soak (2026-10-27; Owen pick t221s1362; measurement only, no code changes)

`chem_soak 12 {ocean,rocky} 300000 5000 m1 driven` (+ `m1 classic` ocean control), seed 1, books every tick. Logs + per-window CSVs: `/workspace/scale_measure/soak300k/` (`extract.py` regenerates `*_sigma.csv`).

**Headline: σ saturates.** Ocean driven plateaus at ~1.38 from ~90k (mean/sd per 50k block: 1.380±.009, 1.372±.012, 1.383±.014, 1.392±.008); peak 1.406 @235k. Rocky driven climbs until ~210k then flattens: 0.750, 0.782, 0.806±.006, 0.808±.005; peak 0.818 @290k. 250k–300k is flat within noise in both (block-to-block change ≤ 1 sd). Classic ocean control is flat at 1.249±.005 the whole run (reached in first 5k).

| tick | ocean driven | rocky driven | ocean classic |
|---|---|---|---|
| 25k | 1.260 | 0.441 | 1.244 |
| 50k | 1.304 | 0.552 | 1.238 |
| 100k | 1.390 | 0.711 | 1.251 |
| 150k | 1.372 | 0.761 | 1.254 |
| 200k | 1.356 | 0.790 | 1.248 |
| 250k | 1.401 | 0.806 | 1.253 |
| 300k | 1.400 | 0.814 | 1.242 |

Late (last 10 windows, mean per 5k): ocean weathered/scoured/uplifted 0.6/1.7/11.9, bare rock 52; rocky 4.8/10.3/2.6, bare rock 28. Relief is a dynamic balance of uplift vs scour/settle on a sediment-armoured surface, not frozen. Books: book_fail_total=0, max |driftE|=0, driftL all 0 in all three runs to 300k. ms/step ~1.6–1.7 driven, ~1.45 classic when uncontended (three runs in parallel inflated means to 2.3–2.5). No harness issues.

Implication: at r12 Driven relief saturates at σ≈1.39 (ocean, +11% over classic) and ≈0.81 (rocky, ~2.2× prior driven). Further relief needs a stronger/longer-range driver (e.g. larger radius or uplift tuning), not more time.

## 5e. Longer-range relief driver: deep convective overturn (`plumed`) (2026-10-27, uncommitted; Owen pick t221s1380)

**What σ actually measures at r12.** The domain is the full r12 ball (no space above the surface), so `surf_r_std`
is the radial spread of exposed cells = sphere boundary (always r≈11–12) + faces on interior AIR pockets. Exposed-cell
radius histograms (`plume_probe`, `probe_hist_*`) show the prior plateau is set by how deep air pockets reach:
baseline air-exposed faces stop at r≥7 (ocean) / r≥8 (rocky). Shallow column uplift only shuffles voids at the same
depth, which is why it saturated.

**Trials that failed** (100k each, books exact): a hot-root plume (8-deep column rises, far column sinks through a
2–6 cell root channel, fluid parked on the far top) at K 16k–262k, slope guards on/off, 1–8 per tick: ocean ≤1.40–1.46
transient, rocky 0.675–0.688 (below baseline 0.711). Firing rate was supply/guard-limited (~50–130 per 5k regardless of K).

**Shipped law (`plume_pass`, after uplift, Driven + `core>0` only).** A cold column top `t` (rock-like, surface fluid
`a` above) with a rock-like column 8 cells down to root `h`; a roll picks a side and reach L∈2..6; the L root-level
cells must be rock-like, ending at hot root `h'`, whose column is walked up to its top `n` and the fluid slot `f` above.
With dQ = Q[h'] − Q[h] > 0 it fires at min(dQ, 400) × 262,144 / 2³², if (n.y+1) − t.y ≤ 2L, ≤ 2 per tick. Rotation
`a → h → channel → h' → … → n → f → a`: the cold root takes in surface fluid (subduction-flavoured volatile
drawdown), the deep channel flows one cell toward the hot side, the hot column rises one cell, its displaced fluid
refills the cold top. Pure permutation: Q stays with cells, lanes/BIND/Q/occupancy untouched.
Constants (`p6_rest.rs`): `PLUME_DEPTH=8`, `PLUME_REACH_MIN/MAX=2/6`, `PLUME_K=262144`, `PLUME_DQ_MAX=400`,
`PLUME_SLOPE=2`, `PLUME_MAX_PER_TICK=2`, `SALT_PLUME=0x91a3e`. Counter `StepReport.plumed`; `chem_soak` window
counters end `…uplifted,plumed`.

**300k soak** (`chem_soak 12 {ocean,rocky} 300000 5000 m1 driven`, seed 1; logs `/workspace/scale_measure/relief_driver/`):
book_fail_total=0, max|driftE|=0, driftL all 0, every tick, both runs. ms/step ~1.74 / 1.68 (≈ +2–5% vs soak300k).

| σ | ocean prior | ocean plume | rocky prior | rocky plume |
|---|---|---|---|---|
| 25k | 1.260 | 1.618 | 0.441 | 0.850 |
| 50k | 1.304 | 1.649 | 0.552 | 1.025 |
| 100k | 1.390 | 1.680 | 0.711 | 1.253 |
| 150k | 1.372 | 1.619 | 0.761 | 1.184 |
| 200k | 1.356 | 1.597 | 0.790 | 1.197 |
| 250k | 1.401 | 1.574 | 0.806 | 1.388 |
| 300k | 1.400 | 1.462 | 0.814 | 1.309 |
| block 50–100k (mean±sd) | 1.364±.015 | 1.667±.020 | 0.638±.049 | 1.182±.059 |
| block 100–150k | 1.380±.009 | 1.657±.027 | 0.750±.007 | 1.199±.045 |
| block 150–200k | 1.372±.012 | 1.640±.031 | 0.782±.009 | 1.242±.043 |
| block 200–250k | 1.383±.014 | 1.559±.034 | 0.806±.006 | 1.240±.082 |
| block 250–300k | 1.392±.008 | 1.496±.030 | 0.808±.005 | 1.397±.065 |

Late mean per 5k (last 10 windows): ocean weathered/scoured/uplifted/plumed
0.5/0.4/9.5/102, rocky 8.7/11.6/2.6/7.2.

**Verdict.** Rocky: materially and durably above the prior plateau (+0.43–0.59 σ, ~1.5–1.7×, 250–300k block 1.40 vs
0.81) and not obviously saturated yet. Ocean: well above at 50–200k (+0.27–0.29, ~20 sd) but **decaying after
~200k** (250–300k block 1.50, +0.10 / +7.5% over 1.39, trend still down). The decay tracks air loss: air cells
668 → 480 and air_enclosed ~390 → 277 while water rises 1136 → 1259 (fluid drawn down stays as water at depth),
so fewer interior interfaces. Honest caveat: the σ gain is mostly **deeper voids** (air-exposed faces now reach
r=4–5), i.e. caves/porosity under the surface plus hot-side uplift, not taller outer mountains (boundary cells stay
r 11–12; the ball has no headroom).

## 5f. Hydrothermal degassing (`degassed`) (2026-10-27, uncommitted)

**Why air fell under the plume law.** In the ocean world water+air+ice cells are constant (1977), and air ≈ vapor
pool − cumulative gypsum dissolved (dissolution turns AIR→WATER). Plume drawdown leaves more water submerged
(up-neighbour WATER), which the Driven evaporation gate cannot touch, so total evaporation per 5k fell (~11.1k vs
~13.7k pre-plume), the vapor steady state (≈ evap rate × RAIN_TAU 512) shrank, and gypsum kept dissolving: air
668 → 480 over 100–300k. `water_probe` (ocean plume 200k): ~600–720 submerged water cells, ~80% with Q>200,
median Q ~400–480, 515–695 "sealed hot".

**Law (`degas_pass`, after `plume_pass`, before settle; Driven + `rain` + `core>0` only).** A WATER cell with
Q > `DEGAS_Q`, whose up-neighbour is WATER (submerged, so the surface evaporation path cannot reach it) and whose
down-neighbour is in-domain and rock-like (heated bed), fires with odds `min((Q−DEGAS_Q)·DEGAS_SLOPE, DEGAS_MAX)/65536`
per tick on `wide_roll(seed^SALT_DEGAS, cell, tick)`, at most `DEGAS_MAX_PER_TICK` per tick. Firing = the same
bookkeeping as evaporation: WATER→AIR, `vapor += 1`, `Q −= LATENT_PAIR` (Q > 400 > 30, never clips). The resulting
AIR bubble rises through the column in `flow_pass` (bottom-up scan) and the vapor returns as normal driven rain.
Counter `StepReport.degassed` (not added to `evaporated`); `chem_soak` window counters end `…plumed,degassed`.
Constants (`p6_rest.rs`): `DEGAS_Q=400`, `DEGAS_SLOPE=1`, `DEGAS_MAX=512`, `DEGAS_MAX_PER_TICK=2`, `SALT_DEGAS=0xde6a5ed`.

**Tests** (`chem/tests_p6.rs`): `driven_hot_submerged_water_degasses_to_vapor_with_exact_books` (8×8 plate, hot pool on
a rock bed, 600 ticks: books/energy/lane drift 0 every tick, water+ice+vapor and air+water+ice constant, rock family
constant, degassed 66 > 0, ≤2 per tick; core=0 and Classic → 0 over 300 ticks) and `degas_needs_a_rock_bed` (pool on
domain floor → 0 degassed, evaporation still runs). `cargo test --release -p bitblock-core chem -- --include-ignored`
60/60 (Classic goldens incl. `p6_golden_f/g` unchanged); full `-p bitblock-core -p bitblock-wasm` 227+74 passed, 0 failed.

**300k soak** (`chem_soak 12 {ocean,rocky} 300000 5000 m1 driven`, seed 1; logs
`/workspace/scale_measure/relief_driver/degas/`): book_fail_total=0, max|driftE|=0, driftL all 0, both runs;
ms/step ~1.68 / 1.61 (no measurable cost).

| | ocean plume | ocean +degas | rocky plume | rocky +degas |
|---|---|---|---|---|
| σ 100k | 1.680 | 1.638 | 1.253 | 1.156 |
| σ 200k | 1.597 | 1.699 | 1.197 | 1.229 |
| σ 300k | 1.462 | 1.647 | 1.309 | 1.254 |
| block 0–50k σ | 1.594±.059 | 1.545±.042 | 0.872±.076 | 0.828±.105 |
| block 50–100k | 1.667±.020 | 1.666±.023 | 1.182±.059 | 1.056±.071 |
| block 100–150k | 1.657±.027 | 1.634±.020 | 1.199±.045 | 1.180±.021 |
| block 150–200k | 1.640±.031 | 1.662±.021 | 1.241±.043 | 1.165±.056 |
| block 200–250k | 1.559±.034 | 1.649±.019 | 1.240±.082 | 1.235±.035 |
| block 250–300k | 1.496±.030 | **1.602±.027** | 1.397±.065 | 1.207±.052 |
| vapor 250–300k mean | 1139 | 1192 | 98 | 86 |
| air_enclosed 250–300k mean | 303 | 324 | 61 | 60 |
| air / water / vapor @300k | 480 / 1259 / 1116 | 491 / 1248 / 1155 | — / — / 90 | — / — / 87 |
| gypsum @300k | 248 | 220 | 126 | ~130 |
| total degassed / evap (300k) | – / 672.6k | 83.5k / 627.7k | – | 134 / 17.5k |

**Verdict.** Ocean: the late decay is removed — σ holds 1.60–1.66 through 300k (250–300k block +0.106 ≈ 3.5 sd over
plume-only; 200–250k +0.09), vapor pool +~50, air_enclosed +~20. Degassing (~0.28/tick, declining 2.0k→1.2k per 5k as
deep water cools) partly *displaces* surface evaporation (−45k) — net volatile return +38.6k (+5.7%). Air still
drifts down (798 → 491) because the gypsum sink is faster with more air (gypsum 220 vs 248): degassing slows but
does not stop drawdown; air at 300k is ~equal to plume-only, it is the interior void count/depth that holds. Early
σ (0–50k) is ~0.05 lower (bubbles first vent at shallow pool tops). Rocky: law is effectively silent (134 firings in
300k — only ~16 water cells); the rocky σ differences are trajectory divergence from those few rolls, not a law
effect (single seed). Caveat: one seed each; scan order biases which cells hit the 2/tick cap (low index first) — cap
rarely binds at this rate.

## 5g. Sky shell: AIR headroom above the ball, outer-relief σ (2026-10-27, uncommitted)

**Why.** r12 genesis fills the whole ball (air 0 at t0) and everything outside it is space (`None`), so no cell can
ever stand above the genesis surface: the legacy soak σ (`surf_r_std`, radial spread of non-air cells with a
space/AIR face) could only grow by digging *down* — it measured void/cave depth, not mountains. The plume law (§5e)
also needs an in-domain fluid slot above a hot top, so it could never fire at the outer surface.

**Mechanism (opt-in, default off).** `ChemWorld::add_sky_shell(&GenesisShape, H) -> cells_added` (and
`genesis_sky(cfg, params, H)` = `genesis_p6` + shell) adds ordinary in-domain **AIR cells with Q = 0** above the ball:
for every footprint column (dx²+dz² ≤ R²) the H cells directly over its top ball cell (top = isqrt(R²−ρ²)), skipping
existing cells. Uniform vertical headroom; the lower hemisphere and the rim keep their space boundary (nothing can
fall off). AIR with Q=0 is book-neutral (no BIND, no lanes) so t0 energy/lanes/`t0_energy` are bit-identical; the
shell then conducts and leaks like any AIR cell (no new heat code — design choice: real conducting air, not vacuum).
Sun (M1) lands on the shell's space faces (top of atmosphere); core input is **pinned** to the pre-shell core cell
(`core_pin`, `Topology.core`), so the lopsided-in-y shell does not move the hotspot. One law edit, gated on
`sky_height > 0`: `weather_pass` counts a sun-track face (±x, +y) on a **sky-open** AIR cell (all-AIR column up to
space, computed per tick top-down over `topo.rise`) as lit, exactly as a space face is lit unpadded. Without a shell
nothing changes (unpadded Driven control below reproduces the §5f 300k numbers to the last digit).
r12: H=8 → +3,528 cells (10,681 total), H=16 → +7,056 (14,209).

**New metric (outer relief).** `ChemWorld::land_relief(&shape) -> LandRelief`: per footprint column, the top "land"
cell (not AIR/WATER/ICE/rising gas — so the seafloor counts as land under an ocean) at height
**h = dy_land − top(ρ)** relative to the genesis surface (datum). `relief_h_std` = **σ_h** (std of h over columns with
land — the outer-relief σ), `relief_h_mean`, `relief_h_max/min`, `above_datum` (land cells standing above the datum),
`land_r_std` (σ_R, radial std of the top land cell), `no_land`. `chem_soak` arg 7 `sky=H` and prints these plus
`air_open/air_buried` (AIR cells sky-open vs under something). The legacy `surf_r_std` is still printed but with a
shell it also counts the shell's lower envelope and becomes meaningless as relief; compare `relief_h_std` across runs.
Probe: `src/bin/sky_probe.rs` (height profile, histogram, ceiling columns, dome split).

**Tests** (`chem/tests_p6.rs`, all pass): `sky_shell_keeps_t0_books_cells_and_core` (energy/t0/lanes identical, every
old cell unchanged, added = footprint×H all AIR Q=0 above the column top, core coord pinned, t0 above_datum = 0, probe
agrees with/without shell), `sky_zero_is_a_noop` (H=0 adds 0 cells, fingerprint identical after 20 ticks),
`sky_shell_driven_m1_books_exact_every_tick` (r10, H=6, Driven+M1+net, 400 ticks: check_books + energy/lane drift 0
every tick), `sky_shell_classic_books_exact_and_unpadded_classic_unchanged` (Classic + H=4, 120 ticks exact).
`cargo test --release -p bitblock-core chem -- --include-ignored` **64/64** (Classic goldens incl. `p6_golden_f/g`
unchanged); full `-p bitblock-core -p bitblock-wasm` **231 + 74** passed, 0 failed, 3 ignored.

**300k soaks** (`chem_soak 12 {ocean,rocky} 300000 5000 m1 driven sky={0,8,16}`, seed 1; logs
`/workspace/scale_measure/sky_shell/`, summary `blocks_summary.txt`): **book_fail_total=0, max|driftE|=0, driftL all 0
in all six runs.** ms/step (window means, shared box): sky0 ~1.8, sky8 ~2.3–2.5, sky16 ~2.6–2.9.

| 50k block mean | ocean sky0 | ocean sky8 | ocean sky16 | rocky sky0 | rocky sky8 | rocky sky16 |
|---|---|---|---|---|---|---|
| **σ_h** 0–50k | 2.33 | 3.51 | 3.58 | 0.39 | 2.98 | 3.23 |
| σ_h 100–150k | 2.63 | 4.11 | 4.11 | 0.37 | 5.20 | 5.69 |
| **σ_h 250–300k** | **2.71±.02** | **4.51±.02** | **4.43±.04** | **0.37±.01** | **5.85±.02** | **7.07±.08** |
| mean_h 250–300k | −2.62 | −1.42 | −1.58 | −0.13 | +0.91 | +1.63 |
| max_h (cap = H) | 0 | 8 (cap from t≤5k) | 15 | 0 | 8 (cap) | 16 (cap from ~100k) |
| above_datum 250–300k | 0 | 426 | 412 | 0 | 1314 | 1533 |
| σ_R (land_r_std) 250–300k | 1.06 | 2.85 | 3.03 | 0.29 | 3.78 | 5.23 |
| legacy σ (`surf_r_std`) 250–300k | 1.602 | 3.14* | 4.52* | 1.207 | 3.12* | 4.55* |
| air_buried 250–300k | 303 | 1110 | 2341 | 80 | 913 | 1584 |
| vapor 250–300k | 1192 | 274 | 132 | 86 | 9 | 7 |
| evap total 300k | 616k | 165k | 69k | 16.5k | 4.0k | 3.1k |
| plumed total 300k | 5.4k | 7.8k | 4.7k | 333 | 12.9k | 17.4k |

*legacy σ with a shell includes the shell envelope; not a relief measure. Old (void-depth) vs new on the same
unpadded world: ocean 1.60 vs σ_h 2.71 (σ_h there is seafloor bathymetry, max_h = 0), rocky 1.21 vs σ_h **0.37** —
i.e. the old rocky "relief" of ~1.2 was almost entirely buried voids; the outer surface was flat to ±0.4 cell.

**Verdict.** The shell turns outer relief on: land rises above the genesis surface within 5k ticks and σ_h is still
climbing at 300k (rocky 0.37 → 5.85 at H=8, 7.07 at H=16; ocean 2.71 → ~4.5). The plume law now fires at the outer
surface (rocky 333 → 12.9k–17.4k firings). Books exact throughout. But read the caveats — the relief is real chem
output, not yet good terrain:
1. **Hotspot dome capped by headroom.** M1 core input is a single cell; with headroom, uplift+plume stack a dome
   (ρ≤4) over it to the ceiling (`sky_probe` 100k: rocky H=8 dome mean h 7.86±0.45 = pinned at cap; H=16 dome 10.4,
   2 columns at ceiling; ocean H=8 dome 5.3). Outside the dome relief is broad too (rocky ρ>4 σ_h 4.45 at H=8, 3.88 at
   H=16 at 100k; ocean ρ>4 2.9 vs 2.6 unpadded), so σ_h is not only the dome — but max_h hitting H means H must grow
   (or the dome needs a limiter) before σ_h is cap-free.
2. **Foamed crust.** Matter only permutes, so land rises by drawing shell AIR down into the crust (air_buried
   80 → 913 rocky; land fraction at the datum layer 0.88 → 0.54). Raised land is porous, not compacted rock; no law
   collapses voids or moves unsupported rock (only SED settles), so overhangs persist.
3. **Sun is now top-of-atmosphere.** Sun heat must diffuse down H air cells and leaks back from the shell; ocean
   evaporation drops 616k → 165k (H=8) → 69k (H=16), vapor 1192 → 274 → 132, more ice. The water cycle is weaker
   with a shell; a transparent-air sun (deposit on the first non-AIR cell of the column) is the natural fix.
4. Ocean water can drain toward the rim (walls at ρ>R are space); not seen to dominate (ocean mean_h still −1.4).
5. Single seed; cost: cells +49% (H=8) / +99% (H=16), ms/step +~30% / +~55%; Driven rolls are coordinate-keyed so padded trajectories are
   new trajectories, not perturbations of unpadded ones. Play/wasm does not use the shell (not rebuilt).

## 5h. Core sphere: M1 core heat spread over a ball (2026-10-27, uncommitted) — negative result, dome diagnosed

**Ask:** spread the core input over a core sphere so the hotspot dome (§5g caveat 1) stops pinning max_h at H.

**Mechanism (built, opt-in).** `EnergyInput` gains `core_r: u32` (`OFF`/`M1` keep `core_r: 0`;
`M1.with_core_r(r)`, `CORE_R_SPREAD = 6`). With `core_r = 0` the original single-cell path runs unchanged. With
`core_r = r > 0` the **cell set** is every domain cell whose squared distance from `topo.core` (the pinned pre-shell
centre) is ≤ r², in domain index order (n cells; r12: r1 = 7, r3 = 123, r4 = 257, r6 = 925). **Exact split:** each
cell gets `core / n`; the `core % n` remainder units go to the cells k with `(k + tick) % n < core % n` (a rotating
window), so the tick total is exactly `core` (M1 = 40 Q/tick, unchanged) and every sphere cell receives exactly
`core` Q over any n consecutive ticks. Booked through the same `input_q` ledger with `checked_add` per cell
(a cell that would overflow is skipped and not booked). Code: `chem/p6.rs` (`core_sphere`, `core_sphere_input`,
`energy_input_pass` branch). `chem_soak` energy spec takes `core_r=N` and preset `m1rN`; `sky_probe` arg 5 = core_r and
now prints mean Q by radius, Q along +x at dy = 4/8 and the hottest cell.

**Classic / H=0 / play:** `core_r = 0` is the default everywhere, so M1, Classic goldens, H=0 and play are
bit-identical (test `core_r_zero_is_the_original_m1`). Play does not expose `core_r`; wasm not rebuilt.

**Tests** (new, in `chem/tests_p6.rs`): `core_sphere_is_the_ball_and_splits_core_heat_exactly` (set = brute-force
ball; each cell gets exactly `core` over n ticks; h6 r3/r4/r1, h0 r2), `core_sphere_books_exact_every_tick_driven_and_classic`
(Driven H6 400t, Driven H0 200t, Classic H4 120t with `M1.with_core_r(4)`: check_books + energy/lane drift 0 every
tick, total injected = single-cell run, fingerprint differs), `core_r_zero_is_the_original_m1`. `cargo test --release
-p bitblock-core chem -- --include-ignored` **67/67**; full core + wasm **234 + 74** passed, 0 failed, 3 ignored.

**Result: the dome is not reduced.** Sweep (30k, H8) dome mean h (ρ≤4): rocky cr0 7.39, cr3 7.37, cr6 7.20, cr9 7.12;
ocean 4.76, 4.88, 4.61, 4.55. 100k H8: rocky 7.86±0.45 → 7.96±0.20 (ceiling columns 61 → 72), ocean 5.35 → 5.22
(26 → 23); 100k H16: rocky 10.43 → 10.39 (2 → 2 ceiling cols), ocean 5.51 → 5.63. 300k soaks (`chem_soak 12
{ocean,rocky} 300000 5000 m1r6 driven sky={8,16}`; **book_fail_total = 0, max|driftE| = 0, driftL all 0 in all four**):

| 250–300k block mean | ocean H8 cr0 (§5g) | ocean H8 cr6 | ocean H16 cr0 | ocean H16 cr6 | rocky H8 cr0 | rocky H8 cr6 | rocky H16 cr0 | rocky H16 cr6 |
|---|---|---|---|---|---|---|---|---|
| σ_h | 4.51 | 4.51 | 4.43 | 4.33 | 5.85 | 5.74 | 7.07 | 7.31 |
| max_h (cap H) | 8 | 8 | 15 | 14 | 8 | 8 | 16 | 16 |
| mean_h | −1.42 | −1.39 | −1.58 | −1.56 | +0.91 | +0.94 | +1.63 | +1.49 |
| above_datum | 426 | 440 | 412 | 389 | 1314 | 1265 | 1533 | 1580 |
| dome mean h, 100k probe | 5.35 | 5.22 | 5.51 | 5.63 | 7.86 | 7.96 | 10.43 | 10.39 |

**Diagnosis (why spreading the source does nothing).** The core input (40 Q/tick) is tiny against the genesis
heat inventory (radial ramp 3000 at the centre → surface_q 900). Heat diffusion moves `(q·dq/6000)/live` with
dq = 300, i.e. ≈ q/120 per face, so differences under ~120 Q never move: the Q field freezes into a **radial
staircase of plateaus near multiples of 120** (probe along +x: steps at dx ≈ 5–6, ≈359→240, 477→360), a fossil of
genesis. cr6 flattens the interior only (centre ~1280 → ~240); outside r6 the staircase is unchanged. Uplift (depth 4)
and plume (depth 8, reach 2–6) compare Q of roots **at the same world y across x/z neighbours**. On a ball the inner
(smaller ρ) root at a given y is always on a hotter step, so under the pole the horizontal slice cuts across radial
isotherms and the axial columns always win: that is the dome, independent of where the core heat lands.

**Evidence: radial-frame guard (scratch only, NOT in the working tree).** In a tar copy (`/tmp/radexp`), uplift/plume
skip a pair when the hot root is strictly closer (d²) to the core centre than the cold root (30-line diff:
`/workspace/scale_measure/core_sphere/radial_guard_experiment.diff`). M1 core_r 0, books exact in all runs
(4 × 300k: book_fail 0, drift 0):

| radial guard | dome mean h 30k / 100k H8 | dome 100k H16 | σ_h 250–300k H8 / H16 | max_h H8 / H16 | above_datum H8 / H16 |
|---|---|---|---|---|---|
| rocky | 7.39 → 0.06 / 7.86 → 0.16 | 10.43 → 0.08 | 5.85 → 3.49 / 7.07 → 4.51 | 8 / 16 (rim) | 739 / 883 |
| ocean | 4.76 → −2.12 / 5.35 → −2.45 | 5.51 → −2.43 | 4.51 → 2.79 / 4.43 → 2.61 | 7 / 11 | 151 / 121 |

The guard kills the dome (ocean also comes off the ceiling) and roughly halves σ_h, showing how much of §5g σ_h was the
dome. But rocky still reaches the cap — now at the **rim** (100k H8: 25 ceiling columns at ρ ≈ 11–12, none central),
because the same horizontal-slice bias now favours the outer edge. A correct fix compares roots along the local radial
frame (or uses an overburden/gradient limit), not a d² veto, and it changes Driven (incl. H=0 play Driven), so it needs
Owen's pick.

**Verdict.** Core sphere kept as an exact, harmless opt-in (`core_r` default 0); it does not unpin the dome and is not
recommended as the fix. The pin is a frame artifact of the uplift/plume laws on a ball plus the diffusion dead band.
Logs: `/workspace/scale_measure/core_sphere/` (`tests_chem.txt`, `tests_core_wasm_full.txt`, `sweep/`,
`probe_*_100k.txt`, `soak_r12_*_m1r6_driven_sky{8,16}_300k.txt`, `blocks_summary.txt`, `radexp/`).

## 5i. Radial-frame uplift and plume (2026-10-27, uncommitted; Owen pick) — dome unpinned, rim pin mostly gone

**Problem (from §5h).** Uplift and plume compared the Q of two roots at the **same world y**. On a ball, a horizontal
slice cuts across the frozen radial heat staircase, so the more central root always reads hotter: the hotspot dome
stacked under the pole up to the shell ceiling. The scratch d² veto (radexp) killed the dome, but it moved the pin to
the **rim**. There, the sky shell is sun-warmed and, on the flank, a vertical column runs *along* the surface instead
of down into it.

**Mechanism (Driven only; `crates/core/src/chem/p6_rest.rs`).** Two helpers gate both `uplift_pass` and `plume_pass`
after the hot root and its lateral neighbour column are picked. The roll, `dq` scaling, caps and the permutation move
are unchanged. Classic never calls either pass, so Classic is untouched.
- `RADIAL_PEER_SPAN = 8` (new constant).
- `radial_peer(topo, o, hot, col)`: in the cold column (same x, z as `col`), find the cell whose d² from the core `o`
  is nearest the hot root's d² (ties: the lower cell; integer `isqrt`, staying on `col`'s side of `o.y`). The cell must
  be within `RADIAL_PEER_SPAN` of `col`'s level and rock-like; otherwise the pair is skipped. `dq = q[hot] − q[peer]`.
  Matching radii cancels the radial background in both directions, so only lateral contrasts drive.
- `heated_from_below(topo, o, c)`: the hot root's outward neighbour is one step along the dominant axis of `c − o`
  (ties x, then y, then z). It must be in the domain and rock-like (radial overburden), and have `q ≤ q[c]`. Space or
  fluid outward → false (a flank root, where the column runs along the surface). Hotter overburden → false (a root
  warmed from the sun-heated sky, not from the core). The core cell itself → true.
- With no core (`topo.core == None`) neither pass fires.

**Iteration.** v1 used the radial peer plus "outward neighbour no hotter, with space counting as cold". At 100k,
rocky H8 still had 15 ceiling columns, all on the rim. An instrumented copy showed that the dominant rim firings were
plumes whose outward step was **space** (111 of 640 firings in the first ~16k ticks). v2, which is shipped, requires
rock-like overburden, so space or fluid outward → false.

**Tests (`chem/tests_p6.rs`).**
- `driven_core_heat_lifts_hot_columns_by_permutation` was re-fixtured as `relief_plate_deep`: an 8×8 rock slab
  y −6..3 (hot half Q 2400, cold 300) under AIR, plus a rock stem down to a pinned core at (3,−24,3), so the roots
  have rock overburden and a core below. Hot gives 5 uplifts in 4000 ticks; cold gives 0. Books are checked every tick.
- New `radial_peer_matches_radius_and_heated_from_below_reads_outward` (`radial_ball` fixture):
  - exact peers, plus a brute-force nearest-radius check
  - a rim column gives `None`
  - heated true on interior roots and on the core
  - false for sky-warmed overburden, space outward and air outward
- New `radial_heat_alone_builds_no_polar_dome`: r8 ball, Q = 3000 − 30·d², sky 6, no M1, 3000 ticks, books every tick.
  It asserts no cap columns and a polar mean rise ≤ 1. v2 gives polar 0.03 and cap 0. **Control:** the old same-y rule
  gives polar 3.00 and cap 4, so the test fails on the pre-change code.
- Results: `cargo test -p bitblock-core chem` 66 passed (3 ignored need external files). The full core suite is
  236 + wasm 74 passed. All 8 p6 goldens pass with `--include-ignored` (a, b, c, d, e, f, g, m), so Classic
  goldens are unchanged.

**Soaks (r12, M1 core_r 0, Driven).** Every v2 run has book_fail 0, driftE 0 and driftL all 0 for 300k ticks.

300k soaks are 250–300k block means. The dome mean is ρ≤4 mean h from `sky_probe`. Rim-at-cap counts the columns
whose top land is at the shell ceiling; every v1, v2 and radexp ceiling column sits at ρ ≈ 11–13.

| run | σ_h | max_h | mean_h | above_datum | dome 100k | cap cols 100k | dome / cap cols 300k |
|---|---|---|---|---|---|---|---|
| rocky H8 sky_shell (§5g) | 5.85 | 8 | +0.91 | 1314 | 7.86 | 61 | — |
| rocky H8 radexp (scratch) | 3.49 | 8 | +0.90 | 739 | 0.16 | 25 (rim) | −0.06 / 50 (rim) |
| rocky H8 v1 | 3.24 | 8 | +1.02 | 732 | 0.47 | 15 (rim) | — |
| **rocky H8 v2** | **2.60** | **8** | **+0.95** | **628** | **0.57** | **1** (6,−9) | **0.82 / 9 (rim)** |
| rocky H16 sky_shell | 7.07 | 16 | +1.63 | 1533 | 10.43 | 2 | — |
| rocky H16 radexp | 4.51 | 16 | +1.07 | 883 | 0.08 | 0 | — |
| rocky H16 v1 | 4.42 | 15.9 | +1.45 | 952 | 0.45 | 0 | — |
| **rocky H16 v2** | **2.87** | **13.8** | **+1.22** | **734** | **0.39** | **0** | **0.71 / 0** |
| ocean H8 sky_shell | 4.51 | 8 | −1.42 | 426 | 5.35 | 26 | — |
| ocean H8 radexp | 2.79 | 7 | −1.98 | 151 | −2.45 | 0 | — |
| **ocean H8 v2** | **2.15** | **6** | **−2.22** | **48** | **−2.24** | **0** | — |
| ocean H16 sky_shell | 4.43 | 15 | −1.58 | 412 | 5.51 | — | — |
| ocean H16 radexp | 2.61 | 11 | −2.06 | 121 | −2.43 | 0 | — |
| **ocean H16 v2** | **2.22** | **5** | **−2.10** | **59** | **−2.20** | **0** | — |

v2 law totals for 250–300k (weathered, scoured, uplifted, plumed, degassed):

| run | v2 | sky_shell |
|---|---|---|
| rocky H8 | 2607, 1504, 750, 1239, 22 | 2580, 1033, 1052, 12927, 18 |
| rocky H16 | 2651, 1017, 424, 1476, 7 | 2889, 1028, 1008, 17351, 17 |
| ocean H8 | 483, 2953, 235, 512, 991 | — |
| ocean H16 | 435, 1682, 200, 533, 771 | — |

H=0, which is play Driven with no sky shell, at 250–300k (books exact):

| run | σ_h | mean_h | uplifted | plumed |
|---|---|---|---|---|
| rocky v2 | 0.427 | −0.19 | 47 | 54 |
| rocky sky_shell | 0.366 | −0.13 | 142 | 333 |
| ocean v2 | 2.005 | −2.43 | 263 | 1042 |
| ocean sky_shell | 2.710 | −2.62 | 692 | 5399 |

**Reading.**
- The dome is gone at both heights. Rocky dome mean drops from 7.86 / 10.43 to 0.57 / 0.39 at 100k (0.82 / 0.71 at
  300k). Ocean comes off the ceiling entirely.
- The rim pin is mostly gone:
  - rocky H8 at 100k has 1 cap column, against 25 for radexp and 61 for sky_shell
  - rocky H8 at 300k has 9 rim columns (radexp at 300k: 50)
  - rocky H16 has none at 100k or 300k (max_h 13.8 < 16)
  - ocean has none
- What remains of rocky H8 at the cap is a thin rim, not a dome. At H=8 the rim can reach the cap after only ~8 steps
  of lateral relief.
- σ_h fell further than with radexp, because only lateral heat anomalies drive now: rocky 2.6–2.9, ocean ~2.2.
  Plume firings fell ~10×. Outside-ρ>4 σ is ~2.0 (100k) to 2.8–3.1 (300k).

**Risks.**
- Less relief: σ_h is lower than every earlier build with headroom. Much of the old σ was the dome itself.
- Rocky H8 still touches the cap at a few rim columns by 300k.
- The dominant-axis outward step is coarse near the 45° diagonals.
- `RADIAL_PEER_SPAN` = 8 is a tuning constant: a cold column that never reaches the radius within ±8 is skipped.
- H=0 play Driven changes (rocky σ 0.37 → 0.43, ocean 2.71 → 2.01; uplift and plume counts fall).
- The Q staircase (diffusion dead band ≈ 120 Q) is still frozen. The fix removes its bias but does not thaw it.

**Next steps.**
1. Void-collapse / unsupported-rock fall law, so raised rim land compacts.
2. Transparent-air sun.
3. A finer outward test (true radial neighbour or two-axis blend) if the rim persists at H8.
4. Multi-seed H ≥ 16 soaks.
5. M2.

**Files.**
- Code: `chem/p6_rest.rs` (`RADIAL_PEER_SPAN`, `radial_peer`, `heated_from_below`, uplift/plume gates) and
  `chem/tests_p6.rs` (fixtures `relief_plate_deep`, `radial_ball`, helper `polar_rise`, 2 new tests, 1 re-fixtured).
- Runs: `/workspace/scale_measure/radial_frame/`.
- Backups: `/workspace/gameladder_chem_findings/backups_radial_frame/{pre,post}/`.
- www rebuilt (`RUSTFLAGS="-C target-feature=+simd128" CARGO_PROFILE_RELEASE_PANIC=abort`, then `build_pages.py`, embed ==
  sidecar). wasm ABI stays 17. md5s: wasm `b360b2060c55a9d348d187bef03dd883` (1,302,349 B), play.html
  `14a97aaa9baa720b360d28b57488b5cd` (1,946,539 B), bitblock-lab.html `3d2bc0f49b5b63e25efa2d60405e291b` (1,973,834 B),
  bitblock.html unchanged `814320ae5671a4f5c14ccc0015b1e5e7`.

## 5j. Finer outward step for `heated_from_below` (2026-10-27, uncommitted; Owen pick "Refine the outward heated_from_below test if the H8 rim pin persists") — rim pin 9 → 1

**Problem (from §5i).** `heated_from_below` took its outward neighbour as one step along the **dominant axis** of
`c − o`. Near the 45° diagonals, and on the flanks below the equator, that single-axis step lands on buried rock while
the true outward cell is already space or fluid. So a flank root still counted as "heated from below with rock
overburden" and kept lifting the rim. Rocky H8 still had 9 rim columns at the shell ceiling by 300k.

**Change (Driven only; `crates/core/src/chem/p6_rest.rs`).**
- New `pub(crate) fn outward_step(d)` picks the nearest of the 26 lattice directions to `d = c − o`. It takes every
  axis whose component is at least half the largest (`2|d_i| ≥ max|d|`), each with its sign:
  - on an axis, or less than ~26.6° off it, this is the old single step
  - near a face or body diagonal it is the diagonal
  - `(0,0,0)` only at the core
- `heated_from_below` now uses `outward_step` and nothing else changed: the outward cell must be in the domain,
  rock-like, and have `q ≤ q[c]`; the core cell is still true.
- `radial_peer`, `RADIAL_PEER_SPAN`, the rolls, `dq` scaling, caps and permutation moves are unchanged. Classic never
  calls uplift or plume, so it is untouched.
- Cost: none measurable. It is the same single neighbour lookup, and win_ms/step is unchanged.

**Tests (`chem/tests_p6.rs`).**
- `radial_peer_matches_radius_and_heated_from_below_reads_outward` gains a diagonal flank case. In the r8 `radial_ball`,
  root (4,1,3) has buried dominant-axis step (5,1,3) but its true outward cell (5,1,4) is outside the ball, so the test
  now asserts false. The sky-warmed case moved its outward cell to the diagonal (5,3,0).
- New `outward_step_is_the_nearest_lattice_direction`:
  - axis and diagonal cases
  - a brute-force check over the 19³ cube that every step strictly increases the core distance
  - the dominant axis is always part of the step
- Results:
  - `cargo test -p bitblock-core -p bitblock-wasm --release`: core 237 passed (3 ignored), wasm 74 passed
  - `chem` filter: 67 passed
  - all 8 p6 goldens (a–g, m) pass with `--include-ignored`; no golden updates
- Logs are in `/workspace/scale_measure/finer_outward/tests/`.

**Soaks (r12, M1 core_r 0, Driven, seed 1).** Every v3 chem_soak run has book_fail 0, driftE 0 and driftL all 0 for
300k ticks. Block stats are 250–300k means. The dome is the ρ≤4 mean h and σ_out is the ρ>4 σ_h, both from `sky_probe`.
Cap columns are columns whose top land sits at the shell ceiling.

| run | σ_h (250–300k) | max_h | mean_h | above_datum | dome / cap cols 100k | σ_out 100k | dome / cap cols 300k | σ_out 300k |
|---|---|---|---|---|---|---|---|---|
| rocky H8 v2 (§5i) | 2.60 | 8.0 | +0.95 | 628 | 0.57 / 1 | 2.01 | 0.82 / **9** (rim) | 2.80 |
| **rocky H8 v3** | **1.97** | **7.5** | **+0.81** | **502** | **0.82 / 0** | **1.60** | **0.65 / 1** (11,0) | **2.14** |
| rocky H16 v2 | 2.87 | 13.8 | +1.22 | 734 | 0.39 / 0 | — | 0.71 / 0 | 3.09 |
| **rocky H16 v3** | **2.11** | **10.0** | **+1.06** | **596** | — | — | **0.47 / 0** | **2.24** |
| ocean H8 v2 | — | — | — | — | −2.24 / 0 | — | — | — |
| **ocean H8 v3** | — | — | — | — | **−2.08 / 0** | 2.08 | — | — |

The rocky H8 300k top-land histogram tail (h ≥ 6) is 31 columns for v2 (6:10, 7:12, 8:9) and 15 for v3 (6:11, 7:3,
8:1).

H=0 (play Driven, no sky shell) at 250–300k:

| run | σ_h | mean_h | land_r_std |
|---|---|---|---|
| rocky v2 | 0.427 | −0.19 | 0.376 |
| **rocky v3** | **0.438** | **−0.19** | **0.378** |
| ocean v2 | 2.005 | −2.43 | 1.536 |
| **ocean v3** | **1.925** | **−2.40** | **1.519** |

Whole-run (0–300k) Driven relief law totals are sums of every `W` window. These are not comparable to the §5i
"250–300k" totals, which used a different count.

| run | weathered | scoured | uplifted | plumed | degassed |
|---|---|---|---|---|---|
| rocky H8 v2 | 2949 | 1700 | 771 | 1611 | 36 |
| rocky H8 v3 | 2717 | 1561 | 692 | 1004 | 48 |
| rocky H0 v2 | 1501 | 983 | 50 | 60 | 332 |
| rocky H0 v3 | 1504 | 1076 | 47 | 38 | 66 |
| ocean H0 v2 | 483 | 3496 | 278 | 1185 | 183667 |
| ocean H0 v3 | 482 | 3416 | 195 | 1036 | 180536 |

**Reading.**
- **Rim pin goal met.** Rocky H8 drops from 9 rim columns to 1 at 300k and from 1 to 0 at 100k. The one remaining
  column, (11,0), sits on the +x axis, where `outward_step` is still the old single step, so it is not a diagonal-flank
  artefact.
- **The dome does not return.** The rocky H8 dome is 0.82 at 100k and 0.65 at 300k (v2 0.57 / 0.82); H16 is 0.47.
  The hottest cell stays at the core (d = 0), and mean Q by radius is identical to v2.
- **Books are exact** in all 4 300k soaks: rocky H8, rocky H16, rocky H0 and ocean H0.
- **Relief is lower again.** Rocky H8 σ_h falls 2.60 → 1.97 and above_datum 628 → 502; rocky H16 falls 2.87 → 2.11.
  Plume firings fall ~38% at H8, which is the spurious flank-root plumes removed. The rim pin and part of the remaining
  relief were the same artefact. H=0 play is essentially unchanged (rocky σ 0.43 → 0.44, ocean 2.01 → 1.93).

**Risks / next.**
- σ_h keeps falling with each radial-frame correction (5.85 → 2.60 → 1.97 at rocky H8). True relief now needs a real
  lateral driver: void-collapse / unsupported-rock fall, transparent-air sun, or M2 plate-scale advection.
- There is still one axis rim column at H8 by 300k. A sub-cell / continuous outward compare is the next refinement,
  for example interpolating `q` along the true unit radial or a two-cell outward average. It is not needed for the
  goal.
- The Q staircase (diffusion dead band ≈ 120 Q) is still frozen. This change removes another bias but does not thaw
  it.
- `RADIAL_PEER_SPAN` = 8 is unchanged as a tuning constant.
- Single seed only; multi-seed H ≥ 16 soaks are still open.

**Files.**
- Code: `chem/p6_rest.rs` (`outward_step`, `heated_from_below`) and `chem/tests_p6.rs` (1 test extended, 1 new).
- Runs: `/workspace/scale_measure/finer_outward/`, with `probe_*`, `v3soak_*`, `blocks_summary.txt`, `blocks.py` and
  `bin/{sky_probe_v3,chem_soak_v3}`.
- Backups: `/workspace/gameladder_chem_findings/backups_finer_outward/{pre,post}/`, with `post/*.diff` and both `www`
  sets.
- www rebuilt (`RUSTFLAGS="-C target-feature=+simd128" CARGO_PROFILE_RELEASE_PANIC=abort`, wasm32 release, then
  `build_pages.py`, embed == sidecar). wasm ABI stays 17. md5s:
  - wasm `5fe72478fdec38f9c93b339c1a9b804d` (1,302,385 B)
  - play.html `bbfb4c24e1d914c587fe98730760c93d` (1,946,587 B)
  - bitblock-lab.html `d8d2d68dd9b0c2eaf3dea1a93fe517fc` (1,973,882 B)
  - bitblock.html unchanged `814320ae5671a4f5c14ccc0015b1e5e7`
- Served on 8765. Chrome chem confirm and the Desktop sync are still pending.

## 5k. Void collapse: unsupported rock falls toward the core (2026-10-27, uncommitted; Owen pick "Add a void-collapse or rock-fall law so raised land compacts and builds real relief") — law works, relief goal not met

**Hypothesis tested.** Rocky H8 σ_h is stuck near 2.0 (§5j) because dry voids under raised columns (plume-drawn
fluid pores, overhangs) never settle, so raised land stays as foam instead of compacting into real relief.

**Change (Driven only; `crates/core/src/chem/p6_rest.rs`, `chem/p6.rs`).**
- New `collapse_pass`, run in the Driven branch after `degas_pass` and before `settle_pass` (both `step` and
  `step_profiled`). It is gated on `params.settle` and needs `topo.core`. New `StepReport.collapsed`.
- Fall direction is `-outward_step(c - o)`, the nearest of the 26 lattice directions toward the core.
  `inward_support(o, c)` returns that step plus, for a diagonal step, its single-axis components (a staircase rests on
  its treads). The set is empty at the core.
- A rock-like cell is unsupported only if **every** support cell is in the domain and AIR. Out-of-domain, any solid,
  and WATER all support it, so water-filled pores hold (pore pressure) and ocean degassing is untouched.
- It falls only when the inward step has a −y component (world gravity is −y). Then every collapse lowers y and cannot
  cycle with `settle_pass`. The underside of the ball keeps hanging, as it already did for SED.
- Odds `COLLAPSE_ODDS = 4096/65536` per tick (64-bit `wide_roll`, salt `0xc011a95e`). A `moved` bitvec limits each
  void to one cell of travel per tick. The move is a pure material swap (solid ↔ AIR, Q stays with cells), so books are
  exact by construction.
- Classic never calls it.

**Iteration.** v1 used the pure radial inward step with no −y restriction. Below the equator SED settled −y into AIR
and collapse pushed it back +y: ~3000 collapses and ~2800 settles per 5k ticks, min_h −8, σ_h 2.14 at 30k (a churn
artefact). v2 (shipped) adds the −y gate; at 30k σ_h was 1.21 vs baseline 1.18.

**Tests (`chem/tests_p6.rs`, 4 new, plus `radial_ball_with` carve fixture and `collapse_ball` helper).**
- `inward_support_is_the_inward_step_plus_its_axes`: axis, face- and body-diagonal sets, empty at the core.
- `driven_buried_void_collapses_to_the_surface_and_compacts_the_column`: a dry pore 3 cells under the pole climbs in
  exactly 3 collapses, the column top drops 6 → 5, the rock count is conserved, `q` is unchanged, and books/energy/lane
  drift are checked every tick.
- `void_collapse_spares_held_cells_wet_pores_the_underside_and_classic`: a diagonal void under axis treads and a
  south-pole pore (inward step +y) see 0 collapses in 2000 ticks; a water-filled pore holds while it stays wet; Classic
  never fires.
- `void_collapse_drops_an_overhang`: a ledge over AIR falls exactly one cell and rests.
- Results: `cargo test -p bitblock-core -p bitblock-wasm --release` core 241 passed (3 ignored), wasm 74 passed;
  `chem` filter with `--include-ignored` 74 passed, including all p6 goldens; no Driven test or golden changed.
  Logs: `/workspace/scale_measure/void_collapse/tests/`.

**Soaks (r12, M1 core_r 0, Driven, seed 1, 300k).** All 4 runs have book_fail 0, driftE 0, driftL all 0. Block stats
are 250–300k means. Dome = ρ≤4 mean h and σ_out = ρ>4 σ_h from `sky_probe`; cap = columns at the shell ceiling.

| run | σ_h (250–300k) | max_h | mean_h | above_datum | dome / cap 100k | σ_out 100k | dome / cap 300k | σ_out 300k | collapsed (0–300k) |
|---|---|---|---|---|---|---|---|---|---|
| rocky H8 v3 (§5j) | 1.97 | 7.5 | +0.81 | 502 | 0.82 / 0 | 1.60 | 0.65 / 1 | 2.14 | — |
| **rocky H8 collapse** | **1.99** | **6.6** | **+0.67** | **472** | **0.27 / 0** | **1.66** | **0.08 / 0** | **2.12** | **834** |
| rocky H16 v3 | 2.11 | 10.0 | +1.06 | 596 | — | — | 0.47 / 0 | 2.24 | — |
| **rocky H16 collapse** | **2.24** | **8.4** | **+1.00** | **606** | — | — | — | — | **907** |
| ocean H8 v3 | — | — | — | — | −2.08 / 0 | 2.08 | — | — | — |
| **ocean H8 collapse** | — | — | — | — | **−2.18 / 0** | **2.00** | — | — | — |

H=0 (play Driven, no sky shell), 250–300k:

| run | σ_h | mean_h | land_r_std | collapsed (0–300k) |
|---|---|---|---|---|
| rocky v3 | 0.438 | −0.19 | 0.378 | — |
| **rocky collapse** | **0.437** | **−0.19** | **0.377** | **5** |
| ocean v3 | 1.925 | −2.40 | 1.519 | — |
| **ocean collapse** | **1.807** | **−2.77** | **1.476** | **199** (175 in the first 50k) |

Whole-run law totals, rocky H8 v3 → collapse: settled 5077 → 5622, weathered 2717 → 2692, scoured 1561 → 1504,
uplifted 692 → 689, plumed 1004 → 1103, degassed 48 → 50. Collapse rate at rocky H8: 243 in the first 5k ticks (the
genesis foam), then a steady ~1–2 per 1k ticks (200 in 200–300k), i.e. it keeps closing the pores plume opens.

**Reading.**
- **Real-relief goal not met.** Rocky H8 σ_h 1.97 → 1.99 is inside the ±0.04 block noise; H16 2.11 → 2.24 is a small
  gain, but max_h falls there too (10 → 8.4).
- **The law does what it says: raised land compacts.** max_h 7.5 → 6.6, mean_h +0.81 → +0.67, above_datum 502 → 472,
  the dome flattens (0.65 → 0.08 at 300k) and the last ceiling column (11,0) is gone. No rim or dome comes back.
- **Hypothesis largely discarded.** There are few dry voids under upper-hemisphere land: after the initial 243
  collapses, it fires ~1 per 1k ticks. Low σ is not foam. It is the slow, capped uplift/plume driver against
  weathering/scour, plus the frozen radial Q staircase. Compaction removes height rather than creating contrast.
- **Ocean H0 loses relief** (σ 1.93 → 1.81, mean_h −2.40 → −2.77): early dry pores in the ocean crust close and the
  floor sinks. That is physically consistent but is a real change to Driven play.
- Cost is one pass with a per-candidate support check; win_ms/step is within noise (rocky H8 2.18 vs 2.09).

**Risks / next.**
- σ needs a real lateral driver, not compaction: (a) transparent-air sun so the water cycle and erosion contrast
  return under the sky shell; (b) relax `UPLIFT_MAX_STEP` / `PLUME_SLOPE` caps or let plume heads lift a whole
  column (isostasy-like); (c) M2 plate advection; (d) lithification so SED blankets stop armouring and smoothing.
- If Owen does not want Driven ocean play to sink, the law can be reverted from `backups_void_collapse/pre/` or gated
  off for H=0; currently it is always on in Driven.
- The −y gate means the southern hemisphere never compacts (same as SED settling); a true radial-gravity frame
  would need `settle_pass`/`flow_pass` to go radial too.
- Single seed only.

**Files.**
- Code: `chem/p6_rest.rs` (`COLLAPSE_ODDS`, `SALT_COLLAPSE`, `inward_support`, `collapse_pass`), `chem/p6.rs`
  (`StepReport.collapsed`, Driven order), `chem/tests_p6.rs` (`radial_ball_with`, `collapse_ball`, `run_collapse`,
  4 tests). `chem_soak` prints `collapsed` as the 17th law.
- Runs: `/workspace/scale_measure/void_collapse/` (`vc_soak_r12_*_300k.txt`, `probe_{rocky,ocean}_sky8_100k.txt`,
  `probe_rocky_sky8_300k.txt`, `blocks.py`, `blocks_summary.txt`, `tests/`, `bin/{chem_soak,sky_probe}_vc{1,2}`; vc1
  is the discarded ping-pong variant).
- Backups: `/workspace/gameladder_chem_findings/backups_void_collapse/{pre,post}/`, with `post/*.diff` and both `www`
  sets.
- www rebuilt (`RUSTFLAGS="-C target-feature=+simd128" CARGO_PROFILE_RELEASE_PANIC=abort`, wasm32 release, then
  `build_pages.py`, embed == sidecar). wasm ABI stays 17. md5s:
  - wasm `64015d3b5fcb39e57a565aa39046c307` (1,304,144 B)
  - play.html `36649d04610a00149666463f0ee0ee8e` (1,949,162 B)
  - bitblock-lab.html `5886415b6fc925d444ee52912fdcbfaa` (1,976,457 B)
  - bitblock.html unchanged `814320ae5671a4f5c14ccc0015b1e5e7`
- Served on 8765. Chrome chem confirm and the Desktop sync are still pending.

## 5l. Transparent-air sun: light crosses AIR and heats the first non-air cell (2026-10-27, uncommitted; Owen pick "Let sunlight pass through air and heat the first non-air cell") — water cycle restored under the shell, σ_h up

**Diagnosis.** M1 sun is deposited per `Topology::space_faces` cell: each face with no domain neighbour gets
`sun * max(0, d·s) / 1024`. That cell was the absorber whatever it held. With a sky shell (H>0) every space-face cell
is the top layer of AIR, so the sun heated the top of the atmosphere, not the ground. Open-sky AIR averaged Q≈590
while ground tops sat at ≈330, there were 12 ice tops, and rocky H8 had only about 12 vapor cells.

**Change (Driven only; `crates/core/src/chem/p6.rs`).**
- In `energy_input_pass`, under `WaterCycle::Driven`, each lit face `k` of a space-face cell `i` carries its own
  share `fk = sun * d_k / 1024`. `sun_target(topo, i, k)` marches from `i` along `-NB6[k]` while the cell is AIR and
  returns the first non-AIR cell. `fk` goes into that cell's `q`, and `total` (booked `input_q`) grows by `fk`.
- A ray that crosses only AIR and leaves the domain returns `None`. That light passes straight through and is neither
  deposited nor booked, so books stay exact by construction. Space is transparent. WATER, ICE, rising gases and every
  solid absorb.
- Low sun works without new rules: a ±x face ray crosses AIR sideways and lands on a cliff. There is no extra
  shadowing beyond "first non-AIR cell along the face normal". The day clock, geometry and `weather_pass` lit gate
  are unchanged.
- Classic (and so planet6 play and every Classic golden) keeps the rim absorber bit-for-bit. At H=0, Driven
  changes only where the space-face cell is AIR, e.g. above a basin.

**Tests (`chem/tests_p6.rs`, 6 new; helpers `sun_only`, `sun_column`, `q_at`).**
- `sun_passes_through_air_and_heats_the_rock_below`: an AIR column over rock. Driven puts all 16 on the rock;
  Classic puts it on the top AIR cell.
- `sun_heats_a_wet_surface_and_stops_at_ice_and_sediment`: WATER, ICE and SED each absorb and shade the rock below.
- `an_all_air_column_lets_the_light_through_unbooked`: injected 0, `input_q` 0, books OK over 50 ticks.
- `a_low_sun_crosses_air_sideways_to_a_cliff`: with sx, sy > 0, a rock at the end of an AIR row gets fx + fy.
- `sky_shell_sun_never_lands_on_air_and_books_every_quantum`: genesis_sky H6. ΣΔq equals the injected amount, and
  no AIR cell gains Q.
- `transparent_sun_books_exact_every_tick_under_a_sky_shell_full_driven`: 700 ticks with every law and the trimmed
  net. Every book holds every tick, and energy and lane drift are 0.
- Two existing tests were adjusted, because sun now depends on the AIR layout:
  - `core_sphere_books_exact_every_tick_driven_and_classic`: exact injection equality is now asserted only for
    Classic; Driven asserts injected ≥ core every tick.
  - `driven_core_heat_lifts_hot_columns_by_permutation`: sets `sun = 0` (core-only). Sun on the thin plate's rock
    tops flipped its root gradient.
- Results (`--release`): core 247 passed (3 ignored), wasm 74 passed, `chem --include-ignored` 80 passed, including
  all p6 goldens. Logs: `/workspace/scale_measure/sun_air/tests/`.

**Soaks (r12, M1 core_r 0, Driven, seed 1, 300k; pre = §5k tree rebuilt from `backups_sun_air/pre`, which
reproduces the §5k vc numbers exactly).** All 8 runs: book_fail 0, driftE 0, driftL all 0. Values are 250–300k block
means. `top_q` is the mean Q of the top non-AIR cell per column, and `open_air_q` is the mean Q of sky-open AIR (new
`sun:` line in `chem_soak`).

| 250–300k | rocky H8 pre → sa | ocean H8 pre → sa | rocky H0 pre → sa | ocean H0 pre → sa |
|---|---|---|---|---|
| σ_h | 1.99 → **2.34** | 1.82 → **2.17** | 0.44 → 0.46 | 1.81 → 1.83 |
| max_h / mean_h | 6.6 / +0.67 → 7.1 / +0.47 | 2.0 / −2.50 → 5.0 / −2.56 | 0 / −0.19 → 0 / −0.20 | 0 / −2.77 → 0 / −2.87 |
| above_datum | 472 → 521 | 9 → 49 | 0 | 0 |
| land_r_std | 1.19 → 1.56 | 1.14 → 1.50 | 0.38 → 0.39 | 1.48 → 1.49 |
| vapor cells | 12 → **106** | 438 → **1,244** | 111 → 114 | 1,619 → 1,778 |
| top_q / land / water | 326 / 343 / 160 → 841 / 866 / 509 | 348 / 402 / 307 → 589 / 558 / 665 | 522 / 513 / 708 → 524 / 515 / 707 | 578 / 514 / 697 → 655 / 589 / 799 |
| ice tops | 12.2 → 1.9 | 11.9 → 3.3 | 0 → 0 | 1.9 → 1.0 |
| open-sky AIR Q | 587 → 417 | 613 → 412 | 618 → 565 | 814 → 696 |
| evap / rained, last 50k | 1.1k / 1.1k → 10.3k / 10.5k | 42.8k / 42.9k → 110k / 123k | 9.7k / 9.7k → 10.0k / 10.0k | 128k / 158k → 138k / 174k |
| win_ms/step | 2.20 → 2.20 | 2.32 → 2.39 | 4.6* → 2.04 | 1.98 → 1.93 |

\*one noisy window.

Whole-run law totals (0–300k), pre → sa:
- **rocky H8:** evap 8.1k → 60.8k; entrained 7.7k → 29.1k; scoured 1.5k → 3.3k; uplifted 689 → 2,267;
  degassed 50 → 815; plumed 1,103 → 807; weathered 2,692 → 2,371; froze 23.0k → 9.5k; collapsed 834 → 364.
- **ocean H8:** evap 238k → 623k; degassed 1.4k → 69.9k; froze 297k → 164k; scoured 2.0k → 2.9k; plumed 512 → 1,440.
- **H0:** within a few % except froze (rocky 696 → 238, ocean 44.5k → 29.7k).

Dome/rim (`sky_probe`, rocky H8; pre = §5k probes):

| | dome mean (ρ≤4) / cap | σ_out (ρ>4) |
|---|---|---|
| 100k pre → sa | 0.27 / 0 → **−1.02** / 0 | 1.66 → 2.13 |
| 300k pre → sa | 0.08 / 0 → **−1.65** / 0 | 2.12 → 2.24 |

Mean Q by radius at 300k: shell air at radius 17–20 drops from 579/506/345/39 to 114/52/1/0, the near-surface crust
(radius 12–13) rises from 264/366 to 490/464, and the interior (radius 1–10) warms by about 170–250.

**Reading.**
- **Pass: sun reaches the surface under the shell.** No AIR cell takes sun (test). Ground tops warm about 2.6× at
  rocky H8 and the cold sky air cools. Ice tops nearly disappear. Rocky H8 vapor rises about 9× and evaporation
  about 7.5×, reaching the same per-50k rate as H0, so the sky shell no longer starves the water cycle.
- **Relief rises with the restored cycle.** σ_h is +0.35 at both H8 presets, about 9× the ±0.04 block noise, and
  still edging up at 300k on ocean. land_r_std is about +0.37, max_h is higher, and above_datum ocean land rises
  about 5×. Erosion contrast returns (scour ×2.2, entrainment ×3.8). Uplift ×3.3 because sun-warmed tops and
  wetter, warmer crust feed `heated_from_below`.
- **H0 play is nearly unchanged.** σ shifts are within a few hundredths, ocean H0 vapor +10%, and fewer freezes.
  Play does change (Driven + M1 only), so www was rebuilt.
- **New risk: a subsolar basin.** At rocky H8 the ρ≤4 polar patch, where the zenith sun is strongest, sinks to
  mean −1.65 (σ 1.87) instead of a dome. It looks like enhanced evaporation and erosion where insolation peaks, not a
  ceiling pin (cap 0). Watch it with H16, multi-seed runs and a tilted or orbiting sun before calling it emergent.
- Ocean rain exceeds evap in both pre and sa, because degassing and other water sources add rain; the books still
  close.

**Risks / next.**
- Driven-only gate: Classic still heats the rim (by design, for goldens).
- No albedo, no partial transmission through vapor/gas (gases absorb fully), no oblique-ray shadowing (rays follow
  the face normal).
- Single seed. `driven_core_heat_lifts_hot_columns_by_permutation` now has to switch the sun off, because surface sun
  can mask a shallow core gradient.
- Next options: (a) relax uplift/plume caps or add isostatic whole-column lift, now that the cycle has water to
  erode against; (b) albedo (ice and water reflect part of `fk`) for climate feedback; (c) M2 plate advection; (d)
  lithification; (e) multi-seed H ≥ 16 soaks to test the subsolar basin.

**Files.**
- Code: `chem/p6.rs` (`energy_input_pass` Driven branch, `sun_target`, `EnergyInput` doc) and `chem/tests_p6.rs`
  (6 new, 2 adjusted). `chem_soak` prints a `sun:` line (`sun_probe`).
- Runs: `/workspace/scale_measure/sun_air/`:
  - soaks `{pre,sa}_soak_r12_{rocky,ocean}_m1_driven_sky{0,8}_300k.txt`
  - probes `probe_sa_rocky_sky8_{100k,300k}.txt`
  - `blocks.py`, `blocks_summary.txt`, `tests/`
  - binaries `bin/{chem_soak_pre,chem_soak_sa,sky_probe_sa}`
  - pre build tree at `/workspace/sunair_pre/`
- Backups: `/workspace/gameladder_chem_findings/backups_sun_air/{pre,post}/`, with `post/*.diff` and both `www`
  sets.
- www rebuilt (`RUSTFLAGS="-C target-feature=+simd128" CARGO_PROFILE_RELEASE_PANIC=abort`, wasm32 release, then
  `build_pages.py`, embed == sidecar). wasm ABI stays 17. md5s:
  - wasm `7c898c3385bbf2d836d881bbcd8d102a` (1,304,675 B)
  - play.html `4ff8ea85ce6f148f260a9dd0bc3f068f` (1,949,936 B)
  - bitblock-lab.html `17b30e911883dfc27667c005bb49b339` (1,977,231 B)
  - bitblock.html unchanged `814320ae5671a4f5c14ccc0015b1e5e7`
- Served on 8765 (same `http.server` process, `-d crates/wasm/www`). Chrome chem confirm and the Desktop sync are
  still pending.

## 5m. Whole-column plume: the cold column subsides and the hot column rises as blocks (2026-10-27, uncommitted; Owen pick "Relax uplift and plume caps or let plumes lift whole columns for real lateral relief") — rocky H8 σ_h 2.34 → 3.90, books exact

**Diagnosis.** Before this change the §5e plume was root-only: the cold leg moved only the cold root `h` and pulled surface fluid down
into a buried void, which settle, flow or collapse later closed. Most of its relief vanished again, so σ_h sat
near the uplift/scour plateau. Loosening the uplift caps alone did little at 100k: `UPLIFT_MAX_STEP` 3 → 6 gave
σ 2.13 → 2.12, and adding uplift 2/tick gave 2.16 (`exp/probes/u6*`). Lifting whole columns with no guards worked (σ 4.54 at 100k) but brought the
ceiling pin back, with 7 columns at the cap.

**Change (Driven only; `crates/core/src/chem/p6_rest.rs` `plume_pass`, Classic untouched).**
- Whole-column cycle. The cold leg is now the whole column `t .. h`, so the rotation is
  `a → t → … → h → channel → h' → … → n → f → a`. The cold column subsides one cell as a block, and its top
  takes in the surface fluid. The hot column rises one cell as a block. It is still a pure permutation: Q stays
  with the cells, so lanes, BIND and Q are untouched and no buried void is seeded.
- Three guards keep this to broad relief:
  - **`PLUME_HEADROOM = 1`**: the hot fluid slot `f` must have one more fluid cell straight above it, so a rising
    top never fills the outermost domain layer (no sky-shell ceiling pin).
  - **Flank gate**: both tops `t` and `n` must have `+y` as their nearest outward lattice direction
    (`outward_step`). A vertical column then runs radially, and the ball's flank never stacks sideways.
  - **`PLUME_REPOSE = 4`**: after the move, the sunk cold top may sit at most 4 below each in-domain lateral
    neighbour top, and the risen hot top at most 4 above each. This gives no shafts and no spires. It uses the new
    helpers `within_repose` and `col_top_near`, which search for a neighbour top within `REPOSE + 2`. A missing
    top fails the check, and a neighbour off the domain edge does not count.
- `UPLIFT_MAX_STEP` (3), `PLUME_SLOPE` (2), `PLUME_K`, `PLUME_MAX_PER_TICK` and the uplift law are unchanged. The
  whole-column plume is the only driver change. `p6.rs` is unchanged.
- No experiment knobs are left in the tree. The `XK_*` env knobs exist only in the scratch copy at
  `uplift_caps/exp/bitblock`. The shipped code equals the scratch build with `XK_PWHOLE=1 XK_PHEAD=1 XK_PFLANK=1 XK_PNB=4`
  ("wfd1nb4"). A 20k rocky H8 run is line-identical apart from timing (`uplift_caps/equiv/`), and the 100k
  `sky_probe` output matches exactly.

**Tests (`chem/tests_p6.rs`: 4 new, 1 adjusted; helpers `plume_only_slab`, `run_plume_only`, which checks books, energy
drift 0 and lane drift 0 every tick).**
- `whole_column_plume_lowers_one_cold_top_and_raises_one_hot_top_per_firing`: per tick, exactly `plumed` tops go
  down 1 and `plumed` go up 1. No column moves by any other amount, no AIR lies under rock, and the rock count is
  conserved (47 firings; hot mean 10.96, cool 8.35).
- `whole_column_plume_keeps_neighbour_steps_within_repose`: over 30k plume-only ticks no neighbour step ever exceeds
  `PLUME_REPOSE` (66 firings; the worst step reaches 4).
- `whole_column_plume_needs_headroom_over_the_hot_slot`: with 1 fluid layer, 0 firings. With 2 layers it fires, and rock
  never enters the top layer.
- `whole_column_plume_only_moves_columns_along_their_outward_axis`: on a radial ball with a hot half, every moved
  column has `outward_step(..).1 == 1` (230 firings).
- `driven_plume_builds_broad_relief_by_permutation` (adjusted): the old assertion was "buried air > 0", which was the
  root-only signature. It now asserts `buried_air == 0`.
- Results (`--release`): core 251 passed (3 ignored), wasm 74 passed, `chem --include-ignored` 84 passed, including
  all p6/Classic goldens. Log: `uplift_caps/tests/final_tests.txt`.

**Soaks (r12, M1 core_r 0, Driven, seed 1, 300k; sa = §5l sun-air, ship = wfd1nb4).** All runs: book_fail 0,
driftE 0, driftL all 0. Values are 250–300k block means.

| 250–300k | rocky H8 sa → ship | ocean H8 sa → ship | rocky H0 sa → ship | ocean H0 sa → ship |
|---|---|---|---|---|
| σ_h | 2.34 → **3.90** | 2.17 → **2.60** | 0.46 → 0.51 | 1.83 → **2.28** |
| max_h / mean_h | 7.1 / +0.47 → 8.0 / +0.32 | 5.0 / −2.56 → 4.0 / −3.06 | 0 / −0.20 → 0 / −0.26 | 0 / −2.87 → 0 / −3.27 |
| above_datum | 521 → 745 | 48.5 → 49.6 | 0 | 0 |
| land_r_std (radial) | 1.56 → **2.81** | 1.50 → **2.01** | 0.39 → 0.47 | 1.49 → 1.98 |
| vapor cells | 106 → 110 | 1,244 → 1,223 | 114 → 108 | 1,778 → 1,783 |
| top_q | 841 → 641 | 589 → 550 | 524 → 543 | 655 → 671 |
| ice tops | 1.9 → 4.0 | 3.3 → 3.6 | 0 | 1.0 → 0.4 |
| win_ms/step | 2.20 → 2.44 | 2.39 → 2.48 | 2.04 → 1.74 | 1.93 → 4.81* (other windows 1.97–2.50) |

\*one noisy window.

- Rocky H8 σ_h by block, ship: 2.17 / 3.35 / 3.67 / 3.81 / 3.88 / 3.90. It is still creeping up at 300k, by about +0.02 per 50k.
- Whole-run law totals for rocky H8 (0–300k), sa → ship: plumed 807 → 6,188; uplifted 2,267 → 1,478; collapsed
  364 → 1,162; entrained 29.1k → 35.6k; scoured 3.3k → 3.6k; evap 60.8k → 63.8k.
- Ocean H8 plumed is about the same (1,440 → 1,387). Its gain comes from each firing moving a whole column.

Ablations (rocky H8, 250–300k σ_h unless noted; `uplift_caps/blocks_summary.txt`, `exp/probes/`):
- No flank gate (wd1nb4): σ 3.73. Ocean H8 rises to 2.95, but rim ceiling cols at 300k come only from the sediment ledge (1).
- `PLUME_REPOSE = 3` (wfd1nb3): σ 3.20, with 2 ceiling cols at 300k.
- 100k probes:
  - Whole column with no guards: σ 4.54, ceiling 7.
  - `+HEADROOM` only: σ 4.12, ceiling 6.
  - `+flank`: σ 4.26, ceiling 4.
  - `+repose 4`: σ 3.51, ceiling 0.
  - `repose 5`: σ 4.06, ceiling 2.
- Repose 4 is the largest bound that keeps the 100k ceiling at 0.

**Dome/rim (`sky_probe`, rocky H8; sa = §5l probes; ship = main-equivalent probe `exp/probes/ship_rocky_sky8_100000.txt`,
`wfd1nb4b_rocky_sky8_300000.txt`).**

| | dome mean / σ (ρ≤4) | σ_out (ρ>4) | ceiling cols (rim) | sigma_3x3 | max_step / steps>3 (of 832) |
|---|---|---|---|---|---|
| 100k sa → ship | −1.02 / 1.56 → −2.31 / 3.22 | 2.13 → **3.43** | 0 → **0** | 1.73 → 2.85 | 6 / 8 → 10 / 107 |
| 300k sa → ship | −1.65 / 1.87 → −2.20 / 3.66 | 2.24 → **3.84** | 0 → 3 (3) | — → 3.31 | — → 12 / 130 |

- Ship at 150k and 200k: ceiling cols 1 and 2, all at the rim.
- Height autocorrelation at 300k: lag 1 0.78, lag 2 0.65, lag 4 0.38, lag 6 0.13. At 100k sa it was 0.63, 0.47, 0.23, 0.10. The relief is broader, not noise.

**Reading.**
- **Pass: lateral relief is clearly up.** Rocky H8 σ_h rises +1.56, about 40× the block noise. Radial land_r_std rises ×1.8, the
  3×3-smoothed σ rises ×1.65, and longer-lag correlation goes up. Ocean H8 gains +0.43 and ocean H0 +0.45. Rocky H0 is
  nearly flat: it has no headroom over most of the ball, so the headroom gate blocks almost every firing (plumed 6).
- **Books exact** in every soak, probe and test.
- **Pass: no uplift/dome ceiling pin.** The 100k ceiling count is 0, as in sa, and the polar dome stays a basin with no cap.
- **Caveat, rim sediment ledge.** By 300k, 3 rim columns (ρ ≥ 10) reach the probe ceiling: (10,6), (11,1) and
  (12,0). They are pure SED stacks (`sed sed sed…`), not plume- or uplift-lifted rock. The flank gate forbids plume
  moves there, because those tops point outward along ±x/±z. More relief means more entrainment, and the sediment
  settles under global −y gravity onto the equatorial ledge, where a vertical column runs nearly tangent to the ball.
  This is a soak-metric artifact of vertical columns at the rim, not the §5i/§5j rim pin. It is still a mild return
  in the probe's count (0 → 3 at 300k) and holds rocky H8 `max_h` at the H = 8 cap in the soak.
- The subsolar basin from §5l deepens (dome mean −1.65 → −2.20). The polar patch is now the plume's cold, subsiding
  side, as well as the side with the strongest evaporation.
- `max_step` 12 is above the repose bound of 4. The repose bound applies only to plume moves, and the large steps come
  from settle and flow plus the rim ledge.
- Play changes only for Driven + M1, as in §5l. www was rebuilt.

**Risks / next.**
- Single seed and r12 only. σ is still creeping at 300k, so run multi-seed H ≥ 16 soaks before calling it saturated.
- Rim sediment ledge: lithification, an angle-of-repose rule for SED or radial gravity for settle would address it.
  Alternatively, measure relief radially only. land_r_std already shows the gain.
- The repose search window is `REPOSE + 2`, so a neighbour top more than 6 cells away blocks a firing rather than
  allowing it. This is conservative.
- Cost: +0.24 ms/step at rocky H8.
- Next options: (a) lithification of SED (also cures the ledge); (b) albedo; (c) multi-seed H16 soaks; (d) M2 plate
  advection; (e) radial `settle` gravity for Driven.

**Files.**
- Code: `chem/p6_rest.rs` (`PLUME_HEADROOM`, `PLUME_REPOSE`, `plume_pass` whole-column path and guards,
  `within_repose`, `col_top_near`) and `chem/tests_p6.rs` (4 new, 1 adjusted). `p6.rs` and `chem_soak` are unchanged.
- Runs: `/workspace/scale_measure/uplift_caps/`:
  - `blocks_summary.txt` and `exp/soaks/{wfd1nb4,wd1nb4,wfd1nb3}_r12_*_300k.txt`
  - probes `exp/probes/*` (`ship_rocky_sky8_{100000,300000}.txt` = main-equivalent)
  - knob-scratch tree `exp/bitblock`, `exp/sm`, `exp/runs/`
  - `equiv/` and `tests/final_tests.txt`
- Backups: `/workspace/gameladder_chem_findings/backups_uplift_caps/{pre,post}/`, with `post/*.diff` and both `www` sets.
- www rebuilt (`RUSTFLAGS="-C target-feature=+simd128" CARGO_PROFILE_RELEASE_PANIC=abort`, wasm32 release, then
  `build_pages.py`, embed == sidecar). wasm ABI stays 17 (`bitblock_wasm_abi()` = 17). md5s:
  - wasm `a63ad79ab56313adb3a8a27d90511d49` (1,309,797 B)
  - play.html `34cccaa5b2693e9ffa0081ec60abb9da` (1,957,435 B)
  - bitblock-lab.html `f9b362b589597776590cfc8bd22d7ff8` (1,984,730 B)
  - bitblock.html unchanged `814320ae5671a4f5c14ccc0015b1e5e7`
- Served on 127.0.0.1:8765 (`http.server -d crates/wasm/www`). The Chrome chem confirm and the Desktop sync are still pending.
  Desktop still has the sun-air build `7c898c33…`.

## 5n. Albedo: ice and water reflect part of the Driven sun (2026-10-27, uncommitted; Owen pick "Add albedo so ice and water reflect part of the sunlight") — ice persists, oceans cooler, evap down, books exact

**Where heat was booked before.** `EnergyInput` sun in `energy_input_pass` (`chem/p6.rs`). Under Driven, each lit
space face's share `fk = sun·(d·s)/1024` rides `sun_target` through AIR to the first non-AIR cell `t`, all of `fk`
goes into `Q[t]`, and the same amount is added to `input_q` (the energy book subtracts it). An all-AIR ray returns
`None` and is never deposited or booked.

**Change (Driven only; Classic rim absorber bit-for-bit; Soft DDA untouched).**
- `albedo(m)` in 1/1024ths: **ICE 614 (~0.60)**, **WATER 154 (~0.15)**, and everything else 0. "Everything
  else" covers ROCK, MIN, SED, GYP and gases. Land stays at the §5l baseline, and AIR never reaches the test
  because it is transparent.
- At the first non-AIR cell, the absorbed part is `absorbed_share(fk, a, tick, t, k)`, which computes
  `floor((fk·(1024−a) + dither)/1024)`. The dither is a stateless 10-bit hash of (tick, cell, face).
  - Its expectation is exactly `fk·(1−a/1024)`, so small dawn quanta are not truncated to 0.
  - It is deterministic, never exceeds `fk`, and is the identity when `a = 0`.
- **Books.** Only the absorbed part goes into `Q` and `input_q`. The reflected part `fk − absorbed` is never
  injected, so it is in no book: `energy_drift == 0` with no new ledger term. It is counted only in the diagnostic
  `ChemWorld::reflected_q()` and `StepReport.reflected`, which are outside every book and the fingerprint.
- Public API: `albedo`, `ALBEDO_{DEN,ICE,WATER}` re-exported from `chem`. `energy_input_pass` now returns
  `(injected, reflected)`.

**Tests (`chem/tests_p6.rs`, 4 new and 2 adjusted).**
- `albedo_ice_reflects_more_than_water_and_water_more_than_rock`: zenith sun, 4096 ticks, 16 per tick on the column
  `[ROCK, top, AIR, AIR]`. Every tick checks books, `energy_drift == 0`, lane drift 0 and `injected + reflected == 16`.
  - Reflected: ice > water > rock = sed = 0.
  - Absorbed means are within 1% of `16·n·(1−a)`.
  - `energy_total` is unchanged. Reflected light never entered the books.
- `albedo_share_is_exact_unbiased_and_never_exceeds_the_quantum`: the share is always the floor or floor+1, its
  means are within 2%, and a dawn quantum of 1 on water is absorbed more than 75% of the time.
- `classic_sun_has_no_albedo`: Classic still puts 16 into ice or water and reflects 0.
- `albedo_full_driven_sky_shell_books_exact_and_reflects_off_water`: full Driven sky shell over 700 ticks. Books are
  exact every tick, and `0 < reflected < injected`.
- Adjusted: `sun_heats_a_wet_surface_and_stops_at_ice_and_sediment` (asserts `injected + reflected = 16`, SED still
  takes 16, and the rock below is shaded). `an_all_air_column_lets_the_light_through_unbooked` now also asserts
  `reflected_q == 0`, so AIR pass-through is not reflection.
- Results (`--release`): core 255 passed (3 ignored), wasm 74 passed, `chem --include-ignored` 88 passed, including the
  p6 and Classic goldens. Log: `scale_measure/albedo/tests/final_tests.txt`.

**Soaks (r12, M1 core_r 0, Driven, seed 1, 300k).**
- bl = the whole-column plume ship, run from the knob-scratch tree (`XK_PWHOLE=1 XK_PHEAD=1 XK_PFLANK=1 XK_PNB=4`).
  Its relief lines through 35k are identical to the §5m `wfd1nb4` soak.
- al = albedo.
- Both binaries print the new `ice_cells`, `water_cells`, `ice_top_q_raw`, `input_q_tot` and `reflected_q_tot`.
- All 8 runs: book_fail 0, driftE 0, driftL all 0.
- Values are 250–300k block means. rsun% = reflected / (sun reaching non-AIR), with core heat excluded.

| 250–300k | rocky H8 bl → al | ocean H8 bl → al | rocky H0 bl → al | ocean H0 bl → al |
|---|---|---|---|---|
| rsun% (sun reflected) | 0 → 0.79 | 0 → **6.35** | 0 → 0.59 | 0 → **4.12** |
| σ_h | 3.90 → 3.78 | 2.60 → **2.83** | 0.51 → 0.50 | 2.28 → 2.34 |
| land_r_std | 2.81 → 2.73 | 2.01 → 2.16 | 0.47 → 0.46 | 1.98 → 2.02 |
| max_h / above_datum | 8 / 745 → 8 / 714 | 4.0 / 49.6 → 3.9 / 63.8 | 0 / 0 | 0 / 0 |
| ice cells | 10.7 → 13.5 | 280 → **425** (+52%) | 3.0 → 3.0 | 134 → **188** (+40%) |
| ice tops | 4.0 → 4.0 | 3.6 → **6.5** | 0 → 0 | 0.4 → **2.2** |
| water cells | 55 → 59 | 995 → 1010 | 27 → 32 | 612 → 746 |
| vapor cells | 110 → 103 | 1,223 → **1,000** (−18%) | 108 → 106 | 1,783 → **1,558** (−13%) |
| evap (block total) | 10.5k → 9.9k | 106.3k → **88.8k** (−16%) | 10.4k → 10.4k | 140.7k → **126.8k** (−10%) |
| froze / melted | 1.98k / 1.96k → 2.59k / 2.56k | 24.8k / 24.7k → 32.4k / 32.2k | 83 / 83 → 83 / 83 | 2.85k / 2.85k → 7.0k / 7.0k |
| water_top_q | 460 → **413** | 670 → **594** (−11%) | 683 → 631 | 826 → **701** (−15%) |
| land (ground) top_q | 657 → 656 | 497 → 467 | 534 → 514 | 586 → 519 |
| top_q (all tops) | 641 → 637 | 550 → 504 | 543 → 522 | 671 → 581 |
| win_ms/step | 2.46 → 2.44 | 2.54 → 2.54 | 1.93 → 1.99 | 2.27 → 2.27 |

- `ice_top_q_raw` (mean Q of exposed ice tops) stays near 0–7 in both. Ice tops sit at the freezing floor, so the effect
  shows up as *more and longer-lived ice*, not as a colder ice Q.
- Ocean H8 ice cells by block, bl → al: 380/313/312/280/268/280 → 538/451/445/419/407/425. The ice sheet loses
  ground more slowly and holds about 145 more cells at the end.

**Dome/rim (`sky_probe`, rocky H8; ship = §5m probes; al = `albedo/probes/al_rocky_sky8_{100000,300000}.txt`).**

| | dome mean / σ (ρ≤4) | σ_out (ρ>4) | ceiling cols (rim) | sigma_3x3 | max_step / steps>3 |
|---|---|---|---|---|---|
| 100k ship → al | −2.31 / 3.22 → −2.37 / 3.85 | 3.43 → 3.35 | 0 → **0** | 2.85 → 2.83 | 10 / 107 → 10 / 113 |
| 300k ship → al | −2.20 / 3.66 → −2.08 / 3.98 | 3.84 → 3.62 | 3 → 3 (rim: (11,0) (11,4) (12,0)) | 3.31 → 3.13 | 12 / 130 → 8 / 141 |

**Reading.**
- **Pass: albedo goal met.**
  - Ice reflects most of its sun, water some, and land none, all checked in tests.
  - On water-rich worlds about 4–6% of arriving sun now goes back to space. The oceans run cooler (water-top Q
    −11 to −15%) and evaporation drops 10–16%. The vapor pool shrinks 13–18%, and ice persists: +40–52% ice cells,
    with ice tops ×1.8 (H8) and ×5.5 (H0).
  - This is the expected ice-albedo feedback: ice that survives keeps reflecting, so more of it survives. The
    feedback is bounded here, not runaway. Ice cells still decline slowly over the run.
  - Freeze/thaw cycling rises (ocean H0 2.85k → 7.0k per 50k), because more cells sit near the freezing line.
- **Books exact** everywhere: tests, all 8 soaks and both probes. `reflected_q` is purely diagnostic.
- **No dome or ceiling regression.** The 100k ceiling count is 0, as in ship. At 300k there are 3 rim columns, the
  same count as ship's SED ledge (§5m). The subsolar/polar basin stays a basin (dome mean −2.08 to −2.37).
- **σ_h:**
  - Ocean H8 rises +0.23 (2.60 → 2.83), and above_datum rises 49.6 → 63.8. Ocean H0 rises +0.06.
  - Rocky H8 dips 0.12 (3.90 → 3.78), and rocky H0 is flat.
  - Rocky worlds reflect less than 1% of their sun, so the rocky change is within single-seed trajectory
    divergence. Albedo does not drive relief there.
  - The ocean gain plausibly comes from less evaporation and more standing ice, which change where scour and
    entrainment act. This is a single seed, so treat it as a lean, not a law.
- Cost: none measurable (win_ms equal within noise).

**Risks / next.**
- Single seed, r12. The rocky σ dip (−0.12) needs multi-seed confirmation before it is called noise.
- The constants are physically flavoured but fixed. Grazing-angle Fresnel for water (higher at low sun) and a
  darker-land albedo (~0.1–0.25) are natural extensions. Land at 0 keeps the §5l land energy budget unchanged.
- Snow is not a separate material. ICE stands in for snow/sea ice at 0.60.
- The ice-albedo feedback could run away on colder settings, such as a lower `sun` or a weaker core. The current
  M1 soaks show bounded growth.
- Next options:
  - (a) lithification of SED (cures the rim ledge);
  - (b) angle-dependent water albedo (Fresnel-flavoured, bright at dawn and dusk);
  - (c) land albedo by material (SED/GYP brighter than ROCK);
  - (d) multi-seed H16 soaks of plume + albedo;
  - (e) greenhouse coupling of the vapor pool to the leak (now that albedo cools, vapor could trap heat back);
  - (f) M2 plate advection.

**Files.**
- Code: `chem/p6.rs` (`ALBEDO_DEN/ICE/WATER`, `albedo`, `absorbed_share`, Driven branch of `energy_input_pass`
  → `(injected, reflected)`, `StepReport.reflected`, `EnergyInput` doc). `chem.rs` adds the
  `ChemWorld.reflected_q` field, the `reflected_q()` accessor and the re-exports. `chem/tests_p6.rs` has 4 new
  tests, 2 adjusted, and the helper `albedo_column`. `p6_rest.rs` is unchanged.
- Measurement: `/workspace/scale_measure/albedo/`:
  - `soaks/{al,bl}_r12_{rocky,ocean}_sky{0,8}_300k.txt`, `blocks2.py` and `blocks_summary.txt`
  - `probes/al_rocky_sky8_{100000,300000}.txt`, `bin/` (`chem_soak_al`, `chem_soak_bl`, `sky_probe_al`) and `tests/`
  - `scale_measure/src/bin/chem_soak.rs` and `uplift_caps/exp/sm/src/bin/chem_soak.rs` gained the ice/water/reflected
    `sun:` fields.
- Backups: `/workspace/gameladder_chem_findings/backups_albedo/{pre,post}/`, with `post/*.diff` and both `www` sets.
- www rebuilt (`RUSTFLAGS="-C target-feature=+simd128" CARGO_PROFILE_RELEASE_PANIC=abort`, wasm32 release, then
  `build_pages.py`, embed == sidecar). wasm ABI stays 17. md5s:
  - wasm `38eb89264ded57af3c102107d68830e7` (1,310,304 B)
  - play.html `914d3f8d6611a0fae0a76c4c731a91d6` (1,958,177 B)
  - bitblock-lab.html `df93cb951e5810c15f2c0217116897f6` (1,985,472 B)
  - bitblock.html `814320ae…` (unchanged)
  - Served at http://127.0.0.1:8765 (`python3 -m http.server -d crates/wasm/www`), and the served bytes match. Chrome and
    Desktop have not been checked yet.

## 5o. Vapor greenhouse: the vapor pool traps part of the outbound IR (2026-10-27, uncommitted; Owen pick "greenhouse coupling of the vapor pool to the leak", §5n option e) — ocean warms back, ice-lock eased, no runaway, books exact

**Goal.** Balance the §5n albedo cooling. The aim was a warmer, less ice-locked ocean than albedo alone, with no
runaway. Rocky worlds should be mostly unchanged, and the books should stay exact.

**Change (Driven only; Classic and `Greenhouse::Off` stay planet6 bit-for-bit; Soft DDA untouched).**
- `vapor_trap(vapor, rim)` gives a share in 1/1024ths: `MAX·c/(c + HALF)` with `c = vapor·1024/rim`. Here `rim` is the
  number of cells that have at least one space face.
  - **`GH_VAPOR_MAX = 512`** (at most 0.50 of outbound IR is held back).
  - **`GH_VAPOR_HALF = 4096`** (half of MAX at 4 vapor pairs per rim cell).
  - The curve saturates and is all-integer. A dry sky traps about 0.
- `heat_pass` runs the leak through the old CO2 factor (planet6) first. When `gh_vap > 0`, only
  `absorbed_share(leak, gh_vap, …)` of each rim cell's leak goes to space. That is the same dithered, unbiased integer
  split that albedo uses.
- **Back-radiation.** The held-back part leaves the rim cell, but goes downward. It lands on the first non-AIR cell
  inward from one of the cell's space faces (`sun_target`; face order rotates with tick and cell). If every column is
  all AIR, or the target would overflow, it stays on the rim cell.
  - The rim cell sheds exactly what it would have shed with no vapor. So the greenhouse warms the ground, not the top
    of the sky.
- **Books.** Heat only moves between cells, and nothing is minted. `space_q` gets only the part that escapes, so
  `energy_drift == 0` with no new ledger term.
  - The trapped amount is a diagnostic only: `StepReport.trapped` and `ChemWorld::trapped_q()`. It is outside every
    book and the fingerprint.
- Reflected albedo light is still never injected. The greenhouse acts on outbound IR only, not on reflected sun.
- `heat_pass` now returns `(leaked, trapped)`. The `Greenhouse` enum (`Off | Planet6 | Rational`, default `Planet6`)
  now gates both the CO2 factor and the vapor trap.
- Public API: `vapor_trap` and `GH_VAPOR_{DEN,MAX,HALF}` are re-exported from `chem`.
- **Tuning** (`scale_measure/greenhouse/tune*/`, 60–100k r12 soaks):
  - HALF 1024 and 2048 overheat the H=0 ocean well past its pre-albedo state.
  - HALF 4096 brings ocean H=0 ice back to its pre-albedo level and pulls ocean H=8 about halfway back.

**Tests (`chem/tests_p6.rs`, 6 new).**
- `vapor_trap_is_zero_dry_saturates_and_scales_with_the_column`
- `vapor_greenhouse_holds_back_leak_without_minting_heat`
- `vapor_greenhouse_back_radiates_onto_the_ground_under_open_air`
- `a_thin_vapor_column_barely_traps`
- `classic_and_greenhouse_off_have_no_vapor_greenhouse`
- `greenhouse_full_driven_sky_shell_books_exact_traps_and_albedo_still_reflects`
- Helper: `gh_plate`.
- Results (`--release`): core 261 passed (3 ignored), wasm 74 passed, `chem --include-ignored` 94 passed (incl. p6 and
  Classic goldens). Log: `scale_measure/greenhouse/tests/final_tests.txt`.

**Soaks (r12, M1 core_r 0, Driven, seed 1, 300k).**
- bl = pre-albedo (§5m ship), al = albedo (§5n), gh = albedo + vapor greenhouse.
- All 4 gh runs: END book_fail 0, driftE 0, driftL all 0.
- Values are 250–300k block means. trap% = trapped / (trapped + leaked to space).

| 250–300k | ocean H0 bl / al / **gh** | ocean H8 bl / al / **gh** | rocky H0 bl / al / **gh** | rocky H8 bl / al / **gh** |
|---|---|---|---|---|
| trap% (IR held back) | 0 / 0 / **11.4** | 0 / 0 / **5.1** | 0 / 0 / 0.82 | 0 / 0 / 0.48 |
| rsun% (sun reflected) | 0 / 4.12 / 3.35 | 0 / 6.35 / 5.99 | 0 / 0.59 / 0.60 | 0 / 0.79 / 0.82 |
| σ_h | 2.28 / 2.34 / 2.44 | 2.60 / 2.83 / **2.62** | 0.51 / 0.50 / 0.48 | 3.90 / 3.78 / **3.57** |
| above_datum | 0 / 0 / 0 | 49.6 / 63.8 / 45.1 | 0 / 0 / 0 | 745 / 714 / 677 |
| ice cells | 134 / 188 / **138** | 280 / 425 / **381** | 3.0 / 3.0 / 3.0 | 10.7 / 13.5 / 11.5 |
| ice tops | 0.4 / 2.2 / **0.0** | 3.6 / 6.5 / 3.7 | 0 / 0 / 0 | 4.0 / 4.0 / 3.8 |
| water cells | 612 / 746 / 578 | 995 / 1010 / 992 | 27 / 32 / 30 | 55 / 59 / 58 |
| vapor cells | 1,783 / 1,558 / **1,829** | 1,223 / 1,000 / **1,114** | 108 / 106 / 107 | 110 / 103 / 103 |
| evap (block total) | 140.7k / 126.8k / **142.8k** | 106.3k / 88.8k / **97.7k** | 10.4k / 10.4k / 10.4k | 10.5k / 9.9k / 9.7k |
| froze (≈ melted) | 2.85k / 7.0k / **5.3k** | 24.8k / 32.4k / 29.6k | 83 / 83 / 83 | 1.98k / 2.59k / 2.52k |
| water_top_q | 826 / 701 / **930** | 670 / 594 / **637** | 683 / 631 / 629 | 460 / 413 / 420 |
| land (ground) top_q | 586 / 519 / 699 | 497 / 467 / 505 | 534 / 514 / 529 | 657 / 656 / 679 |
| win_ms/step | 2.27 / 2.27 / 2.01 | 2.54 / 2.54 / 2.54 | 1.93 / 1.99 / 1.74 | 2.46 / 2.44 / 2.44 |

- Ocean H8 ice cells by 50k block:
  - bl: 380 / 313 / 312 / 280 / 268 / 280
  - al: 538 / 451 / 445 / 419 / 407 / 425
  - gh: 492 / 405 / 399 / 384 / 372 / 381
- Ocean H0 vapor by block (gh): 1,679 / 1,837 / 1,833 / 1,826 / 1,822 / 1,829. It plateaus by 100k, so the
  vapor → trap → evap loop is **bounded, not runaway**. trap% is flat at 11.4–11.5% from 50k on.

**Dome/rim (`sky_probe`, H8; `greenhouse/probes/`; albedo values from §5n).**

| | dome mean / σ (ρ≤4) | σ_out (ρ>4) | ceiling cols | sigma_3x3 | max_step / steps>3 |
|---|---|---|---|---|---|
| rocky 100k al → gh | −2.37 / 3.85 → −1.06 / 2.85 | 3.35 → 3.23 | 0 → **0** | 2.83 → 2.55 | 10 / 113 → 9 / 107 |
| rocky 300k al → gh | −2.08 / 3.98 → −2.08 / 3.27 | 3.62 → 3.49 | 3 → **1** (rim (12,0)) | 3.13 → 2.92 | 8 / 141 → 12 / 119 |
| ocean 100k / 300k gh | −3.86 / 3.18; −4.06 / 3.12 | 2.47; 2.54 | 0; 0 | 1.54; 1.61 | 10 / 149; 9 / 156 |

**Reading against "balance albedo cooling".**
- **Pass: the ocean is warmer and less ice-locked than with albedo alone.**
  - Ocean H0 swings fully back. Ice is 188 → 138 (pre-albedo 134), and ice tops go to 0. Vapor and evap return to
    slightly above pre-albedo (+3% and +1.5%).
  - The H0 ocean surface runs **warmer than pre-albedo**: water-top Q 930 against 826 (+13%), and ground 699
    against 586. The sky-less H0 ocean traps the most (11.4%).
  - Ocean H8 recovers about half: water-top Q is 57% of the way back, vapor 52% and evap 51%. Ice recovers 30%
    (425 → 381, against 280 pre-albedo). Freeze/thaw cycling eases.
- **Pass: no runaway.** Vapor, trap% and ice are flat from about 100k to 300k on every run, and trap% stays far
  below the 50% cap.
- **Pass (with a caveat): rocky is mostly unchanged.** Rocky traps less than 1%. On rocky H0 every column matches
  albedo within noise.
  - Rocky H8 σ_h dips 3.78 → 3.57, and above_datum 714 → 677. The same −0.2 dip appeared for albedo (§5n). With 0.5%
    trap this is most likely single-seed trajectory divergence, but it is now two dips in a row and needs a
    multi-seed check.
  - The ceiling count improves (3 → 1 at 300k) and stays 0 at 100k.
- **Pass: books exact.** Tests, all 4 soaks and the probes all hold. Albedo still reflects (`greenhouse_full_driven_…`
  asserts both).
- **σ_h:** the albedo ocean H8 gain (2.60 → 2.83) is mostly undone (2.62). The relief lean in §5n tracked the extra
  ice and dry sky, and the greenhouse removes about half of each.
- Cost: none measurable.

**Risks / next.**
- Single seed, r12. Both the rocky H8 σ dip and the tuned HALF need multi-seed and r16 checks.
- The H0 ocean now overshoots pre-albedo warmth (+13% water-top Q). With a weaker sun or a stronger core, HALF may
  need to depend on sky depth, or MAX may need lowering.
- The column density uses `vapor / rim`, a global scalar. There is no local humidity, so a dry desert under an ocean
  planet's sky traps as much as the ocean does. Spatial vapor (§7) would fix this.
- There is no cloud albedo yet. Real vapor also forms bright clouds, which cool, so the warming is one-sided.
- Next options:
  - (a) cloud albedo from the vapor pool (balances the greenhouse);
  - (b) multi-seed H8/H16 soaks of albedo + greenhouse;
  - (c) lithification of SED;
  - (d) angle-dependent water albedo;
  - (e) M2.

**Files.**
- Code: `chem/p6.rs` (`GH_VAPOR_{DEN,MAX,HALF}`, `vapor_trap`, `heat_pass` → `(leaked, trapped)` with
  back-radiation, `StepReport.trapped`, `Greenhouse` doc). `chem.rs` adds `trapped_q` and the re-exports.
  `chem/tests_p6.rs` has 6 tests and `gh_plate`.
- Measurement: `/workspace/scale_measure/greenhouse/`:
  - `soaks/gh_r12_{ocean,rocky}_sky{0,8}_300k.txt`, `blocks3.py` and `blocks_summary.txt` (gh + al + bl)
  - `probes/gh_{rocky,ocean}_sky8_{100000,300000}.txt`, `tune/` and `tune2/` (HALF sweeps)
  - `rim_*.txt`, `bin/` (`chem_soak_gh_final`, `sky_probe_gh`) and `tests/`
- Backups: `/workspace/gameladder_chem_findings/backups_greenhouse/{pre,post}/`, with `post/*.diff` and both `www` sets.
- www rebuilt (same flags as §5n). wasm ABI stays 17. md5s:
  - wasm `74718ff33cc6b90072fe93e67c86c22c` (1,311,937 B)
  - play.html `318ea305357c2466be9d99ef214f72f4`
  - bitblock-lab.html `cca6a11d3475747dbc4055be25bec162`
  - bitblock.html `814320ae…` (unchanged)
  - Served at http://127.0.0.1:8765, and the served bytes match. Chrome and Desktop have not been checked yet.

## 5p. Angle-dependent water albedo: Fresnel-like reflection off open water (2026-10-27, uncommitted; §5o option d) — high sun absorbed more, glancing sun reflected more, no runaway, books exact

**Goal.** Replace the fixed 0.15 water albedo (§5n) with one that depends on the sun angle. High sun should be absorbed
more, glancing sun reflected more. Ice stays at 0.60, reflected light is still never injected, and the books stay exact.

**Change (Driven sun only; Classic has no albedo and is unaffected; `WaterAlbedo::Fixed` reproduces §5n/§5o bit-for-bit).**
- New `P6Params.water_albedo: WaterAlbedo` (`Fixed | Fresnel { nadir, graze, pow }`). The default is
  **`WaterAlbedo::FRESNEL`** = `WATER_ALBEDO_NADIR = 61` (0.06), `WATER_ALBEDO_GRAZE = 1024` (1.0) and
  `WATER_ALBEDO_POW = 5`.
- `water_albedo(mu, model)` is a Schlick-style curve, all-integer:
  `a(μ) = nadir + floor((graze − nadir)·x/1024)`, where `x = (1024 − μ)^pow` is taken by repeated floor-multiplication
  in 1/1024ths. It is monotone, equals `nadir` at μ = 1024 and equals `graze` at μ = 0.
- `albedo_at(m, mu, model)` uses `water_albedo` for WATER and the fixed `albedo(m)` for everything else (ICE 614,
  land and gas 0).
- In `energy_input_pass`, `μ = min(1024, d·1024/|s|)`, where `d` is the face's sun dot product and
  `|s| = isqrt(sx² + sy²)` (724–1024 for the triangle-wave sun).
  - **Deviation from the hypothesis.** μ uses the normal of the face the ray enters, not a radial normal. Driven
    fluids settle under world −y gravity, so a water free surface is axis-aligned (a pond top is a +y face), and the
    ray rides that face's normal. On a pond top, μ is the sine of the sun's elevation.
- **Books.** Unchanged: `absorbed_share` still splits each quantum with dither, the absorbed part is booked to
  `input_q`, and the reflected part goes to `reflected_q` only.
- **Diagnostic.** `ChemWorld::water_sun_q()` returns `[inc_hi, refl_hi, inc_lo, refl_lo]` for water targets, binned
  at μ ≥ 0.5 (high sun) vs μ < 0.5 (low sun). It is outside the books and the fingerprint.
  - Re-exported from `chem`: `water_albedo`, `albedo_at`, `WaterAlbedo` and `WATER_ALBEDO_{NADIR,GRAZE,POW}`.
- Flux-weighted over the M1 day this averages about 0.08, below the fixed 0.15, because grazing quanta are mostly
  truncated (`f = 16·d/1024` is 0 for d < 64). So the default is net-warming as well as angle-shaped.
  - A mean-preserving variant, `Fresnel{113, 1024, 3}`, averages about 0.15 and was soaked as a sensitivity check.

**Tests (`chem/tests_p6.rs`, 5 new; `albedo_column` pinned to `Fixed` via `albedo_column_wa`).**
- `water_albedo_curve_is_fresnel_like_monotone_and_bounded`
- `zenith_water_absorbs_more_under_fresnel_and_ice_is_unchanged`
- `glancing_sun_reflects_more_off_water_over_a_day_and_never_mints` (`day_column`, books every tick, Q sum = 200 + absorbed)
- `classic_ignores_the_water_albedo_model`
- `fresnel_full_driven_sky_shell_books_exact_and_bins_water_light`
- Results: chem 99/99, core 269/269 (incl. goldens), wasm 74/74.

**Soaks (`scale_measure/water_albedo/`, r12, seed 1, 300k, m1 driven; 250–300k block means; gh = §5o).**

| 250–300k | ocean H0 gh | wa | mp | ocean H8 gh | wa | rocky H0 gh | wa | rocky H8 gh | wa |
|---|---|---|---|---|---|---|---|---|---|
| σ_h | 2.44 | 2.44 | 2.35 | 2.62 | 2.82 | 0.48 | 0.51 | 3.57 | 3.64 |
| ice cells | 138 | 111 | 134 | 381 | 303 | 3.0 | 3.0 | 11.5 | 10.9 |
| vapor | 1829 | 1920 | 1820 | 1114 | 1251 | 106.5 | 109.5 | 102.8 | 111.9 |
| evap /50k | 142.8k | 152.4k | 144.6k | 97.7k | 107.8k | 10.4k | 10.8k | 9.7k | 10.6k |
| water-top Q | 930 | 961 | 917 | 637 | 667 | 629 | 672 | 420 | 439 |
| rsun% | 3.35 | 1.70 | 3.27 | 5.99 | 3.27 | 0.60 | 0.30 | 0.82 | 0.43 |
| trap% | 11.4 | 11.9 | 11.4 | 5.1 | 5.8 | 0.82 | 0.85 | 0.48 | 0.53 |
| water albedo hi / lo / all | 0.15 | 0.061 / 0.230 / 0.080 | 0.119 / 0.405 / 0.152 | 0.15 | 0.061 / 0.229 / 0.078 | 0.15 | 0.061 / 0.229 / 0.080 | 0.15 | 0.061 / 0.228 / 0.080 |

- About 88–90% of the light reaching water arrives at μ ≥ 0.5.
- **Control:** a `walb=fixed` ocean H0 run reproduces `gh_r12_ocean_sky0_300k.txt` line for line over all 300k ticks
  (timing excluded).

**Verdict (single seed, r12).**
- **Pass: high-sun water absorbs more.** Effective albedo is 0.061 vs 0.15.
- **Pass: glancing water reflects more.** The low-sun bin is 0.23 vs 0.15, and the curve reaches 1.0 at grazing.
- **Pass: no runaway.**
  - Ocean vapor block means from 50k to 300k: H0 1866 → 1920 (+3%), H8 1182 → 1251 (+6%, with shrinking increments).
  - Water-top Q is flat.
- **Pass: books exact.** Every run ends with `book_fail_total=0`, `driftE=0` and `driftL=0`.
- **Pass: rocky mostly unchanged.**
  - σ_h: 0.48 → 0.51 and 3.57 → 3.64.
  - Ice is unchanged within noise. Water-top Q is +4–7% on the few ponds.
- **Ocean effect.** The default warms the ocean.
  - Ice: H0 −20%, H8 −21%. Evap: +7% and +10%.
  - σ_h H8 rises 2.62 → 2.82, back to the §5n albedo-only level.
- The mean-preserving `mp` run lands on gh (ice 134 vs 138, evap +1%). So the warming comes from the lower mean
  albedo, and the angle shape alone is close to climate-neutral at r12.

**Risks / next.**
- H0 ocean warmth now overshoots pre-albedo by more than §5o did (water-top Q 961 vs 930 gh). If that matters,
  raise nadir to about 0.12 or use pow 3 (mp).
- μ uses the axis-aligned entry face, so a staircase coast sees the same μ as a flat sea. There is no wave or
  roughness term.
- Single seed, r12.
- Next options:
  - (a) cloud albedo from the vapor pool;
  - (b) multi-seed / r16 soaks of albedo + greenhouse + water angle;
  - (c) retune to mean-preserving (`mp`) if warming is unwanted;
  - (d) angle-dependent ice/snow albedo;
  - (e) lithification of SED;
  - (f) M2.

**Files.**
- Code: `chem/p6.rs` (`WaterAlbedo`, `WATER_ALBEDO_*`, `water_albedo`, `albedo_at`, `P6Params.water_albedo`,
  `energy_input_pass` μ). `chem.rs` adds `water_sun`/`water_sun_q` and the re-exports. `chem/tests_p6.rs` has
  5 tests plus `albedo_column_wa` and `day_column`.
- Measurement: `/workspace/scale_measure/water_albedo/`:
  - `soaks/{wa,mp,fixed}_r12_*_300k.txt`, `blocks4.py`, `blocks_summary.txt` and `blocks_full.txt`
  - `bin/chem_soak_wa`
  - `tests/`
  - `chem_soak` arg 8 `walb=fresnel|fixed|N,G,P`, and a `water_sun=[..]` field on the `sun:` line.
- Backups: `/workspace/backups_water_albedo/{pre,post}/`, with `post/*.diff` and both `www` sets.
- www rebuilt (same flags as §5n). wasm ABI stays 17. md5s:
  - wasm `52521a8591d1b47b6af4d1b75a114124` (1,312,566 B)
  - play.html `ac5a7be2fb8675725ed8717886296270` (1,961,490 B)
  - bitblock-lab.html `fbe34b60879761604fe944a34aae70ab` (1,988,785 B)
  - bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (2,369 B, unchanged)
  - Served at http://127.0.0.1:8765, and the served bytes match. Chrome and Desktop have not been checked yet.

## 5q. Cloud albedo from the vapor pool: clouds reflect sun before the surface (2026-10-28, uncommitted; §5p option a) — oceans cooler than §5p, near the §5o greenhouse baseline, no runaway, books exact

**Goal.** Give the vapor greenhouse (§5o) a cooling partner. More vapor should reflect more shortwave before it reaches
the surface, offsetting the extra warming from the Fresnel water albedo (§5p). Reflected light is still never injected
or booked, and the books stay exact.

**Change (Driven sun only, and only with a greenhouse on; Classic has no albedo and is unaffected; `CloudAlbedo::Off`
reproduces §5p bit-for-bit).**
- New `P6Params.cloud: CloudAlbedo` (`Off | Vapor { max, half }`). The default is **`CloudAlbedo::VAPOR`** =
  `CLOUD_MAX = 256` (0.25 of the sun at most) and `CLOUD_HALF = 4096` (1/1024 pairs per rim cell, the same column
  unit as `GH_VAPOR_HALF`).
- `cloud_reflect(vapor, rim, model)` is a saturating curve on the same column density as the greenhouse:
  `c = vapor·1024/rim` (rim = cells with a space face) and `cloud = max·c/(c + half)` in 1/1024ths, computed in i128
  and clamped to `0..=min(max, 1024)`. It is 0 when Off, dry, rim = 0 or max = 0.
- `energy_input_pass` computes `cloud` once per tick (global vapor pool, like the greenhouse). It is gated on
  `cycle == Driven && greenhouse != Off`, mirroring the greenhouse gate. Each sun ray quantum `f` first passes the
  cloud, `fc = absorbed_share(f, cloud, tick, cell, face + 8)`, and then the surface albedo,
  `a = absorbed_share(fc, albedo_at(..))`. Only `a` is injected and booked in `input_q`; `f − a` (cloud plus surface) is
  added to the `reflected_q` diagnostic.
- Diagnostics outside books and fingerprint: `cloud_q()` (sun reflected by clouds, which is part of `reflected_q`) and
  `cloud_last()` (this tick's cloud fraction /1024). The `water_sun` bins now count the light under the clouds.
- Tuning: the sweep was `max` 256 / 384 / 512 with half 4096, on ocean H0 and H8 to 300k. 256 lands the oceans near the
  §5o greenhouse baseline (pre-Fresnel). 384 and 512 cool further but stay bounded, so 256 is the conservative default.

**Tests (4 new).**
- `cloud_reflect_is_zero_dry_saturates_and_scales_with_the_column`
- `vapor_clouds_reflect_sun_before_the_surface_and_never_mint` (helper `cloud_column`)
- `cloud_albedo_off_classic_and_greenhouse_off_leave_the_sun_unchanged`
- `cloud_albedo_full_driven_sky_shell_books_exact_and_clouds_reflect`
- Results: core 273/273 with `--include-ignored` (269 + 4), chem:: filter 101/101, wasm 74/74.
- Control: `cloud=off` ocean H0 30k reproduces `wa_r12_ocean_sky0_300k.txt` line for line, excluding timing.

**Soaks** (r12, seed 1, 300k, m1 driven, `walb=fresnel`; 250–300k block means; `ca` = default cloud, `wa` = §5p).

| 250–300k | ocean H0 wa | ocean H0 ca | ocean H8 wa | ocean H8 ca | rocky H0 wa | rocky H0 ca | rocky H8 wa | rocky H8 ca |
|---|---|---|---|---|---|---|---|---|
| σ_h | 2.44 | 2.37 | 2.82 | 2.75 | 0.51 | 0.48 | 3.64 | 3.89 |
| ice cells | 110.7 | 142.9 | 302.7 | 348.6 | 3.0 | 3.0 | 10.9 | 11.2 |
| vapor | 1,920 | 1,786 | 1,251 | 1,108 | 110 | 108 | 112 | 105 |
| evap /50k | 152k | 142k | 108k | 98k | 10.8k | 10.5k | 10.6k | 9.9k |
| froze /50k | 3,357 | 5,187 | 22,991 | 29,321 | 83 | 83 | 2,019 | 2,288 |
| water-top Q | 961 | 852 | 667 | 629 | 672 | 668 | 439 | 423 |
| cloud (/1024) | 0 | 61.5 | 0 | 32.3 | 0 | 4.9 | 0 | 3.0 |
| cloud % of sun | 0 | 5.90 | 0 | 3.05 | 0 | 0.42 | 0 | 0.28 |
| rsun % (cloud + surface) | 1.70 | 7.77 | 3.27 | 6.89 | 0.30 | 0.71 | 0.43 | 0.70 |
| trap % | 11.88 | 11.14 | 5.76 | 5.10 | 0.85 | 0.83 | 0.53 | 0.49 |

- Reference §5o greenhouse values (fixed 0.15 water): ocean H0 ice 138 / water-top Q 930, H8 381 / 637.
- Sweep, same block, ice / water-top Q: max 384 gives H0 155 / 783 and H8 372 / 614; max 512 gives H0 174 / 727 and
  H8 389 / 618.
- Books: END `book_fail_total=0 max_abs_driftE=0 driftL=[0;12]` on all 11 runs (4 final, 6 sweep, 1 control).

**Verdict.**
- **Pass:** the oceans are cooler than §5p. Ocean H0 ice goes 111 → 143 (+29%) and water-top Q 961 → 852. Ocean H8
  ice goes 303 → 349 (+15%) and Q 667 → 629. Evap falls 7–9%. This lands near the §5o greenhouse baseline, so the
  Fresnel warming is offset.
- **Pass:** no runaway ice lock. Ice per 50k block falls after spin-up and flattens: H0 223 170 153 146 143 143, H8 465
  368 365 353 337 349. Vapor and cloud are flat from 100k. Even max 512 stays bounded (H8 389).
- **Pass:** books are exact.
- **Mostly pass:** rocky is barely affected. Its cloud is under 0.5% of the sun because rocky vapor is about 108 per
  about 1,400 rim cells. Rocky H0 is unchanged. Rocky H8 ice is unchanged (10.9 → 11.2), but σ_h moves 3.64 → 3.89 and
  water-top Q 439 → 423. That is likely single-seed divergence.
- The feedback is negative, as intended: cooler oceans mean less vapor, which thins the cloud (H0 vapor −7%).

**Risks / next.**
- Vapor is still one global pool, so the cloud is uniform over the planet. Deserts are as cloudy as open ocean, and
  there is no day/night or latitude structure.
- Cloud and greenhouse read the same column density but have no shared optical-depth model. The ratio of cloud cooling
  to greenhouse warming is a tuning choice (`CLOUD_MAX` 256 against `GH_VAPOR_MAX` 512).
- The cloud stage does not depend on the sun angle. A longer slant path would be more realistic.
- Single seed, r12.
- Next options:
  - (a) multi-seed / r16 soaks of the full albedo + greenhouse + water-angle + cloud stack;
  - (b) local clouds, a per-column or per-region vapor pool, so clouds sit over the oceans;
  - (c) slant-path cloud (cloud grows with 1/μ);
  - (d) angle-dependent ice/snow albedo;
  - (e) lithification of SED;
  - (f) M2.

**Files.**
- Code: `chem/p6.rs` (`CloudAlbedo`, `CLOUD_MAX`, `CLOUD_HALF`, `CLOUD_DEN`, `cloud_reflect`, `P6Params.cloud`,
  the `energy_input_pass` cloud stage). `chem.rs` adds `cloud_q`/`cloud_last` and the re-exports. `chem/tests_p6.rs`
  has the 4 tests plus `cloud_column`.
- Measurement: `/workspace/scale_measure/cloud_albedo/`:
  - `soaks/ca_r12_{ocean,rocky}_sky{0,8}_300k.txt`
  - `sweep/ca{256,384,512}_r12_ocean_sky{0,8}_300k.txt` and `sweep/ctrl_off_ocean_sky0_30k.txt`
  - `blocks5.py`, `summarize.py`, `blocks_summary.txt` and `blocks_full.txt`
  - `bin/chem_soak_ca`
  - `tests/`
  - `chem_soak` arg 9 `cloud=vapor|off|MAX,HALF`, and `cloud_q_tot`/`cloud_last` on the `sun:` line.
- Backups: `/workspace/backups_cloud_albedo/{pre,post}/`, with `post/*.diff` and both `www` sets.
- www rebuilt (same flags as §5n). wasm ABI stays 17. md5s:
  - wasm `4dca107fb529539ab23d683cb0e144df` (1,312,945 B)
  - play.html `93139d7a394db8d4ad4aa41652bd7410` (1,962,053 B)
  - bitblock-lab.html `f2352dc5fc2d4dcd24e719342e80e9db` (1,989,348 B)
  - bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (2,369 B, unchanged)
  - Served at http://127.0.0.1:8765, and the served bytes match. Chrome and Desktop have not been checked yet.

## 5r. Local vapor: the vapor pool split into 54 regions so clouds sit over the water (2026-10-28, uncommitted; §5q option b) — oceans still cloud-cooled, water cycle intact after a rain-spill fix, rocky dry ground near-clear, no runaway, books exact

**Goal.** Replace the single global vapor pool (§5o–§5q) with a regional one. Clouds and the vapor greenhouse should sit
where the water evaporates, so deserts are not as cloudy as open ocean. Books stay exact and reflected light is never
injected.

**Change (Driven only; Classic never touches the bins; `VaporLocality::Global` reproduces §5q bit-for-bit).**
- New `chem/vapor.rs` and a new `P6Params.vapor_local: VaporLocality` (`Global | Patches { n, mix_tau }`). The default is
  **`VaporLocality::PATCHES`** = `VAPOR_PATCH_N = 3` and `VAPOR_MIX_TAU = 2048`.
- Regions are a cube-sphere split of directions from the M1 core cell. The dominant axis of `cell − core` picks one of
  6 faces, and the other two components pick an `n × n` tile, so n = 3 gives 54 regions. There is no noise or texture.
- The bins are an **attribution** of the one booked `vapor` count, with the invariant `sum(vapor_bins) == vapor`, so the
  books are untouched by construction. Evaporation and degassing add a pair to the bin of the cell they leave, rain
  removes one from the bin it falls in, and transport only moves pairs between bins.
- Transport (`vapor_mix_pass`): each pair of adjacent regions relaxes its rim-density difference over `mix_tau` ticks.
  Integer moves use stochastic rounding, so the transport conserves exactly.
- Rain is per region: a region rains `bin / RAIN_TAU` (512, unchanged) drops on its own sky-open ground, then its
  waterline.
- **Rain spill (v2, a bug fix found in the first soak).** Rain still uses the −y fall test (AIR open above, resting on
  ground), so regions on the underside and the lower flanks have no landing ground. In v1 their vapor waited forever,
  and the ocean water cycle ran about 3.7× slower (ocean H0 evap 36.5k vs 142k per 50k). In v2 the drops such a region
  cannot land spill onto the planet-wide landing line (the old global pick). The spill is counted in
  `ChemWorld::rain_spilled()`, a diagnostic outside the books. With a single region there is never a spill.
- The cloud (§5q formula, max 256, half 4096) and the vapor greenhouse (§5o) now read the **per-region** column density
  `c = bin·1024/rim_region` at the cell the ray strikes or leaks from.
- Diagnostics outside the books and fingerprint: `vapor_bins()`, `rain_spilled()`, and `patch_diag`, which keeps
  per-region cumulative sun, sun on water/ice, cloud, trap and leak. The soak prints these on a `vloc:` line, with
  `cloud_split = [sun_wet, cloud_wet, sun_dry, cloud_dry]` classed by whether the struck cell is WATER/ICE, plus `BINS` lines.

**Tests (7 new).**
- `vapor_patch_is_a_cube_sphere_split_of_directions`
- `local_vapor_bins_sum_to_the_pool_and_books_stay_exact` (helper `local_sky_world`)
- `global_locality_is_one_bin_and_rescales_on_switch`
- `local_clouds_and_greenhouse_sit_only_over_the_region_holding_vapor`
- `vapor_mix_conserves_and_relaxes_toward_equal_column_density`
- `classic_never_touches_the_local_vapor_bins`
- `local_rain_spills_from_regions_without_landing_ground`
- Results: core 280/280 with `--include-ignored` (273 + 7), chem:: filter 105 passed + 3 ignored, wasm 74/74.
- Control: `vloc=global` ocean H0 30k reproduces `ca_r12_ocean_sky0_300k.txt` line for line, both before and after the
  spill change. Only the extra `vloc:`/`BINS` lines and the header field differ.

**Soaks** (r12, seed 1, 300k, m1 driven, `walb=fresnel cloud=vapor`; 250–300k block means; `ca` = §5q global pool,
`lv` = local vapor v2 default).

| 250–300k | ocean H0 ca | ocean H0 lv | ocean H8 ca | ocean H8 lv | rocky H0 ca | rocky H0 lv | rocky H8 ca | rocky H8 lv |
|---|---|---|---|---|---|---|---|---|
| σ_h | 2.37 | 2.53 | 2.75 | 2.91 | 0.48 | 0.51 | 3.89 | 3.66 |
| ice cells | 142.9 | 183.7 | 348.6 | 365.3 | 3.0 | 3.0 | 11.2 | 8.7 |
| vapor | 1,786 | 1,675 | 1,108 | 986 | 108 | 110 | 105 | 100 |
| evap /50k | 142k | 132k | 98k | 88k | 10.5k | 10.3k | 9.9k | 9.2k |
| water cells | 612 | 644 | 1,011 | 1,110 | 29 | 27 | 59 | 65 |
| water-top Q | 852 | 856 | 629 | 657 | 668 | 685 | 423 | 435 |
| cloud % of sun | 5.90 | 6.18 | 3.05 | 2.90 | 0.42 | 0.71 | 0.28 | 0.20 |
| trap % | 11.14 | 10.56 | 5.10 | 5.30 | 0.83 | 1.29 | 0.49 | 0.64 |
| cloud % over water/ice / other (lv) | — | 10.61 / 4.96 | — | 4.11 / 2.44 | — | 4.34 / 0.58 | — | 1.18 / 0.16 |
| per-rim cloud wet / dry regions (/1024) | — | 105 / 30 | — | 33 / 21 | — | — / 4.0 | — | 38 / 1.4 |
| trap % wet / dry regions | — | 29.8 / 4.3 | — | 11.1 / 4.2 | — | — / 1.3 | — | 6.5 / 0.3 |
| wet regions (of 54) | — | 6.6 | — | 13.2 | — | 0 | — | 2 |
| vapor in wet / dry regions | — | 698 / 977 | — | 321 / 664 | — | 0 / 110 | — | 51 / 49 |

- Here a "wet" region means most of its cumulative sun struck water or ice. In the "over water/ice" row, by contrast,
  each ray is classed by the cell it hits.
- v1 (no spill) for reference, same block: evap/50k was 36.5k / 45.5k / 1.2k / 3.7k (ocean H0, ocean H8, rocky H0,
  rocky H8), and ocean H0 water cells were 289. That run is kept in `v1_stuck/`.
- Spilled drops to 300k: 164k of 775k evaporated (ocean H0), 89k (H8), 16k (rocky H0), 5.5k (rocky H8). The largest
  single bin at 300k is 240 pairs.
- Books: END `book_fail_total=0 max_abs_driftE=0 driftL=[0;12]` on all 4 lv runs, the 4 v1 runs, the 2 v1 variants
  (n2/τ2048 and n3/τ8192) and the A/B controls.

**Verdict.**
- **Pass:** the oceans are still cloud-cooled. Cloud is 6.18% vs 5.90% of the sun (H0) and 2.90% vs 3.05% (H8), and
  water-top Q holds (856 vs 852, 657 vs 629). Ice rises on H0 (143 → 184, +29%) and is +5% on H8.
- **Pass:** the water cycle is intact. With the spill, evap is within 7–11% of §5q on all four runs.
- **Mixed, reported honestly:** rocky cloud/trap is much lower than global. Rocky totals stay about 9× (cloud) and 8×
  (trap) below the oceans, and the dry ground is near-clear: 0.58% / 0.16% of the sun on non-water cells, against 4.34% /
  1.18% over the puddles. But the rocky H0 **total** went *up* against §5q (cloud 0.42 → 0.71%, trap 0.83 → 1.29%),
  because the little vapor it has now piles over its few puddles instead of being spread thin. Rocky H8 cloud fell
  (0.28 → 0.20%) and its trap rose (0.49 → 0.64%).
- **Pass:** no runaway. Vapor, ice and water are flat across the 50–300k blocks on all four runs. Ocean H0 ice per
  block: 265 205 192 188 185 184. H8: 487 399 389 370 355 365.
- **Pass:** books are exact.
- **Wet/dry split is real but moderate.** By struck cell the split is clear (ocean H0: 10.6% vs 5.0%; per-rim cloud
  105 vs 30 over wet vs dry regions). On ocean H8 the per-rim contrast is only about 1.5× (33 vs 21). In pair *count*,
  dry regions hold more vapor than wet ones on both oceans (977 vs 698, 664 vs 321).
- Diagnosis of the split: this is classification and area, not a bug. Only 7–13 of the 54 regions are wet. The unlit
  underside regions count as dry and hold most of the rim, and diffusion (τ 2048) keeps feeding them.
- The early ocean H0 inversion (at 25k: wet 33 vs dry 41 per rim) is spin-up before the wet basins fill. By 145k the
  split is 104 vs 30. No code change was made for this.

**Risks / next.**
- The spill is a global teleport for vapor stranded over underside regions. That is about 21% of ocean H0 rain, and it
  dilutes locality. A radial (core-relative) fall test for rain would remove the need for it.
- The regions are coarse (54 regions, about 26 rim cells each), the rocky world has 0 "wet" regions on H0, and there is
  no wind or day/night advection. The mixing is plain diffusion.
- Ice rises on ocean H0 (+29%). The concentrated wet-region cloud cools the basins that make the vapor, which is
  plausible but tuned on one seed.
- Single seed, r12.
- Next options:
  - (a) a radial rain fall test so underside regions rain locally and the spill goes away;
  - (b) multi-seed / r16 soaks of the full stack with local vapor;
  - (c) sun-side advection (vapor drifts toward the terminator or night side);
  - (d) a mix_tau / n sweep against the wet/dry contrast;
  - (e) slant-path cloud;
  - (f) lithification of SED;
  - (g) M2.

**Files.**
- Code:
  - `chem/vapor.rs` (new: `VaporLocality`, `VAPOR_PATCH_N`, `VAPOR_MIX_TAU`, `vapor_patch`, `vapor_sync`,
    `vapor_bin_add`, `vapor_mix_pass`).
  - `chem/p6.rs`: `P6Params.vapor_local`, plus the per-region cloud/greenhouse, `cloud_split` and `patch_diag`.
  - `chem/p6_rest.rs`: per-region rain and the spill.
  - `chem.rs`: `vapor_bins`, `rain_spilled`, `patch_diag`.
  - `chem/tests_p6.rs`: the 7 tests.
- Measurement: `/workspace/scale_measure/local_vapor/`:
  - `soaks/lv_r12_{ocean,rocky}_sky{0,8}_300k.txt`
  - `v1_stuck/` (v1 300k runs and the n2t2048 / n3t8192 variants) and `ab/` (30k global/patch A/B controls)
  - `summarize_lv.py`, `blocks5.py` and `blocks_summary.txt`
  - `bin/chem_soak_lv2` (v1: `chem_soak_lv`) and `tests/`
  - `chem_soak` arg 10 `vloc=patch|global|N,TAU` and the `vloc:` / `BINS` lines.
- Backups: `/workspace/backups_local_vapor/{pre,post,post2}/`. `post` is v1, and `post2` is v2 with `*_v2.diff`
  against v1 and the old www in `post2/www_before/`.
- www rebuilt after the spill change (same flags as §5n). wasm ABI stays 17. md5s:
  - wasm `0f0eca4539cfb036ab6cb533755b1e51` (1,335,920 B)
  - play.html `7111e971014899bc7fafb374706583d3` (1,995,688 B)
  - bitblock-lab.html `dd903e9a6da92e2f3849ed19941beada` (2,022,983 B)
  - bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (2,369 B, unchanged)
  - Served at http://127.0.0.1:8765, and the served bytes match. Chrome and Desktop have not been checked yet.

## 5s. Rain toward the core: radial fall test plus nearest-ground shed (2026-10-28, uncommitted; §5r option a) — planet-wide spill gone (0 on all four runs), ocean H8 and rocky H8 locality stronger, ocean H0 contrast weaker, no runaway, books exact

**What changed.**
- New `RainFall::{Down, Core}` in `chem/vapor.rs`, with `P6Params.rain_fall` defaulting to `Core`. Driven only. Classic and the 1-region global pool are untouched.
- `Core`: a region's drop lands on a rim cell whose outward step (from the core) is AIR or space and whose inward neighbour is non-AIR. This is the old `-y` test turned radial, so flanks and the underside can now catch their own rain. The waterline fallback is unchanged.
- A region with no landing cell even after the waterline fallback (on the full r12 ball, flank or underside rock with no AIR headroom) **sheds** its drop. It walks region adjacency (BFS over 6-adjacent rim edges, `VaporGeo.adj`, `nearest_ground`) to the nearest region that has ground and lands it there. `rain_shed` counts these drops and `rain_shed_hops` counts the hops (diagnostic, outside the books).
- The planet-wide spill (`rain_spilled`, §5r) now only happens under `Down`, or when no region on the planet has ground. `Down` reproduces §5r line for line (`ab/` controls).
- Books: a shed drop is a vapor → water move at a real landing cell, the same as local rain. The bins are debited from the region that holds the vapor, so `sum(bins) == vapor` holds.
- Not changed: evaporation still uses the world `+y` top test, and flow/settle still use world `-y`.

**Tests.**
- New: `core_rain_finds_more_landing_ground_below_the_equator_than_down_rain` (21 lower regions: 77 → 302 spots), `nearest_ground_walks_region_hops`, `core_rain_keeps_books_and_never_spills_with_transport` (2000 ticks: rained 795, shed 3, spilled 0) and, in wasm, `full_ball_core_rain_sheds_locally_where_down_rain_spills` (r12 ocean seed 1: `Down` spills and never sheds; `Core` has spill 0 and shed > 0; books exact).
- Two tests were pinned to `RainFall::Down`: the §5r spill regression, and the flat plate in `driven_rain_follows_vapor_not_the_calendar_and_spreads`, because a plate has no core frame.
- Results: core 283/283 with `--include-ignored` (+3), chem:: 108 passed + 3 ignored, wasm 75/75 (+1).

**Soaks** (r12, seed 1, 300k, m1 driven, `walb=fresnel cloud=vapor vloc=patch`; 250–300k block means; `lv` = §5r, `rc` = rain=core).

| 250–300k | ocean H0 lv | ocean H0 rc | ocean H8 lv | ocean H8 rc | rocky H0 lv | rocky H0 rc | rocky H8 lv | rocky H8 rc |
|---|---|---|---|---|---|---|---|---|
| σ_h | 2.53 | 2.57 | 2.91 | 3.05 | 0.51 | 0.55 | 3.66 | 3.09 |
| ice cells | 183.7 | 200.8 | 365.3 | 408.4 | 3.0 | 7.0 | 8.7 | 16.0 |
| vapor | 1,675 | 1,630 | 986 | 896 | 110 | 106 | 100 | 76 |
| evap /50k | 132k | 128k | 88k | 80k | 10.3k | 9.9k | 9.2k | 7.2k |
| water cells | 644 | 659 | 1,110 | 1,144 | 27 | 32 | 65 | 76 |
| water-top Q | 856 | 853 | 657 | 648 | 685 | 720 | 435 | 425 |
| cloud % of sun | 6.18 | 5.96 | 2.90 | 2.70 | 0.71 | 0.73 | 0.20 | 0.13 |
| trap % | 10.56 | 10.31 | 5.30 | 4.90 | 1.29 | 1.26 | 0.64 | 0.45 |
| cloud % over water/ice / other | 10.61 / 4.96 | 10.42 / 4.82 | 4.11 / 2.44 | 4.12 / 2.16 | 4.34 / 0.58 | 3.72 / 0.63 | 1.18 / 0.16 | 1.15 / 0.08 |
| per-rim cloud wet / dry regions | 105 / 30 | 81 / 32 | 33 / 21 | 33 / 16 | — / 4.0 | — / 4.0 | 38 / 1.4 | 37 / 0.75 |
| trap % wet / dry regions | 29.8 / 4.3 | 25.2 / 8.4 | 11.1 / 4.2 | 14.9 / 2.8 | — / 1.3 | — / 1.3 | 6.5 / 0.3 | 6.2 / 0.2 |
| wet regions (of 54) | 6.6 | 7.0 | 13.2 | 17.9 | 0 | 0 | 2 | 2 |
| vapor in wet / dry regions | 698 / 977 | 543 / 1,087 | 321 / 664 | 482 / 414 | 0 / 110 | 0 / 106 | 51 / 49 | 49 / 27 |
| spilled (planet-wide) % of rain | 16.4 | **0** | 15.8 | **0** | 22.7 | **0** | 9.3 | **0** |
| shed (local) % of rain, mean hops | — | 23.3, 1.60 | — | 17.3, 1.16 | — | 23.1, 1.17 | — | 10.7, 1.08 |

- Totals to 300k: spilled was 163,672 / 89,388 / 15,583 / 5,488 (lv). It is now 0 on all four rc runs, and shed is 222,074 / 86,673 / 13,655 / 4,554.
- Ice per block, rc: ocean H0 291 224 212 207 203 201; ocean H8 507 447 444 419 403 408; rocky H0 18 9 7 7 7 7; rocky H8 24 14 16 15 12 16. Vapor flattens by 100k on all four runs.
- Books: END `book_fail_total=0 max_abs_driftE=0 driftL=[0;12]` on all four rc runs, the `rcg` run (vloc=global rain=core, ocean H0) and the four `rct8k` runs.
- Control `rcg` (one global region + Core): there is never any shed or spill. Ocean H0 evap is 135k/50k against 142k for §5q, because core landing moves some rain onto the flanks.
- Sensitivity `rct8k` (rain=core, mix τ 8192; not shipped): dry regions get clearer on every world (ocean H0 per-rim dry 32 → 25, ocean H8 16 → 13, rocky H8 0.75 → 0.58). Shed falls on H8 (17 → 12%, 11 → 5%). But ocean H0 per-rim wet cloud stays at 83, not back to 105.

**Verdict.**
- **Pass:** the planet-wide spill is gone, at 0 on all four runs. It is **not** replaced by nothing: 11–23% of rain is shed 1.1–1.6 region hops to the nearest ground. That share is about the same as the old spill, but it is now local rather than a global teleport.
- **Pass:** the oceans are cloud-cooled, and a bit cooler than §5r. Ice is 184 → 201 (H0) and 365 → 408 (H8). Water-top Q holds (853 vs 856, 648 vs 657).
- **Pass:** rocky dry ground is clearer. On rocky H8, cloud over dry cells is 0.16 → 0.08% and dry-region per-rim cloud is 1.4 → 0.75. Rocky H0 is unchanged within noise (0.58 → 0.63%).
- **Mixed, reported honestly:** the wet/dry contrast is stronger on ocean H8 (per-rim 1.5× → 2.1×, trap 2.6× → 5.4×, wet-region vapor now exceeds dry) and on rocky H8. It is **weaker on ocean H0** (per-rim 3.4× → 2.5×, trap 7× → 3×). Core landing spreads water onto more flank regions (wet regions 6.6 → 7.0, dry-region vapor 977 → 1,087), and the shed feeds the dry neighbours of bare underside regions.
- **Watch:** the water cycle slows by 3–22% (evap 132k → 128k, 88k → 80k, 10.3k → 9.9k, 9.2k → 7.2k). Rocky H8 σ_h is 3.66 → 3.09 and its ice is 9 → 16. This is single seed.
- **Pass:** there is no runaway, and the books are exact.

**Risks / next.**
- Shed is still 11–23% of rain, because the full ball has no AIR over the flank/underside rock. Real runoff on bare rock would need headroom (a full sky shell) or a "wet rock" surface store.
- Evaporation still uses world `+y` tops, so flank and underside water evaporates as if it were buried. That holds the cycle slower and is the likely cause of the evap drop.
- The regions are coarse (54), and this is single seed at r12.
- Next options:
  - (a) radial evaporation (the same core-relative top test for evap) so flank puddles cycle;
  - (b) a full sky shell (radial headroom) so the shed falls toward 0;
  - (c) a mix τ sweep (8192 helps dry clearing, see `rct8k`);
  - (d) multi-seed / r16 soaks of the full stack;
  - (e) sun-side advection;
  - (f) lithification of SED / M2.

**Files.**
- Code:
  - `chem/vapor.rs`: `RainFall`, `VaporGeo.adj` and `nearest_ground`.
  - `chem/p6.rs`: `P6Params.rain_fall`.
  - `chem/p6_rest.rs`: Core landing and the shed in `driven_water_pass`.
  - `chem.rs`: re-export, plus `rain_shed()`.
  - `chem/tests_p6.rs`: 3 new tests and 2 tests pinned to `Down`.
  - `wasm/src/play_chem.rs`: 1 new test.
  - `chem_soak` arg 11 `rain=core|down`, and `shed=`/`shed_hops=`/`water_lower=`/`ice_lower=` on the `vloc:` line.
- Measurement: `/workspace/scale_measure/rain_core/`:
  - `soaks/rc_*`, `rcg_*` and `rct8k_*`
  - `ab/`
  - `summarize_rc.py`, `blocks5.py` and `blocks_summary.txt`
  - `bin/chem_soak_rc` and `tests/`
- Backups: `/workspace/backups_rain_core/{pre,post}/`, with `post/rain_core_core.diff` and `post/chem_soak.diff`.
- www rebuilt, and wasm ABI stays 17. md5s:
  - wasm `195905eb21ba133b43dc0471f33a25dc` (1,344,994 B)
  - play.html `3af888eb768f4b2e78d2f15d4e58a15f` (2,008,976 B)
  - bitblock-lab.html `aa8f6a7405d6e88c85bc2b614eedf4e3` (2,036,271 B)
  - bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged)
  - Served at http://127.0.0.1:8765, and the served bytes match. Chrome and Desktop have not been checked yet.

## 5t. Radial evaporation: the evap top test in rain's core frame (2026-10-28, uncommitted; §5s option a) — ocean evap back above §5s (ocean H0 above §5r), rocky evap lower, ocean H0 wet/dry contrast weaker, spill 0, no runaway, books exact

**What changed.**
- New `EvapTop::{Up, Core, CoreAir}` in `chem/vapor.rs`, with `P6Params.evap_top` defaulting to `Core`. Driven only; Classic is untouched. Rain (§5s) is unchanged.
- `Core` (shipped): a WATER cell may evaporate when its outward lattice step from the core (`outward_step(cell - core)`, the step `RainFall::Core` uses) is space or not WATER. This is the old `+y` "cell above is not WATER" test turned radial. Flank and underside columns now evaporate from their own outer end, not from whichever cell has no WATER at world `+y`. Straight up (outward = `+y`) the two tests are identical.
- `Up` reproduces §5s exactly: the T/W lines of `evap=up` match the §5s `rc` ocean H0 soak through 20k (`ab/idcheck_up_*`), apart from wall-clock ms.
- `CoreAir` (A/B only, not the default): the outward step must be AIR or space, which is rain's landing test. It starves the cycle (below).
- The new code is two helpers in `chem/p6_rest.rs`, `evap_frame` (core cell, or `None` under `Up`) and `evap_open`, called from the `driven_water_pass` evap loop. The odds (`EVAP_BASE/SLOPE/MAX`), latent pair and books are unchanged. `degas_pass` still uses its `+y` submerged / `-y` bed test.
- The core is the domain cell nearest the bounding-box centre, so a flat plate also has one. Plate tests that need the legacy test pin `Up`, the same way they pin `RainFall::Down`.

**Why not the strict AIR test.** The task asked for rain's "open above" test (outward AIR/space). A 300k A/B (`reair_*`) shows it cuts evap 16–44% against §5s, and water pools (ocean H0 water cells 659 → 810, lower-hemisphere water 118 → 228). A 20k probe (`bin/evap_probe`, ocean H0) of hot (Q > 200) WATER shows why. Of 282 cells the `+y` test opens, 99 are shut under AIR-only: 56 have WATER outward (a diagonal/sideways neighbour in the same pool) and 43 have rock outward (pocketed or overhung water). Only 2 are ice-lidded, so the sea-ice-lid hypothesis was wrong. The AIR-only test opens 252 cells (odds 2.08/tick) against 282 (2.63/tick) under `+y`. The not-WATER radial test opens 347 (2.90/tick): it keeps rock-capped water evaporating, as `+y` always did, and adds the flank/underside outer ends.

**Tests.**
- New in core:
  - `core_evap_opens_the_outward_face_of_flank_and_underside_water` (local-sky world, 12 samples: Up-open upper/lower 236 / 1,734; Core-open 268 / 2,917; Up-open cells with WATER outward 661; asserts Core = "outward space or not WATER" and Core = Up straight up).
  - `evap_top_up_and_core_both_keep_exact_books_and_diverge` (1,500 ticks: up evap 794, rained 573; core 743 / 529; core-air 547 / 397; books, energy and lane drift exact every tick).
- New in wasm: `full_ball_core_evap_keeps_books_and_differs_from_up_evap` (r12 ocean seed 1, 2,500 ticks, Up vs Core: books exact, spill 0, outputs differ).
- Results: core 285/285 with `--include-ignored` (+2), chem:: 110 passed + 3 ignored (+2), wasm 76/76 (+1).

**Soaks** (r12, seed 1, 300k, m1 driven, `walb=fresnel cloud=vapor vloc=patch rain=core`; 250–300k block means; `rc` = §5s, `re` = §5t `evap=core`; `lv` §5r evap in brackets).

| 250–300k | ocean H0 rc | ocean H0 re | ocean H8 rc | ocean H8 re | rocky H0 rc | rocky H0 re | rocky H8 rc | rocky H8 re |
|---|---|---|---|---|---|---|---|---|
| σ_h | 2.57 | 2.46 | 3.05 | 3.05 | 0.55 | 0.55 | 3.09 | 3.11 |
| ice cells | 200.8 | 216.5 | 408.4 | 330.7 | 7.0 | 8.0 | 16.0 | 16.9 |
| vapor | 1,630 | 1,740 | 896 | 983 | 106 | 102 | 76 | 65 |
| evap /50k (lv) | 128k | **137k** (132k) | 80k | **85k** (88k) | 9.9k | 9.5k (10.3k) | 7.2k | 6.0k (9.2k) |
| water cells | 659 | 590 | 1,144 | 1,204 | 32 | 35 | 76 | 87 |
| water-top Q | 853 | 844 | 648 | 687 | 720 | 711 | 425 | 423 |
| cloud % of sun | 5.96 | 5.72 | 2.70 | 2.66 | 0.73 | 0.71 | 0.13 | 0.12 |
| trap % | 10.31 | 10.01 | 4.90 | 5.22 | 1.26 | 1.24 | 0.45 | 0.41 |
| cloud % over water/ice / other | 10.42 / 4.82 | 10.45 / 4.51 | 4.12 / 2.16 | 4.69 / 1.97 | 3.72 / 0.63 | 3.34 / 0.63 | 1.15 / 0.08 | 0.96 / 0.07 |
| per-rim cloud wet / dry regions | 81 / 32 | 69 / 32 | 33 / 16 | 50 / 15 | — / 4.0 | — / 3.9 | 37 / 0.75 | 31 / 0.70 |
| trap % wet / dry regions | 25.2 / 8.4 | 19.8 / 8.3 | 14.9 / 2.8 | 15.8 / 3.0 | — / 1.3 | — / 1.2 | 6.2 / 0.16 | 5.1 / 0.18 |
| wet regions (of 54) | 7.0 | 7.2 | 17.9 | 12.6 | 0 | 0 | 2 | 2 |
| vapor in wet / dry regions | 543 / 1,087 | 634 / 1,106 | 482 / 414 | 572 / 411 | 0 / 106 | 0 / 102 | 49 / 27 | 41 / 24 |
| spilled (planet-wide) | 0 | **0** | 0 | **0** | 0 | **0** | 0 | **0** |
| shed % of rain, mean hops | 23.3, 1.60 | 30.6, 1.54 | 17.3, 1.16 | 27.1, 1.22 | 23.1, 1.17 | 22.6, 1.21 | 10.7, 1.08 | 11.5, 1.08 |

- Strict `reair` evap /50k: 108k / 56k / 6.9k / 4.0k (ocean H0, ocean H8, rocky H0, rocky H8), i.e. −16 / −30 / −30 / −44% vs rc, with water cells 810 / 1,420 / 60 / 105.
- Evap per 50k block, re: ocean H0 120k 132k 135k 135k 137k 137k; ocean H8 74k 81k 83k 85k 85k 85k; rocky H0 8.1k 8.9k 9.2k 9.5k 9.6k 9.5k; rocky H8 6.1k 6.2k 6.1k 6.1k 6.3k 6.0k. Vapor and ice flatten by 100k on all four. Rocky H8 water creeps 71 → 87 but levels off over the last three blocks.
- Books: END `book_fail_total=0 max_abs_driftE=0 driftL=[0;12]` on all eight 300k runs (re and reair).

**Verdict.**
- **Pass (oceans):** evap recovers. Ocean H0 is 128k → 137k (above §5r's 132k), and ocean H8 is 80k → 85k (§5r 88k). Ocean H8 ice 408 → 331 and water-top Q 648 → 687 (warmer). Ocean H0 ice 201 → 217 is within block noise of §5s.
- **Fail (rocky):** evap does not recover. Rocky H0 is 9.9k → 9.5k (−4%), and rocky H8 is 7.2k → 6.0k (−16%, §5r 9.2k). On rocky worlds the extra flank/underside opening is outweighed by `+y` tops the radial step now shuts (pool neighbours outward).
- **Fail:** ocean H0 wet/dry contrast is lower than §5s. Per-rim cloud is 2.5× → 2.2× and trap 3.0× → 2.4×. More water cycles from flank regions, so dry-region vapor (1,106) still exceeds wet (634). Ocean H8 contrast is stronger (per-rim 2.1× → 3.4×, trap about 5.3× both); rocky H8 is about the same (49× → 44× per-rim).
- **Pass:** spill is 0 on all eight runs, and the books are exact.
- **Pass:** there is no runaway.
- **Watch:** shed rises on the oceans (23 → 31%, 17 → 27% of rain). More vapor rains onto regions with no landing ground. This is single seed at r12.

**Risks / next.**
- The rocky slowdown and the ocean H0 contrast loss are both real at one seed. Pinning `evap_top = Up` restores §5s bit-for-bit if needed.
- `degas_pass` still uses world `±y`, so hot buried flank water degasses by a different frame than it evaporates.
- Shed (11–31%) is the next lever: the full ball has no AIR over flank/underside rock.
- Next options:
  - (a) a full radial sky shell (headroom over the flanks) so the shed falls toward 0 and flank rain lands where it evaporates;
  - (b) multi-seed / r16 soaks of the §5r–§5t stack to tell the rocky and ocean H0 shifts from seed noise;
  - (c) a mix τ sweep (8192 cleared dry regions in §5s);
  - (d) a radial `degas_pass`;
  - (e) sun-side advection;
  - (f) SED lithification / M2.

**Files.**
- Code:
  - `chem/vapor.rs`: `EvapTop`.
  - `chem/p6.rs`: `P6Params.evap_top`.
  - `chem/p6_rest.rs`: `evap_frame` and `evap_open`, plus the `driven_water_pass` evap loop.
  - `chem.rs`: re-export.
  - `chem/tests_p6.rs`: 2 new tests and the `evap_tops` helper.
  - `wasm/src/play_chem.rs`: 1 new test.
  - `chem_soak` arg 12 `evap=core|up|coreair`, and `evap_top=` in the header. Probe `src/bin/evap_probe.rs`.
- Measurement: `/workspace/scale_measure/radial_evap/`:
  - `soaks/re_*` (shipped `evap=core`; run as `corenw` before the variant rename, T/W lines identical, see `ab/idcheck_core_*`) and `soaks/reair_*` (strict A/B)
  - `ab/` (30k up/core/coreair probes, id checks, `probe_ocean0_*`)
  - `summarize_rc.py`, `traj.py`, `blocks5.py` and `blocks_summary.txt`
  - `bin/chem_soak_re`, `bin/evap_probe` and `tests/`
- Backups: `/workspace/backups_radial_evap/{pre,post}/`, with `post/radial_evap_core.diff`, `post/radial_evap_wasm.diff` and `post/chem_soak.diff`.
- www rebuilt, and wasm ABI stays 17. md5s:
  - wasm `21f3a2b904a436f0422ca90dce718fe5` (1,345,587 B)
  - play.html `9063b5c0fb488556b3114142f1461409` (2,009,841 B)
  - bitblock-lab.html `a3335c7819818e1bb9d8ed7f097de545` (2,037,136 B)
  - bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged)
  - Served at http://127.0.0.1:8765, and the served bytes match. Chrome and Desktop have not been checked yet.

## 5u. Full radial sky shell: headroom over flanks and underside (2026-10-28, uncommitted; §5t option a) — shed falls to ~0 (ocean 1.05% / 0%, rocky 0%), ocean wet/dry contrast much stronger, books exact; but the ocean migrates to the never-lit night hemisphere, rocky contrast collapses and rocky evap decays, ocean H8 vapor still climbing

**What changed (shell geometry).**
- New `skyshape=radial`: `ChemWorld::add_radial_sky_shell(shape, H)` / `genesis_sky_radial(cfg, params, H)` add AIR (Q = 0) on every lattice cell with R² < |c − centre|² ≤ (R + H)², skipping cells already in the domain or on a non-chem bit. The §5g polar `+y` shell cells are a subset. Books and t0 state are untouched; the M1 core is pinned before the merge (`merge_sky_air`). `H = 0` is a no-op (full-ball control).
- Domain at r12: ball 7,153 cells; polar H8 10,681 (+3,528); radial H4 17,077 (+9,924); radial H8 33,401 (+26,248).
- Outward guard (`shell_blocked`, on by default with a radial shell): flow, settle, scour, dissolve and gas rise never move material into a radial-only shell cell (outside the ball and outside the polar `+y` columns). Without it world `−y` gravity drips flank/underside water straight off the planet into the lower shell. `set_outward_guard(false)` is the A/B (`skyshape=radialfree`). A first cut with a dot-product (outward-step) guard let the upper ocean slide round to the underside and was discarded (`radial_sky/discarded_v1/`).
- Rain (§5s `RainFall::Core`) and evaporation (§5t `EvapTop::Core`) are unchanged; they simply find AIR headroom over flank and underside rock now, so drops land in their own region instead of being shed. The sun is unchanged: `sun_dir` sweeps the x–y plane and only shines with sy > 0, so y < core is permanent night.
- No wasm-crate source change (`radial_sky_wasm.diff` empty); play stays polar. ABI stays 17.

**Tests.**
- New in core (`chem/tests_p6.rs`): `radial_sky_shell_keeps_t0_books_cells_and_covers_every_direction`, `radial_sky_zero_is_a_noop`, `radial_guard_blocks_moves_into_radial_only_cells_only`, `radial_sky_driven_m1_books_exact_and_guard_holds_the_water`, `radial_guard_holds_equator_water_and_free_flow_drips` (guard on holds equator water, guard off drips it into the shell), `radial_sky_sheds_less_rain_than_the_polar_shell` (H6, 2,000 ticks, radial shed fraction ≤ ¼ of polar).
- Results: core 291/291 with `--include-ignored` (+6), chem:: 116 passed + 3 ignored (+6), wasm 76/76 (unchanged).
- Identity controls (20k, T/W lines minus wall-clock vs §5t `re_*`): ocean and rocky `skyshape=radial sky=0` and `skyshape=polar sky=8` are IDENTICAL to §5t, books exact.

**Soaks** (r12, seed 1, 300k, m1 driven, `walb=fresnel cloud=vapor vloc=patch rain=core evap=core`; 250–300k block means; `re0` = §5t full ball H0, `re8p` = §5t polar H8, `rs4` / `rs8` = §5u radial H4 / H8, guard on).

| 250–300k | ocean re0 | ocean re8p | ocean rs4 | ocean rs8 | rocky re0 | rocky re8p | rocky rs4 | rocky rs8 |
|---|---|---|---|---|---|---|---|---|
| domain cells | 7,153 | 10,681 | 17,077 | 33,401 | 7,153 | 10,681 | 17,077 | 33,401 |
| **shed % of rain** (tot 300k) | 30.6 (298k) | 27.1 (150k) | **1.05 (4.3k)** | **0.00 (0)** | 22.6 (12.9k) | 11.5 (4.2k) | **0.00 (0)** | **0.00 (0)** |
| spilled | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| σ_h | 2.46 | 3.05 | 4.20 | 4.98 | 0.55 | 3.11 | 2.47 | 2.88 |
| ice cells | 216.5 | 330.7 | 368.6 | 285.9 | 8.0 | 16.9 | 90.5 | 55.0 |
| vapor | 1,740 | 983 | 893 | 1,298 | 102 | 65 | 29.5 | 88.7 |
| evap /50k | 137k | 85k | 78.5k | 123k | 9.5k | 6.0k | **2.7k** | 8.4k |
| froze /50k | 7.2k | 21.4k | 24.4k | 17.6k | 248 | 2.3k | 8.5k | 5.8k |
| water cells | 590 | 1,204 | 1,228 | 825 | 34.6 | 86.5 | 50.9 | 54.8 |
| water-top Q / land-top Q | 844 / 615 | 687 / 489 | 511 / 748 | 544 / 875 | 711 / 568 | 423 / 675 | 353 / 829 | 568 / 1,263 |
| cloud % of sun | 5.72 | 2.66 | 0.92 | 0.77 | 0.71 | 0.12 | 0.05 | 0.09 |
| trap % | 10.01 | 5.22 | 3.10 | 3.62 | 1.24 | 0.41 | 0.11 | 0.23 |
| per-rim cloud wet / dry regions | 69 / 32 (2.2×) | 50 / 15 (3.4×) | **30 / 4.2 (7.3×)** | **36 / 1.7 (21×)** | — / 3.9 | 31 / 0.70 (44×) | **0.35 / 0.67 (0.5×)** | **2.4 / 1.0 (2.4×)** |
| trap % wet / dry regions | 19.8 / 8.3 | 15.8 / 3.0 | 7.1 / 0.86 | 10.7 / 0.13 | — / 1.2 | 5.1 / 0.18 | −0.11* / 0.13 | 0.22 / 0.23 |
| wet regions (of 54) | 7.2 | 12.6 | 23.0 | 21.7 | 0 | 2 | 8.9 | 4.9 |
| vapor in wet / dry regions | 634 / 1,106 | 572 / 411 | 773 / 120 | 1,221 / 76 | 0 / 102 | 41 / 24 | 3.3 / 26 | 16 / 72 |
| water / ice with y < core @300k | 101 / 210 | 126 / 206 | **1,189 / 413** | **788 / 332** | 10 / 8 | 9 / 6 | 30 / 105 | 37 / 70 |

\* `trap_wetreg%` −0.11 is a diagnostic artifact (window mean of a split counter on 0.35-cloud regions), not negative physics.

- Evap per 50k block: ocean rs4 64k 66k 71k 76k 77k 79k (rising, slowing); ocean rs8 107k 102k 107k 112k 119k 123k (still climbing; vapor 1,120 → 1,298, water 940 → 825); rocky rs4 8.8k 5.6k 4.2k 3.5k 3.2k 2.7k (monotonic decline; ice 20 → 91); rocky rs8 13.2k 14.3k 13.4k 10.8k 9.3k 8.4k (declining; ice 0 → 55).
- σ_h every 50k: ocean rs4 1.61 → 4.20, rs8 1.61 → 5.18 (still rising); rocky rs4 0.45 → 2.51, rs8 0.45 → 2.88.
- **Night-hemisphere migration.** Water with y < core at t0 / 5k / 300k: §5t polar 950 / 432 / 126 of 1,977 / 1,129 / 1,175 (ocean drains to the lit top); radial H4 950 / **1,207 of 1,232** / 1,189 of 1,195; radial H8 950 / 869 of 928 / **788 of 788**. Within 5k ticks the ocean ends up on the never-lit underside. Day-side water evaporates, rain now lands everywhere (the underside has headroom), and night-side water never re-evaporates: a cold-trap pump. The guard-off A/B (`free_r12_ocean_H8`, 30k) shows the same end state by another route (water_lower = water_cells every block, shed 0, books exact), as did the discarded v1 guard.
- Books: END `book_fail_total=0 max_abs_driftE=0 driftL=[0;12]` on all four 300k radial soaks, the four 20k id checks and the radialfree A/B.

**Verdict vs intent.**
- **Pass:** shed falls to near 0. Ocean 27–31% → 1.05% (H4) / 0% (H8), rocky 11–23% → 0%. Spill stays 0.
- **Pass:** books exact on every run; reflected light never injected (cloud/albedo paths unchanged).
- **Pass (ocean) / Fail (rocky):** wet/dry contrast. Ocean per-rim cloud 3.4× → 7.3× (H4) / 21× (H8), trap 5.3× → 8.3× / 80×, wet regions 12.6 → 23 / 22, dry-region vapor 411 → 120 / 76. Rocky collapses: per-rim 44× → 0.5× (inverted, H4) / 2.4× (H8), cloud of sun 0.12% → 0.05 / 0.09%.
- **Watch / fail:** no runaway. Ocean H4 is slowing (+2% last block). Ocean H8 vapor and evap still climb ~4%/block while water falls ~3%/block (a growing atmosphere in a 3× larger domain, not flattened by 300k). Rocky H4 is a slow freeze-out (evap 8.8k → 2.7k, ice 20 → 91, not flat); rocky H8 the same trend, milder.
- **Pass:** relief measurable. Ocean σ_h 3.05 → 4.20 / 4.98; rocky 3.11 (polar H8) → 2.47 (H4) / 2.88 (H8). Single seed.
- **Big caveat:** the shell does what was asked (rain lands where it falls), but it exposes the sun model: y < core is permanent night, and with landing headroom there the ocean and much of the ice move onto it. Ocean "wet" regions and the stronger contrast are partly the night-side ocean.

**Risks / next.**
- Radial is opt-in (`skyshape=radial`, `genesis_sky_radial`); polar `genesis_sky` and play are bit-identical to §5t (id checks). Domain grows 2.4× (H4) / 4.7× (H8) at r12; ~6.9 ms/step native at H8.
- The guard is a rule, not physics: world `−y` gravity still drives flow, so the guard is what stops underside water dripping into space. Single seed, r12.
- Next options:
  - (a) light the whole sphere: a sun sweep / rotation axis (or tilt) so the underside gets day, removing the night-side cold trap;
  - (b) radial gravity for flow/settle (toward the core) so the guard becomes unnecessary;
  - (c) multi-seed / r16 soaks of §5r–§5u, and a longer ocean H8 to see if vapor flattens;
  - (d) a mix τ sweep;
  - (e) radial `degas_pass`;
  - (f) SED lithification / M2.

**Files.**
- Code: `chem.rs` (`add_radial_sky_shell`, `merge_sky_air`, `set_outward_guard`, `outward_guard`, `sky_radial`, `shell_blocked`, `genesis_sky_radial`), `chem/p6.rs`, `chem/p6_rest.rs` (guard in flow/settle/scour/dissolve/gas rise), `chem/tests_p6.rs` (6 tests, `shell_water` helper); `chem_soak` arg 13 `skyshape=polar|radial|radialfree` (header prints `ball_cells`, `domain_cells`, `outward_guard`).
- Measurement: `/workspace/scale_measure/radial_sky/`: `soaks/rs_r12_{ocean,rocky}_H{4,8}.txt`, `soaks/idchk_*`, `soaks/free_r12_ocean_H8.txt`, `discarded_v1/`, `make_summary.sh`, `summarize_rc.py`, `traj.py`, `blocks5.py`, `blocks_summary.txt`, `bin/chem_soak_rs`, `tests/`.
- Backups: `/workspace/backups_radial_sky/{pre,post}/`, with `post/radial_sky_core.diff`, `post/chem_soak.diff`, `post/radial_sky_wasm.diff` (empty).
- www rebuilt at 04:44 PDT (the core change is compiled in; no wasm-crate source change). ABI stays 17. md5s (served at http://127.0.0.1:8765, bytes match):
  - wasm `cca1a1ca30f6e5964005454e4300288a` (1,347,271 B)
  - play.html `33dfa6a281d408264d887f627e1d0983` (2,012,309 B)
  - bitblock-lab.html `217c9ca1cefb95c3e31f0238edb1899d` (2,039,604 B)
  - bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged)
  - Chrome and Desktop have not been checked.

## 5v. Full-sphere sun: `SunSweep::Full` lights the underside (2026-10-28, uncommitted; §5u option a) — night-side ice gone (0 at sun 16, 332 → 42 below core at matched energy), shed stays 0 with the radial shell, books exact; but the ocean still sits below the core: §5u's "night cold trap" was mostly world −y flow, not night. Full at sun 16 doubles the day-mean input and dries the ocean; tilt changes little

**What changed (sun model).**
- New `P6Params.sun_sweep: SunSweep` (`chem/p6.rs`), default `SunSweep::Upper` = the §5b–§5u sun bit-for-bit (turns in the x–y plane, shines only while `s.y > 0`, so every `−y` face is never lit).
- `SunSweep::Full { tilt, year }`: the sun is a distant source that never sets. At every tick the faces with `d·s > 0` are lit (the half of the sphere facing the sun), and over one day every x–y longitude, `−y` included, gets a noon. `SunSweep::FULL` = `{tilt: 0, year: 0}` (equinox: the ±z caps stay unlit). With `tilt > 0` and `year > 0`, `s.z = tilt · tri_sin(year phase) / 1024` over `year` days and `s.x, s.y` shrink by `(1024 − |s.z|)/1024`, so `|s|₁` stays 1024 (same total sun on a closed surface; test checks 1022..=1024).
- `EnergyInput::sun_dir3(tick, sweep) -> (sx, sy, sz, lit)` replaces the 2-D call in `energy_input_pass`. The NB6 dot list gains `−sz, +sz`. Each lit face's share `sun·d/1024` is deposited (cloud and surface albedo first; reflected light is never injected) and the same amount is booked in `input_q`, exactly as before.
- `SunSweep::track_mask()` tells the dry-weathering proxy which faces see the day/night swing: `−x, +x, +y` (Upper), plus `−y` (Full), plus `±z` (tilted).
- Diagnostic only (outside books): `hemi_diag() = [evap_lower, evap_all, rain_lower, rain_all]` (y < core split) and `water_zcap` / `ice_zcap` in `chem_soak`'s `vloc:` line.
- **Opt-in only.** `chem_soak` arg 14 `sweep=upper|full|tilt:T:Y` (default upper). Play has no sweep control and still runs Upper. No wasm-crate source change (`full_sun_wasm.diff` empty); ABI stays 17.
- Energy note: Full roughly doubles the day-mean sun at the same `sun` (no half-day of night). `fe8` (`sun=8`) is the energy-matched run.

**Tests.**
- New in core (`chem/tests_p6.rs`, §5v block): `sun_sweep_upper_is_the_legacy_sun_and_full_never_sets`, `full_sun_gives_every_track_face_a_noon_and_tilt_keeps_l1`, `full_sun_lights_the_underside_face_and_books_it` (a single ROCK cell at an `sy < 0` tick: Upper injects 0, Full injects exactly `16·−sy/1024 + 16·|sx|/1024`, books exact), `full_sun_radial_sky_books_exact_and_lights_the_lower_hemisphere` (radial H6, 1,200 ticks, books and lane totals exact every tick; Full input > 1.5× Upper; y < core rock warmer).
- Results: core 295/295 with `--include-ignored` (+4), chem:: 123 (+4), wasm 76/76 (unchanged).
- Identity controls (20k, `sweep=upper`, T/W lines minus wall-clock vs §5u `rs_*_H8`): ocean and rocky IDENTICAL, books exact.

**Soaks** (r12, seed 1, 300k, m1 driven, `walb=fresnel cloud=vapor vloc=patch rain=core evap=core skyshape=radial`; 250–300k block means). `rs4` / `rs8` = §5u (Upper). `fs0` / `fs4` / `fs8` = Full, sky 0 / 4 / 8, sun 16. `fe8` = Full, sun 8 (energy-matched to rs8). `ft8` = `tilt:410:8` (8-day year), sun 16.

| ocean, 250–300k | rs4 | rs8 | fs0 | fs4 | fs8 | **fe8** | ft8 |
|---|---|---|---|---|---|---|---|
| water / ice cells | 1,228 / 369 | 825 / 286 | 430 / 0 | 346 / 0 | 363 / 0 | **860 / 149** | 367 / 0 |
| **water / ice with y < core** @300k | 1,189 / 413 | 788 / 332 | 1 / 0 | 325 / 0 | 329 / 0 | **818 / 42** | 341 / 0 |
| water in ±z caps @300k | — | — | 273 | 186 | 72 | 232 | 62 |
| evap share at y < core (last 50k) | — | — | 0.3% | 99.5% | 99.8% | 99.8% | 99.9% |
| vapor | 893 | 1,298 | 2,307 | 2,105 | 2,054 | 1,387 | 2,068 |
| evap /50k | 78.5k | 123k | 170k | 189k | 197k | 132k | 199k |
| froze /50k | 24.4k | 17.6k | 0 | 0 | 0 | 26.9k | 0 |
| water-top Q / land-top Q | 511 / 748 | 544 / 875 | 1,867 / 1,594 | 1,766 / 1,564 | 2,314 / 1,717 | 299 / 658 | 2,052 / 1,701 |
| **shed % of rain** (tot 300k) | 1.05 (4.3k) | 0 (0) | 29.0 (302k) | **0 (0)** | **0 (0)** | **0 (0)** | **0 (0)** |
| spilled | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| per-rim cloud wet / dry regions | 30 / 4.2 (7.3×) | 36 / 1.7 (21×) | 168 / 38 (4.4×) | 61 / 20 (3.0×) | 19 / 18 (1.05×) | **30 / 3.0 (9.9×)** | 53 / 17 (3.2×) |
| wet regions (of 54) | 23.0 | 21.7 | 2 | 8 | 3 | 25.3 | 3 |
| cloud % of sun / trap % | 0.92 / 3.10 | 0.77 / 3.62 | 3.50 / 8.30 | 2.85 / 6.40 | 2.67 / 6.18 | 2.04 / 5.07 | 2.60 / 5.97 |
| σ_h | 4.20 | 4.98 | 2.46 | 4.91 | 6.02 | 5.55 | 6.25 |

| rocky, 250–300k | rs4 | rs8 | fs0 | fs4 | fs8 |
|---|---|---|---|---|---|
| water / ice cells | 50.9 / 90.5 | 54.8 / 55.0 | 19.1 / 0 | 22.6 / 0 | 19.1 / 0 |
| water / ice with y < core @300k | 30 / 105 | 37 / 70 | 0 / 0 | 19 / 0 | 15 / 0 |
| vapor | 29.5 | 88.7 | 129 | 174 | 183 |
| evap /50k (first → last block) | 8.8k → 2.7k | 13.2k → 8.4k | 11.8k → 12.4k | 13.7k → 16.3k | 13.8k → 17.2k |
| shed % of rain (tot 300k) | 0 | 0 | 16.8 (13.8k) | 3.11 (619) | **0 (0)** |
| wet regions | 8.9 | 4.9 | 0 | 0 | 1 |
| per-rim cloud wet / dry | 0.35 / 0.67 | 2.4 / 1.0 | — / 4.6 | — / 3.8 | 0.2 / 2.5 |
| σ_h | 2.47 | 2.83 | 0.57 | 4.32 | 5.35 |

- **Who removes the upper ocean (0 → 5k, ocean).** Upper-hemisphere water+ice goes 1,027 → 0–169 on every radial run, Upper control included (`up8`: → 59). Rain onto the upper half beats evaporation there (net +335 to +769 cells), so evaporation is not what empties it: 1,190–1,800 cells are moved down by flow and settle under world −y gravity. The guard then holds that water on the ball's underside. Full sun does not change this, because it changes light, not gravity.
- **What Full does change.** The underside is now lit. Ocean ice drops to 0 at sun 16, and at matched energy it falls from 286 to 149 (below the core: 332 → 42). Freeze goes 17.6k → 0 per 50k at sun 16. 99.8% of evaporation now comes from the y < core ocean, which is a lit, cycling sea rather than a frozen night store.
- **No shell (H0).** Water leaves the lower half (950 → 1), but 273 of 461 cells pool in the ±z caps, which stay unlit at equinox. Shed is 29% (ocean) / 16.8% (rocky), the same as the §5t no-shell runs (30.6% / 22.6%).
- **Tilt.** `ft8` ≈ `fs8` (water 367 vs 363, z-cap water 62 vs 72, σ_h 6.25 vs 6.02). An 8-day tilted year barely matters at H8.
- Runaway check. The sun-16 runs flatten: vapor grows < 1% per block by the end, and rocky evap rises then levels off. That ends §5u's rocky freeze-out. `fe8` behaves like rs8: vapor 1,077 → 1,387 (+5%/block at the end), water 990 → 860, evap 103k → 132k, still climbing at 300k.
- σ_h every 50k: ocean fs8 1.61 → 6.10, fe8 → 5.65, ft8 → 6.39, fs4 → 4.92; rocky fs8 0.45 → 5.46, fs4 → 4.43 (rs8 2.88). The added `−y` weathering track and the doubled sun both feed this.
- Books: END `book_fail_total=0 max_abs_driftE=0 driftL=[0;12]` on all 300k Full/tilt runs (ocean ×5, rocky ×3) and both id checks.
- `fe8 trap_wetreg%` −70.5 is the same split-counter window artifact as §5u, not physics.

**Verdict vs intent.**
- **Partial: night cold trap.** The night-side ice store is gone (sun 16) or greatly reduced (matched: 332 → 42 below core), and the underside ocean is now lit and evaporates. **Fail** on "water not all below core" with a radial shell: 325/346, 329/363, 818/860, 341/367. That is world −y flow, which full sun cannot fix. §5u's "night-side cold-trap pump" diagnosis is corrected here: night added the ice, gravity moved the water.
- **Pass:** books exact everywhere; reflected light never injected; Upper default bit-identical to §5u.
- **Pass:** shed stays ~0 with the radial shell (ocean 0 on H4/H8/fe8/ft8; rocky 0 at H8, 3.1% at H4). H0 shed ~29% is expected (no headroom), not a regression.
- **Mixed: wet/dry.** At matched energy, ocean contrast holds (9.9× vs 21×, wet regions 25 vs 22). At sun 16 it collapses (H8 1.05×, 3 wet regions), because the ocean dries to ~360 cells and vapor reaches ~2,050 everywhere. Rocky has no wet regions now (0–1 vs 5–9), though its water cycle no longer freezes out.
- **New issue:** `Full` at the same `sun` is a 2× energy change. Fair comparisons need `sun=8` (`fe8`); a rocky `fe8` was not run.

**Risks / next.**
- Full is opt-in (`sweep=`, `P6Params.sun_sweep`); play and `genesis_*` defaults remain Upper. Single seed, r12.
- The equinox ±z caps are a new unlit cold spot on H0. Tilt reaches them, but an 8-day year changed little at H8.
- Next options:
  - (a) radial gravity for flow/settle (toward the core), the real fix for the water-below-core result, which also retires the guard;
  - (b) Full + `sun=8` as the energy-matched default for future soaks, plus a rocky `fe8`;
  - (c) multi-seed / r16 and a longer fe8 (still trending at 300k);
  - (d) a play sweep toggle (opt-in) if Owen wants to see it live;
  - (e) a mix τ sweep / radial degas / SED lithification / M2, as before.

**Files.**
- Code: `chem/p6.rs` (`SunSweep`, `track_mask`, `sun_dir3`, `P6Params.sun_sweep`, `energy_input_pass`), `chem/p6_rest.rs` (`weather_pass` track mask), `chem.rs` (re-export, `hemi_diag`), `chem/tests_p6.rs` (4 tests); `chem_soak` arg 14 `sweep=` (header prints `sun_sweep=`; `vloc:` prints `hemi=`, `water_zcap`, `ice_zcap`).
- Measurement: `/workspace/scale_measure/full_sun/`: `soaks/{fs,fe,ft}_r12_*`, `soaks/idchk_*_H8upper.txt`, `smoke_full.txt`, `make_summary.sh`, `summarize_rc.py`, `hemi.py`, `flow.py`, `traj.py`, `blocks_summary.txt`, `bin/chem_soak_fs`, `tests/{core_all,wasm}.txt`.
- Backups: `/workspace/backups_full_sun/{pre,post}/`, with `post/full_sun_core.diff`, `post/chem_soak.diff`, `post/full_sun_wasm.diff` (empty).
- www rebuilt at 06:25 PDT (core change compiled in; no wasm-crate source change). ABI 17. md5s (served at http://127.0.0.1:8765, bytes match):
  - wasm `2ed457817cd0411335c3fa6c363d1129` (1,348,264 B)
  - play.html `1f5f3cf021ea87da4d8bea15974e586f` (2,013,765 B)
  - bitblock-lab.html `f28eb10af26c3dc872981ad40c38f568` (2,041,060 B)
  - bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged)
  - Chrome and Desktop have not been checked.

## 5w. Coreward gravity: Driven flow/settle fall toward the core (2026-10-28, uncommitted; §5v option a) — the ocean no longer pools below the core (water with y < core 27–43% vs 91–100% in §5u/§5v), shed stays ~0 with the radial shell and the guard auto-off, books exact; but the sun-16 ocean still dries to one wet region, σ_h roughly halves, and the leftover water collects in the unlit equinox ±z caps

**What changed (gravity model).**
- New `Gravity::{Down, Core}` (`chem/vapor.rs`), default **`Core`**, applied only under `WaterCycle::Driven`. Classic stays planet6's world `−y` bit-for-bit. `Down` = §5b–§5v world `−y`, kept for flat plates and parity.
- `ChemWorld::coreward()` = Driven && Core (and a core cell exists; otherwise falls back to `−y`). Frame matches `RainFall::Core` / `EvapTop::Core`: "down" is the cell's inward support set (`−outward_step(cell − core)` plus, on a diagonal, its axis components); "level" is r² = |cell − core|².
- `p6_rest.rs`: `grav_core`, `inward_target` (a diagonal inward step only through an open corner, so nothing squeezes between two solids), `side_target` (lowest face neighbour with r² ≤ r²[i] + 1, ties rotated by `wide_roll` so no direction is favoured), `settle_core` and `flow_core`. Both iterate `topo.rsort` inner cells first, move each cell at most once per tick and are pure swaps (books untouched). Gas rises along the outward step; the scour / entrain / degas / collapse direction gates use the same frame.
- Outward guard: `outward_guard: Option<bool>`, `None` = auto = on only under world `−y`, so it is **auto-off under coreward gravity**. `set_outward_guard(b)` still forces it (used for the A/B).
- `chem_soak` arg 15 `grav=core|down` (header prints `gravity=… coreward=… outward_guard_live=…`); new skyshape `radialguard` = radial shell + forced guard.
- No wasm-crate source change (`core_grav_wasm.diff` empty); ABI 17. **Play's Driven mode is now coreward by default** (core default changed, no play toggle).

**Tests.** 5 new in `tests_p6.rs`: `gravity_core_is_the_driven_default_and_auto_drops_the_guard`, `coreward_flow_falls_toward_the_core_and_down_flow_falls_to_minus_y`, `coreward_side_flow_never_climbs_and_corners_are_not_squeezed`, `coreward_radial_shell_holds_the_ocean_without_the_guard`, `classic_ignores_gravity`. Gravity/coreward/classic + full_sun/sun_sweep filter: 10 passed. Chem suite (`cargo test --release -p bitblock-core --lib -- chem`, rerun post-soak): 127 passed, 0 failed, 3 ignored (`core_grav/tests/core_chem_post.txt`). The full `--include-ignored` core suite and the wasm tests were not rerun.

**Soaks** (r12 seed 1, 300k, Driven, `walb=fresnel cloud=vapor vloc=patch rain=core evap=core`, radial shell, `sweep=full`; block means 250–300k). `cg0/cg4/cg8` = core gravity H0/H4/H8, guard auto (off); `cgg8` = cg8 + guard forced on; `cgp8` = core gravity on the polar sky shell H8; `cge8` = core gravity `sun=8` (energy-matched to `fe8`). `rs8` = §5u, `fs8`/`fe8` = §5v.

| ocean, 250–300k | rs8 | fs8 | fe8 | cg0 | cg4 | cg8 | cgg8 | cgp8 | cge8 |
|---|---|---|---|---|---|---|---|---|---|
| water / ice cells | 825 / 286 | 363 / 0 | 860 / 149 | 274 / 0 | 345 / 0 | 437 / 0 | 472 / 0 | 312 / 0 | 518 / 0.2 |
| water with y < core @300k | 788 / 788 | 329 / 329 | 818 / 848 | **119 / 276** | **114 / 333** | **161 / 435** | 190 / 481 | 88 / 328 | **171 / 528** |
| water in ±z caps @300k | – | 72 | 232 | 161 | 262 | 354 | 414 | 145 | 391 |
| evapL% / rainL% (last block) | – | 99.8 / 97.4 | 99.8 / 97.8 | 48.6 / 52.4 | 40.4 / 42.9 | 44.7 / 46.3 | 43.7 / 46.6 | 34.3 / 34.8 | 45.1 / 46.0 |
| vapor | 1,298 | 2,054 | 1,387 | 2,160 | 2,129 | 2,055 | 2,021 | 2,168 | 1,962 |
| evap /50k | 123k | 197k | 132k | 202k | 186k | 179k | 186k | 198k | 171k |
| shed % of rain (tot 300k) | 0 | 0 | 0 | 0.01 (158) | 0 | **0** | 0 | 0 | 0 |
| spill | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| wet regions / rim_wet | 21.7 / 1,402 | 3 / 156 | 25.3 / 1,651 | 1 / 59 | 1 / 43 | 1 / 85 | 10 / 665 | 1 / 63 | 7 / 564 |
| cl_wet% / cl_dry% | 2.24 / 0.48 | 3.01 / 2.64 | 2.35 / 1.86 | 7.47 / 4.41 | 3.32 / 2.08 | 2.40 / 1.70 | 3.02 / 1.28 | 5.98 / 3.94 | 2.60 / 2.22 |
| σ_h | 4.98 | 6.02 | 5.55 | 1.75 | 2.46 | 3.71 | 3.90 | 3.03 | 2.93 |

| rocky, 250–300k | rs8 | fs8 | cg8 | cgg8 |
|---|---|---|---|---|
| water / ice cells | 54.8 / 55.0 | 19.1 / 0 | 24.4 / 0 | 28.0 / 0 |
| water with y < core @300k | 37 / 44 | 15 / 15 | **8 / 23** | 16 / 29 |
| evapL% / rainL% (last block) | – | 94.6 / 68.9 | 35.2 / 42.8 | 51.9 / 52.2 |
| vapor | 88.7 | 183 | 208 | 164 |
| evap /50k | 8.4k | 17.2k | 19.9k | 15.6k |
| shed % of rain | 0 | 0 | 0 | 0 |
| wet regions / rim_wet | 4.9 / 374 | 1 / 50 | 0 / 0 | 1 / 76 |
| σ_h | 2.83 | 5.35 | 2.48 | 2.38 |

- **Who empties the upper hemisphere now (0 → 5k, ocean).** Under §5v (`fs8`) rain onto the upper half beat evaporation (net +621) and 1,648 cells were moved down by world `−y` flow. Under core gravity, residual flow is ≈ 0 (−315 … +365) and the upper half loses water to evaporation instead (upper evap 9.5–10.9k vs rain 8.5–10.2k, net −512 … −1,202). That is the global sun-16 dry-down, now symmetric: the lower half loses the same share (cg8: 950 → 106 below core at 5k). Flow no longer carries water across the equator.
- **Water below the core.** 27–43% of the ocean (35–55% of rocky, 23–29 cells) sits at y < core, close to even, versus 91–100% in §5u/§5v. Evaporation and rain also split ~even (evapL 34–49%, vs 99.8%). The radial shell holds the ocean with the guard off (also covered by a unit test).
- **New pooling: the ±z caps.** At equinox (`sweep=full`, tilt 0) the ±z caps get no sun. The remaining ocean collects there: 74–86% of the water on cg4/cg8/cgg8/cge8 (t0 31%); the polar shell (`cgp8`) 44%. That is a cold-spot effect of the sun model, not flow toward −y (the caps straddle both hemispheres). Ice stays 0, since the caps still get greenhouse and conducted heat.
- **Wet/dry at sun 16 still collapses.** All sun-16 core runs dry the ocean to ~275–470 cells, with ~2,050–2,170 vapor and usually one wet region (cgg8 has 10). The doubled Full-sun input causes this, as in fs8. Gravity does not fix it. The energy-matched **cge8** is the healthiest core run: water 518, 7 wet regions (rim_wet 564), ice ≈ 0 (fe8: 149), evap 171k, and vapor flat at ~1.9–2.0k from 50k on (fe8 was still climbing).
- **σ_h drops (new issue).** Ocean: cg8 3.71 vs fs8 6.02 / rs8 4.98. cge8 2.93 vs fe8 5.55. cg0 1.75 (barely above t0 1.61). Rocky cg8 2.48 vs fs8 5.35 / rs8 2.83. Under world `−y`, the ocean piled on one side and flow/scour ran one way across the whole ball. That made a strong hemispheric weathering/sediment contrast, which drove a large part of σ_h. With coreward gravity, flow runs short and downslope only, so the relief stays lower. Single seed. It is unclear whether the old σ_h was "real" relief or a gravity artifact; given what caused it, probably partly an artifact.
- **Guard A/B (cgg8 vs cg8).** Shed 0 both, spill 0 both, books exact both, water below core 190 vs 161, σ_h 3.90 vs 3.71 (ocean) / 2.38 vs 2.48 (rocky). The guard is not needed for containment, so auto-off is safe. It is not neutral, though: forced on, the ocean keeps more wet regions (10 vs 1, rim_wet 665 vs 85) by holding a little more water in shell-adjacent cells. One seed, so the difference could be chaotic.
- **Polar shell (cgp8).** Shed 0 and water 312. The lowest below-core share (27%) and the fewest cap cells (145), but still one wet region.
- **Runaway check.** Every core run flattens by 100–150k (vapor < 1%/block). cg8 and cgg8 slowly re-wet (water 302 → 437 / 364 → 472) while vapor falls ~5%. cge8 settles after a 50–100k peak (615 → 518).
- **Identity.** `grav=down` 20k vs §5v `fs_*_H8` first 20k: ocean and rocky T/W lines are **IDENTICAL**, so `Down` is bit-for-bit the §5v model.
- **Books.** END `book_fail_total=0 max_abs_driftE=0 driftL=[0;12]` on all 8 core soaks (300k) and both id checks (20k). Reflected light is never injected; the sun code is unchanged from §5v.

**Verdict vs intent.**
- **Pass: water not ~100% below core.** 27–43% (ocean) and 35–55% of a 23–29-cell rocky puddle set vs 91–100%. The §5u/§5v pooling was gravity, and gravity now points at the core.
- **Pass:** books exact everywhere, `Down` identical to §5v, reflected light never injected.
- **Pass:** shed ~0 with the radial shell (0 on every H4/H8 run; 0.01% on H0); spill 0.
- **Pass: the guard can stay auto-off.** Containment is identical without it. It changes wet-region counts on one seed, which is worth a multi-seed check.
- **Fail / unchanged: wet/dry at sun 16.** One wet region, set by energy (Full at sun 16 = 2× input), not by gravity. The matched `cge8` keeps 7 wet regions but a weak cloud contrast (1.17×).
- **New issues:** σ_h roughly halves vs §5v; leftover water concentrates in the unlit equinox ±z caps; play Driven now defaults to coreward.

**Risks / next.**
- Core is the Driven **default**, so play's Driven opt-in changes behaviour (no wasm source change; the core was recompiled). Pin `Gravity::Down` if parity with §5b–§5v play is needed. Single seed, r12.
- The σ_h drop needs a driver study before anyone tunes erosion against it.
- Next options:
  - (a) Full + `sun=8` + core gravity (`cge8` config) as the soak baseline, plus a rocky cge8;
  - (b) tilt (`sweep=tilt:410:8`) under core gravity, to light the ±z caps and break the cap pooling;
  - (c) multi-seed / r16 of cg8 / cgg8 / cge8, to settle the guard A/B and σ_h;
  - (d) a σ_h driver study (scour/weather counts by latitude under Down vs Core);
  - (e) an opt-in play gravity / sweep toggle if Owen wants to see it live;
  - (f) mix τ / radial degassing / SED lithification / M2, as before.

**Files.**
- Code: `chem/vapor.rs` (`Gravity`), `chem.rs` (`coreward`, `outward_guard` auto, `set_outward_guard`, re-export), `chem/p6.rs` + `chem/p6_rest.rs` (gravity plumbing; gas rise, degas and collapse gates), `chem/p6_rest.rs` (`grav_core`, `inward_target`, `side_target`, `settle_core`, `flow_core`, scour/entrain gates), `chem/tests_p6.rs` (5 tests); `chem_soak` arg 15 `grav=`, skyshape `radialguard`.
- Measurement: `/workspace/scale_measure/core_grav/`: `soaks/{cg,cgg,cgp,cge}_r12_*`, `soaks/idchk_*_H8down.txt`, `smoke.txt`, `run_soaks.sh`, `make_summary.sh`, `summarize_rc.py`, `hemi.py`, `flow.py`, `traj.py`, `blocks_summary.txt`, `bin/chem_soak_cg`, `tests/`.
- Backups: `/workspace/backups_core_grav/{pre,post}/`, with `post/core_grav_core.diff`, `post/chem_soak.diff`, `post/core_grav_wasm.diff` (empty).
- www (core change compiled in; no wasm-crate source change). ABI 17. md5s (served at http://127.0.0.1:8765, bytes match):
  - wasm `6f2f994a3c18500eba288366dfccb4bc` (1,365,612 B)
  - play.html `f56e09185bd98960ec2b692dd19b2b97` (2,039,170 B)
  - bitblock-lab.html `3cdc0730e98be3eaa97220b0854c344b` (2,066,465 B)
  - bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged)
  - Chrome and Desktop have not been checked.

## 5x. σ_h driver study: Down vs Core (2026-10-28/29, measurement only; Owen pick "study what drives sigma_h under Down vs Core") — old high σ_h is mostly a −y pile-up artifact; do not tune erosion against it

**Question.** §5w dropped ocean σ_h ~halves (cg8 3.71 vs fs8 6.02; cge8 2.93 vs fe8 5.55). Was that "lost real relief," or was the Down number inflated by world −y gravity?

**Method.** Twelve paired 300k soaks, r12 seed 1, Driven, radial H=8, `sweep=full`, `walb=fresnel cloud=vapor vloc=patch rain=core evap=core`. Gravity `down|core` × energy `sun=8,day=600,core=40` (e8) or M1 sun16 (m1) × ocean/rocky, plus e8 `abl=noerode` (weather+scour forced off). Harness `/workspace/scale_measure/sigma_h_study/` (`bin/sigma_study` adds SX* face/cap diagnostics; simulation identical to chem_soak). Books checked every window. No play/wasm change.

**σ_h (+y face) and water, 250–300k means** (matches §5w within noise):

| run | σ_h Down | σ_h Core | Down/Core | water_lower% Down | Core |
|---|---|---|---|---|---|
| ocean e8 | 5.53 | 2.92 | 1.89× | 94% | 34% |
| ocean m1 | 6.03 | 3.71 | 1.63× | 100% | 37% |
| rocky e8 | 3.58 | 2.25 | 1.59× | 60% | 41% |
| rocky m1 | 5.34 | 2.47 | 2.16× | 96% | 38% |

**What drives the gap.**
- **Between-face height split (`betwF`).** Ocean Down 10.4 vs Core 0.05–0.20. Under Down, mean land height on −y is *positive* while +y is *negative* (e8: mean−y +5.69 vs mean+y −3.99). Core keeps both faces near −2.1 with above_datum only on +y. That hemispheric mean gap is most of Down's σ_h.
- **Wet-column contrast on +y.** Ocean Down wet-var 8–10 of σ²≈31–36 (deep wet columns at in≈−15…−19); Core wet-var ≈1–2 of σ²≈9–14. Ablation kills wet-var (8.07→0.21) but σ_h only falls 5.53→4.24 — so wet contrast is a booster, not the root.
- **Water location.** Down still pools 94–100% of ocean water at y < core; Core 31–41%. Caps under Core hold leftover water in ±z (same §5w issue); under Down the −y face is the cold pool.
- **Law activity is the opposite of the "scour asymmetry" guess.** Ocean Down: entrain ~105k with **91% on −y**, settle ~4k mostly low; scour almost dead (62–164). Ocean Core: scour ~19–21k, entrain ~17–19k mostly on **+y**, settle tiny and on +y. Plume/uplift totals are similar order. So Down's high σ_h is *not* from more weathering/scour carving the lit face — it is from −y flow dumping water+sediment into one hemisphere until heights diverge.

**Noerode ablation (weather+scour off, e8).**
| | Down σ_h | Core σ_h |
|---|---|---|
| ocean | 5.53 → **4.24** | 2.92 → 3.31 |
| rocky | 3.58 → **2.74** | 2.25 → 2.43 |

Turning erosion off closes only part of the ocean gap and nearly closes rocky e8; Core ocean σ_h even rises slightly. **Do not retune erosion rates to "recover" Down's σ_h.**

**Verdict.** The old high Down σ_h was **mostly a gravity artifact** (one-way −y pile → between-face height contrast + wet basin), not a lost lateral-relief driver. Core's ~2.9–3.7 (ocean) / ~2.3–2.5 (rocky) is the honest post-§5w baseline for this stack. Single seed, r12.

**Books.** All 12 END `book_fail_total=0 max_abs_driftE=0 driftL=[0;12]`. Soft DDA untouched; nothing committed; no play rebuild / Chrome / Desktop in this pass.

**Risks / next.**
- Treat Core σ_h as baseline; chase real lateral contrast (plume/lithify/M2), not Down parity.
- Next options (same as §5w minus the study): (a) Full+sun=8+core as soak baseline + rocky cge8; (b) tilt under core to light ±z caps; (c) multi-seed / r16; (d) play gravity/sweep toggle; (e) SED lithification → M2.

**Files.** `/workspace/scale_measure/sigma_h_study/` (`soaks/`, `analyze.py`, `run_soaks.sh`, `blocks_250_300k.txt`, `bin/sigma_study`); findings backup `/workspace/backups_sigma_h_study/`.

## 5y. Bigger scale + gas planet (2026-10-29, uncommitted; Owen ask pt226u "Figure out how to get a bigger scale and then show this off better by making gas planets") — exact network skip halves step cost, play cap 20 → 32, new opt-in `convect` law + gas preset 4 in play, books exact; new bottleneck is the CPU renderer

**1. What capped radius (measured, `scale_measure`).** Native single-thread, play defaults, books ok on every line (`gas_scale/bench_pre_sky6.txt`). Pre-change Driven sky6 per step: r12 rocky 5.75 ms, r20 13.1, r24 22.2, r32 43.9. Stage split at r32 rocky: **`comb2` (reaction-network cube build) 23.5 ms of 43.9**, heat+sun 7.5, erode+relief 8.5. `hot_reactive=0` on every preset: the network was building cubes for cells that cannot react (q < 250 or non-reactive material).

**2. Lever shipped: exact network skip (`chem/net.rs`, `is_reactive()`).** Cells with q < 250 or a non-reactive material skip the cube build; the outcome is identical by construction (no reaction can fire there). Chosen over full M2 (dormant bricks) because it is cheap, exact and needs no new books.
- Parity: `gas_scale/run_parity.sh` (rocky/ocean/ash × classic r12 2000 / driven r12 2000 / driven r20 600, hash + drift + books every 250 ticks): **9/9 SAME** pre vs post-skip vs post-convect (`pre/ post/ post2/`).
- Post-skip Driven sky6 (`bench_postskip_sky6.txt`): r12 rocky 2.06 ms (was 5.75), r20 6.27 (13.1), r24 11.6 (22.2), r32 22.9 (43.9) — **~2× at every radius**; comb2 r12 3.70 → 0.52 ms.
- Play-default classic step, no sky (`classic_play_postskip.txt`): r12 0.35–0.43 ms, r20 1.7–2.0, r24 3.1–3.4, r28 5.1–5.9, r32 7.6–8.4.
- On that basis **play `CHEM_RADIUS_MAX` 20 → 32** (`play_chem.rs`; HTML radius input max 32). Default radius stays 12.
- Also: `PlaySession::mirror_chem` now only probes cells whose material changed since the last mirror (full scan when the cache is empty/resized). Exact, but small win: mirror+commit was only 0.01–1.2 ms.

**3. New law: `P6Params.convect` (opt-in, default false; `p6_rest.rs::convect_pass`).** Driven + core gravity only. Ideal-gas buoyancy: for an outward pair of rising-gas cells a (inner) / b (outer), swap mat and q when the inner parcel is lighter per volume, `m_b·T_a·1010 < m_a·T_b·1000` (T = Q + 300, molar masses from lanes, 1% margin), at odds 500/1000 from the wide roll. Pure swap: atoms, q and energy books unchanged. Reported as `StepReport.convected`. Test `convect_is_opt_in_and_conserves` (off ⇒ 0 swaps, fingerprint = plain params; on ⇒ swaps > 0, books ok, drift 0).

**4. Gas planet preset 4 (`play_chem.rs`, ABI 17 additive).** Budget: Fe n/20, Ni n/100, Si n/50, Mg n/50, H2S n/25, NH3 n/5, CH4 the rest. No water, no rock, no sky shell, no noise terrain. Loads with energy **sun8** (sun 8, day 600, core 40, `SunSweep::Full`), **Driven**, `Gravity::Core`, `convect=true`.
- What it shows: settled Fe/Ni core, thin Si/Mg mantle, an H2S shell, then a CH4 envelope with NH3 plumes mixing through it. Cross-section (x half) shows the layering and mottled envelope (`gas_r20_xhalf.png`); outside it is a mostly uniform CH4 ball with speckled NH3 (`gas_r20_play.png`).
- **Not claimed:** bands, storms, rotation, Coriolis or zonal flow. The stack has none of them; the visible dynamics are convective overturn only.
- Native gas step, Driven + convect, no sky (`gas_bench_sky0.txt`): r12 0.51 ms, r16 1.21, r20 2.50, r24 4.43, r28 9.9, r32 11.6; books ok on all. r16 6000-step probe: books true throughout, ~600–750 swaps/tick, envelope still cooling.

**5. Play wiring.** `bitblock_play_load_chem(4, seed, r)`; `bitblock_play_chem_dynamics` accepts energy 2 = sun8; `_dynamics_get` = `1 | (e&1)<<1 | c<<2 | (e>>1)<<3` (energy 0/1 encode as before). Play menu: "chem · gas planet", energy option "sun8 matched (full sweep)". First switch to gas sets energy sun8 + cycle driven and, if radius ≤ 12, radius 20 (later choices stick). wasm tests 77/77, core 298 passed / 3 ignored.

**6. Self-playtest (Chrome, http://127.0.0.1:8765/play.html, Low 960×540, chem every 4 ticks).** Fresh load → gas: radius 20, sun8, driven, `books=ok energy_drift=0`, lane drift all 0, `error=none`, no console errors; still books ok at tick 1089.

| gas radius | raf fps | presented fps | step_ms avg | main thread busy |
|---|---|---|---|---|
| r16 | 59.7 | 14.9 | 5.8 | 35% |
| r20 | 52.4–52.5 | 15.0 | 8.6–8.7 | 46% |
| r24 | 30.0 | 15.0 | 24.7 | 75% |
| r32 | 6.0 | 6.0 | 151 | 91% |

High 1600×900 r24 gas: raf 13.2 fps. Presented 15 fps = the full chem cadence (60 Hz / 4).

**7. New bottleneck: the CPU renderer.** Native play stage profile at 1600×900 (`play_stage_prof`): r24 gas chem 5.0 ms, mirror+commit 0.12, **render 18.3**; r32 gas chem 12.8, render 35.6; r32 rocky chem 8, render 36. The renderer repaints on every chem step (gas materials change every step) and its cost grows ~linearly with radius (DDA ray length), not with pixels. wasm in node, 960×540 frame with a chem step: r12 ~5.4 ms, r24 26–28, r32 57–69. Renderer untouched this pass (Soft DDA off-limits).

**Verdict.** Bigger scale is real and exact: step ~2× cheaper, cap 32, gas demo smooth at r20 (4.6× the r12 cells) and usable at r24; r32 loads and keeps books but is a ~6 fps slideshow because of rendering, not chem. No fake worldgen.

**Next.** (a) Renderer lever: dirty-region / changed-brick repaint or lower-res chem view (needs Owen OK to touch the render path); (b) M2 dormant bricks for chem beyond r32; (c) rotation/Coriolis term before any band claim; (d) gas heat balance (envelope still cooling at 6k).

**Build.** `RUSTFLAGS="-C target-feature=+simd128" CARGO_PROFILE_RELEASE_PANIC=abort cargo build --release --target wasm32-unknown-unknown -p bitblock-wasm`, copy `bitblock_wasm.wasm` to `crates/wasm/www/`, `python3 crates/wasm/build_pages.py`. www md5: wasm `857afca0d6654413813f83275ba04ab9` (1,369,598 B), play.html `dd70939bdbf5cb1fdaaca1cd532a7899`, bitblock-lab.html `1b79631728bbda6ff0020ddcd2cf4f6e`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged). Soft DDA / bcc-voxel-demo / abi16 untouched; nothing committed.

## 5z. Render scale: exact enclosed-bit cull + streamed world tag (2026-11-16, branch `chem/render-scale-2026-11`; Owen "continue" pt243u on the render-scale wave) — gas r32 raf 30 → 44, ocean r32 9.5 → 36 raf / 15 presented, every hash and book unchanged

**0. Scope.** This is the first change to the BitBlock CPU render path (`crates/core/src/render.rs`). It was made under Owen's "continue" on the render-scale branch, and §7 rule 1 asks for that explicit OK. Soft DDA, `bcc-voxel-demo` and abi16 are untouched. Correction to §5y.7: `render.rs` is an ordered front-to-back triangle rasterizer with tile depth bounds and chunk frustum culling, not a DDA ray caster. Its cost grows with the number of *submitted bits*, which is the whole ball volume, not with pixels.

**1. Profile before (7634a08).** Native play stage profile at 1600×900 with the sidebar (`scale_measure/render_scale_prof`, play defaults, chem every 4 ticks; step4 = one chem step + 4 frames):

| case | chem ms | render ms | step4 ms |
|---|---|---|---|
| gas r20 | 2.71 | 12.20 | 15.92 |
| gas r32 | 13.03 | 35.08 | 52.90 |
| ocean r20 | 2.62 | 12.12 | 18.80 |
| ocean r32 | 11.62 | 33.34 | 61.20 |

`world_px` stays ~137k at every radius, so the render cost is per submitted bit. The named Chrome CPU profile at 960×540 Low, self ms over the same window, shows:
- Gas r32: `draw_bit_with` 3540, `raster_triangle_spans` 1147, `Topology::offset` 880, `ChemWorld::step` 776, `combine2_pass` 626, `heat_pass` 414, `render_tinted` 320.
- Ocean r32: `draw_bit_with` 2871, `sha256 compress` 1522, sort (quicksort) 1108, `raster_triangle_spans` 900, `Topology::offset` 884, `update_octree_ancestors` 779, `set_off` 623, `history::world_tag` 404.

Rank: (1) per-bit hidden tests on interior bits; (2) ocean only: `world_tag` collects and sorts every coordinate on each tick that changes the world.

**2. Lever 1: deeply enclosed bits are never submitted (5134b28).** `World::chunk_open_words` returns a chunk's on bits minus every bit whose full 3×3×3 neighbourhood is on. It uses a separable erosion over an 18×18 padded grid of 18-bit x rows, reads 27 chunks, and uses checked i64 ends. `render_tinted` uses these words instead of the raw chunk words when `enclosed_cull_applies`:
- every corner of the chunk box grown by 1 is in front of the camera (view depth > 1e-3), and
- the projected bit span stays ≥ 2·`SMALL_BIT_EXTENT`, so the single-pixel path is never taken.

Why the cull is exact: a deeply enclosed cube is inside the closed union of its 26 neighbours. Under those conditions every pixel it would cover is already covered by a neighbour at strictly smaller depth, with a margin. Equal depth is impossible, so the lower-RGB tie rule never picks it.

**Crease finding.** A 6-face enclosure, which was faster (r32 render 12.0 ms), is *not* exact. At a concave crease, the culled bit and the visible face tie in depth, and the tie rule then prefers the hidden bit's lower RGB (0x989898 < 0xbdbdbd). The host known answer `recursion_dead_frontal_801_matches_host_known_answer` changed on 35 diagonal pixels. The 3×3×3 rule restores it bit for bit.

Tests:
- `chunk_open_words_match_the_neighbourhood_definition`: random worlds, i64 corners.
- `enclosed_bit_cull_matches_reference_traversal`: balls r7/13/17, a solid, a shell and dense random worlds; fixed and random cameras; tinted and plain; byte-identical frames, >100k bits culled.
- `enclosed_bit_cull_is_exact_on_dead_frontal_creases`: recursion.bb at the 801/401 poses.
- An end-of-frame verifier (`verify_enclosed_bits`) rasterizes every culled bit over the finished frame and asserts it writes nothing.

**3. Lever 2: `world_tag` streams coordinates in order (51219ab).** `World::for_each_on_ordered` sorts the chunk keys once and walks x-slab → lx → y row → ly → z chunk → lz, giving the same ascending (x, y, z) sequence as the old collect and sort. `world_tag` hashes it in 256-coordinate blocks. The canonical bytes and every tag are unchanged. Test: `ordered_world_tag_matches_the_sorting_oracle_on_random_worlds` (24 worlds, i64 extremes, bits switched off).

Native `world_tag` on a solid ball, identical tags:

| radius | bits | before ms | after ms |
|---|---|---|---|
| r20 | 33k | 2.27 | 0.49 |
| r32 | 137k | 9.77 | 1.90 |
| r48 | 463k | 37.9 | 6.3 |

**4. Parity.** A play harness compared 7634a08 with the new code over 13 scenarios: the default world, gallery 0–3, chem p4 r20/r32, p2 r12/r20, p1 r16, p0 r12, a 960×540 game-only view and a 640×480 gallery view. Each ran 300 audited fixed ticks with scripted look, wheel, W/A, F and cut inputs. Results:
- Frame hashes are identical (3,900 lines, 3,719 distinct frames).
- World tags are identical (237 distinct).
- All 2,100 chem book checks are ok, with energy drift 0.

Default play world `#bb.world.sha256.lqpba5lv…` and frame `#bb.frame.sha256.5l3xbpo3…` are unchanged. Chem semantics are untouched (no chem file changed).

**5. Native after:**

| case | chem ms | render ms | step4 ms |
|---|---|---|---|
| gas r20 | 3.18 | 7.96 | 11.66 |
| gas r32 | 12.57 | **14.03** (35.08) | **28.14** (52.90) |
| ocean r20 | 2.64 | 7.98 | 12.42 (18.80) |
| ocean r32 | 11.58 | **14.20** (33.34) | **30.44** (61.20) |

Mirror+commit at ocean r32 is 0.48 ms (was 1.25).

**6. Browser before → after.** Chrome headless on http://127.0.0.1:8765/play.html, Low 960×540, output 1360×540, chem every 4 ticks. Averages of 5 windows each; the 7634a08 build is compared with the 07f1f40 build:

| case | raf fps | presented fps | step ms | ms / presented frame | main thread busy |
|---|---|---|---|---|---|
| gas r20 | 59.7 → 59.7 | 15.2 → 14.8 | 4.15 → 2.77 | 16.6 → 11.5 | 25% → 17% |
| gas r32 | 30.0 → **44.4** | 15.0 → 15.1 | 27.5 → 12.1 | 55.2 → 35.8 | 83% → 54% |
| ocean r20 | 58.5 → 60.4 | 15.0 → 15.0 | 6.11 → 3.76 | 24.0 → 15.4 | 36% → 23% |
| ocean r32 | 9.5 → **35.6** | 9.5 → **15.0** | 101.6 → 20.7 | 102 → 47.9 | 96% → 72% |

15 presented fps is the full chem cadence (60/4), so every r32 world now presents every chem step. Books ok and energy_drift 0 in every run; only the favicon returns 404.

Named profile after:
- Gas r32: `draw_bit_with` 3540 → 602, `raster_triangle_spans` 1134. Chem is now the bulk: `Topology::offset` 831, `step` 764, `combine2` 609, `heat` 433.
- Ocean r32: quicksort 1108 → 0, `draw_bit_with` 2871 → 719. Still high: `sha256` 1320, `raster_triangle_spans` 1143, `Topology::offset` 719, `update_octree_ancestors` 633, `set_off` 536.

Screenshots: `/workspace/render_scale/{before,after}_{4,2}_r{20,32}.png`. Before and after look the same.

**7. Next bottleneck.**
- (a) At r32, chem ≈ render (~12 vs ~14 ms native). Chem levers: `Topology::offset`, combine2 and heat inner loops, or M2 dormant bricks.
- (b) Render: what remains is the 2-layer surface shell plus back-hemisphere bits. A conservative chunk- or brick-level occlusion test is next, held to the same exactness bar.
- (c) Ocean: `world_tag` SHA-256 is still O(N) per edit tick, plus octree ancestor upkeep and `set_off`. An incremental or Merkle world tag would change every world hash, so it is a protocol decision for Owen, not a perf patch.

**Build.** As §5y, from 51219ab. www: wasm md5 `f966135c4a114ee89b843015ddea096f` (1,396,520 B, reproducible from a clean target dir), play.html `dbd39e7270ad634dfe13a646a2686809`, bitblock-lab.html `491624b074aba55c8f0fbf24ce7e021b`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged). ABI 17. Commits: 5134b28 (cull), 51219ab (world_tag), 07f1f40 (www). Desktop: b17d0cd copied to `C:\Users\Owen\Desktop\bitblock\` on 2026-11-16 (Owen OK pt244u), md5s verified against the above (wasm `f966135c…`, play.html `dbd39e72…`, bitblock-lab.html `491624b0…`, bitblock.html `814320ae…`).

## 6a. Radius scaling past r32: cache-miss-free chem + 1.25 px enclosed cull; play cap 32 → 36 (2026-11-16, branch `chem/render-scale-2026-11`; Owen "yes" pt246u) — every hash and book unchanged

Continues the §5 series after §5z. The brief was to measure gas (preset 4, Driven) and ocean (preset 2, Classic) at r32/48/64/96, fix the first wall exactly, and raise the play cap to the largest radius that still plays.

**1. Method.**
- Native: `scale_measure/radius_study <preset> <r> <n>`. Each run does genesis, session init, 2 warm-up chem steps (the first step builds the topology, so it is excluded), then n timed rounds of world_tag, chem step, mirror commit, render (1600×900) and a full step4. It also records RSS and the high-water mark. The play cap was raised to 256 in a scratch copy only.
- Browser: headless Chrome via `perf2.mjs` on play.html, Low quality at 960×540, about 15 s per world. It records raf fps, presented fps (one frame per chem step; 15 is the full 60/4 cadence), step ms and wasm memory.
- Before = b17d0cd (§5z). After = this wave. Every run ends `books=ok energy_drift=0 error=none`. r128 is refused because `MAX_GENESIS_RADIUS = 103` in core `chem.rs` is a hard wall, which was left unchanged.

**2. Native (ms per chem step; step4 = chem + mirror + render + tag).**

| case | cells | chem ms | render ms | world_tag ms | step4 ms | init s | RSS hwm MB |
|---|---|---|---|---|---|---|---|
| gas r32 | 137k | 13.0 → 7.2 | 15.1 → 13.9 | 2.0 | 30.8 → 23.1 | 0.27 | 72 |
| gas r48 | 463k | 58.7 → 28.4 | 31.2 → 31.0 | 6.5 | 97.2 → 61.4 | 0.77 | 170 |
| gas r64 | 1.10M | 206 → 70.6 | 52.4 → 48.6 | 15.2 | 254 → 126 | 1.7 | 318 → 327 |
| gas r96 | 3.71M | 925 → 322 | 453 → 103 | 51 | 1487 → 443 | 6.0 | 1156 → 1090 |
| ocean r32 | 137k | 9.8 → 7.5 | 14.2 → 14.4 | 2.0 | 29.8 → 26.9 | 0.27 | 73 |
| ocean r48 | 463k | 33.6 → 25.6 | 31.5 → 31.3 | 7 | 80.6 → 71.3 | 0.80 | 170 |
| ocean r64 | 1.10M | 99.2 → 59.7 | 49.6 → 50.2 | 15.2 | 184 → 145 | 1.7 | 327 |
| ocean r96 | 3.71M | 399 → 225 | 431 → 115 | 51 | 922 → 438 | 5.9 | 1092 |

Chunks by radius (r32/48/64/96): 70 / 195 / 414 / 1253. Genesis at r96 takes 4.7–4.9 s.

**3. Chrome (raf / presented fps, step ms).**

| case | before (b17d0cd) | after | wasm MB | init s |
|---|---|---|---|---|
| gas r32 | 45.6 / 15.0, 10.8 | 58.8 / 15.0, 6.8 | 80 | 0.33 |
| ocean r32 | 40.0 / 15.0, 16.9 | 44.9 / 15.0, 14.0 | 72 | 0.35 |
| gas r36 (shipped) | — | 45.0 / 15.0, 11.6 | 86 | 0.46 |
| ocean r36 (shipped) | — | 24.4–27.9 / 14.9–15.0, 30.5–40.8 | 78 | 0.47 |
| ocean r38 | — | 13.1 / 13.1, 71 | 110 | 0.57 |
| gas r40 | — | 36.9 / 15.0, 17.8 | 128 | 0.64 |
| ocean r40 | — | 7.3 / 7.3, 126 | 120 | 0.65 |
| gas r48 | 5.1, 180 | 9.4, 93 | 205 | 1.1 |
| ocean r48 | 4.0, 230 | 4.6, 199 | 197 | 1.2 |
| gas r64 | 1.37, 676 | 3.37, 260 | 402 | 2.7 |
| ocean r64 | 1.37, 686 | 1.97, 470 | 393–427 | 2.7 |
| gas r96 | 0.30, 2854 | 0.60, 1511 | 1292 | 10.3 |
| ocean r96 | 0.40, 2445 | 0.50, 1941 | 1284–1395 | 10.3 |

**4. First wall: chem combine2 grows superlinearly from cache misses.** A per-stage profile at r64 gas showed combine2 at 149 ms out of a 206 ms step. The cost went beyond O(N): gas rise and `convect_pass` walk the coreward `rsort` order and look up each cell's outward neighbour through `Topology::offset` (a hashed or boxed lookup per cell). The network law also reallocated about 55 B/cell of scratch on every step. Three exact fixes (be4f403):
- `Topology::rsort_up`: a precomputed outward-neighbour slot per rsort entry (`NO_SLOT` at the core or off-domain), built once in `radial_frame`, plus `rsort_outward_first()`. Gas rise and convect visit the same cells in the same order, with the same outward slot, as the offset lookup. Without a core they fall back to the old `rise` list.
- `nb6` fast path for `Lookup::Boxed`: base index ±sx/±sy/±1 with per-axis bounds. It returns the same slot as `offset()`, including on holey and edge boxes.
- `net::NetScratch`: the network pass's prov/done/cursor/vis_id/fids/nnbs/intent/claim buffers are kept on `Chem` and reused. `prepare(n)` clears every buffer whose contents are read before they are written, and resizes the rest. Because every read sees exactly what a fresh allocation would hold, the output is identical. `Clone` returns empty scratch so a cloned Chem does not share or leak state.

Result: gas combine2 at r64 went 149 → 39 ms and heat 28 → 18 ms. The new test `topology_fast_paths_match_offset_lookups` checks boxed, holey and hashed topologies and i64 edges against `offset()`, plus `rsort_up`.

**5. Render wall at r96: enclosed cull disabled below 2 px.** With a world this large on screen, most bits project to 1–2 px, so the §5z cull (which needed span ≥ 2 px) was off for almost the whole volume, and render cost 453 ms. The bound can be tightened exactly. A bit at distance D whose bounding sphere has radius ρ projects to an image whose minor axis is at least fρ/√(D²−ρ²) > f/(2D) times its extent. At ≥ 1.25 px, the 3×3×3 neighbours' splats still cover the centre bit's full footprint, so hiding it changes no pixel. Below 1 px, single-pixel splats can leave gaps, so the cull is **not** exact there and stays off. `ENCLOSED_MIN_SPAN = 1.25 · SMALL_BIT_EXTENT` (4226569). The new test `enclosed_bit_cull_is_exact_on_one_to_two_pixel_bits` culls about 1.41M bits (the old bound culled about 15k) and asserts pixels and depth bit-for-bit against both the reference traversal and the uncut renderer. Render at r96 went 453 → 103 ms (gas) and 431 → 115 ms (ocean).

**6. Exactness evidence.**
- p6 goldens: `cargo test -p bitblock-core --release -- --ignored` passes.
- The chemparity harness (`render_scale/chemparity`) compares base b17d0cd against this wave byte for byte in three modes: `chem` (242 lines: per-step world tags and books over presets and radii), `play` (180 lines: play-session world and frame hashes) and `big` (80 lines: 68 distinct frames at r48–r96). All three are identical. The default play hashes are unchanged: world `lqpba5lv…`, frame `5l3xbpo3…`.
- world_tag hashing semantics are untouched (Owen's call).

**7. Cap: `CHEM_RADIUS_MAX` 32 → 36 (bc8d9ec).** The rule is that a radius is playable if both presets keep presented fps at about 15 (every chem step shown) in headless Chrome at Low.
- Gas holds 15 up to r40 (36.9 raf).
- Ocean is the binding preset: r34 30.7/15.0, r36 24.4–27.9/14.9–15.0, r38 drops to 13.1, r40 to 7.3.
- 36 is therefore the largest radius where both presets play at full cadence. The slider (`play.template.html` max=36) and `playNum(…, 2, 36)` match the Rust clamp.

**8. Next wall.**
- (a) Ocean in the browser: the SHA-256 `world_tag` runs on every history commit with edits (history.rs:602), and ocean edits every chem step. wasm SHA-256 is software, about 11 MB hashed per edit tick at r48. An incremental or Merkle tag changes every world hash, so it is Owen's protocol call.
- (b) Ocean edit upkeep: `update_octree_ancestors` plus `set_off`/`set_on` on both Worlds per edit.
- (c) Gas: convect performs real swaps (about 215k per step at r64), and heat is now the largest per-cell law. M2 dormant bricks would need a proof or a flag-gated equivalence test.
- (d) Render below 1 px is not exactly cullable, so LOD there would be non-exact and opt-in only.
- (e) Memory and init: r96 is about 1.3 GB wasm and 10 s init (genesis 4.7 s native). Genesis is capped at r103.

**Build.** wasm (bc8d9ec, ABI 17) md5 `50eb47dd86da1468a855f6653e651829` (1,397,658 B), play.html `f1df94b6e8f66e9c97c5f983c9308d0e`, bitblock-lab.html `4d49429b1a52cced634782c72d7e610d`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged). Served at 127.0.0.1:8765 and self-playtested: default hashes unchanged; ocean r12, gas r20 and gas/ocean r36 all `books=ok energy_drift=0 error=none`; only the favicon returns 404. Screenshots: `/workspace/render_scale/scaling/ship_{4,2}_r36.png`, `{base,new2}_*.png`, `smoke_gas_r20_8765_bc8d9ec.png`. Commits: be4f403 (chem), 4226569 (render cull), bc8d9ec (cap), a837795 (www). Soft DDA, bcc-voxel-demo and abi16 are untouched. No Desktop copy of this wave (Owen to decide); main untouched.

## 6b. Exact speedups under the unchanged world_tag: edit upkeep, init/memory, heat and convect; play cap 36 → 40 (2026-11-16, branch `chem/render-scale-2026-11`; Owen: "keep world_tag hashing semantics as they are … do exact speedups first") — every hash and book unchanged

The brief built on §6a's next walls (b), (c) and (e). world_tag hashing semantics stay as they are: no incremental tag and no hash changes. The brief asked for exact speedups to ocean edit upkeep, gas convect/heat, and memory/init at large radii, then a re-measure at r32–r96 and a cap raise only if both presets hold about 15 presented fps. Before = 6475973 (§6a). After = e01710c/5a8393a.

**1. Changes (all exact; code commits are separate from docs).**
- e2ee74a, 0d5647a, 0e71f88 — **deferred octree upkeep.** `World::defer_octree` / `flush_octree`: bits, counts, chunk bounds and `len` stay eager, while octree ancestors and stale world bounds are rebuilt once per changed chunk at the flush. Each level maps to parent octants, sorts, dedupes and keeps the max revision, which is the stamp eager upkeep would have left. While deferred, reading the octree or `bounds()` panics (fail closed). Used around a chem step (including the profiled steps), genesis, `from_cells` and the play mirror. Tests: `deferred_octree_flush_matches_eager_upkeep_exactly` (snapshot compare, including ~90% mass-delete batches; mutation-checked) and `deferred_octree_reads_fail_closed`.
- 69684b9 — a chunk's tight bounds are recomputed from whole words when a boundary bit is deleted (`word_level_chunk_bounds_match_bit_iteration`).
- 1415028 — **fast deterministic hasher** (Fx-style `CoordHasher`) for World's private chunk and octant maps. Iteration order cannot reach any output. `world_tag` and `for_each_on_ordered` sort chunks first. `recompute_bounds` is a commutative min/max merge. Render resolves equal-depth fragments by the lower RGB word, which render.rs documents as being so that unspecified world order cannot change a pixel. The other `iter_on` / `iter_chunks` callers sort, count or are tests. Native std `HashMap` order was already random per process while every golden held.
- 02bafc7 — **genesis mint provenance as a bitset baseline plus exceptions.** History keeps the genesis mints as a `World` bitset, plus a canonical override map and a `circulating` counter instead of one map entry per bit. `active_mint` falls back to the baseline, and verify compares the overrides and the counter (`genesis_baseline_mints_match_the_full_mint_map`).
- cac05a8 — play reuses the genesis world tag that History already computed (it used to hash the world a second time at 3 sites).
- a7cf068 — `ChemWorld::index_of` uses the cached topology box (`Topology::locate`, an O(1) probe) instead of a binary search. It was hot because the render tint looks up every drawn bit's material. The test checks it against the binary search on and off the domain and at i64 extremes; mutation-checked.
- 827981e — heat pass: the rim mask comes from the static `Topology::space_faces` (same masks, index order) instead of a per-step `surface` vector, and neighbours are probed only for cells that emit (`o > 0`).
- 372b4e8 — convection rolls reuse the salt's precomputed first link of the `wide_roll` chain (`wide_roll_key` / `roll_keyed`). The test `keyed_wide_roll_matches_the_five_link_chain` runs 20k random inputs against the written-out chain.
- e01710c — cap 36 → 40. 5a8393a — wasm-only dead-code gate on the profiled-step bodies.

**2. SHA-256 itself (the ocean wall) could not be made faster exactly.** In V8, sha2 0.10.9's software path runs about 376 MB/s warm. A hand scalar compress managed 302 MB/s, and a SIMD message schedule took 33.9 ms vs sha2's 29.2 ms on 10.6 MB. The tag is already computed once per edit tick and was never recomputed when nothing changed. A byte-identical cache is not possible because each edit tick changes the bytes. So sha2 stays, and `sha256::compress256` is still about 26% of ocean wasm time at r40.

**3. Native (`radius_study`, ms per round; step4 = tag + chem + mirror + render).**

| case | genesis ms | session init ms | chem ms | mirror+commit ms | step4 ms | session RSS MB | RSS hwm MB |
|---|---|---|---|---|---|---|---|
| ocean r32 | 166 → 33 | 280 → 116 | 8.2 → 6.8 | 2.4 → 2.3 | 27.6 → 24.4 | 61 → 42 | 72 → 56 |
| ocean r40 | 323 → 68 | 469 → 160 | 18.2 → 13.5 | 4.9 → 4.6 | 46.1 → 40.0 | 98 → 58 | 109 → 78 |
| ocean r48 | 598 → 106 | 751 → 200 | 27.0 → 22.0 | 7.9 → 7.5 | 72.1 → 65.0 | 137 → 61 | 170 → 104 |
| ocean r64 | 1366 → 284 | 1711 → 417 | 65.2 → 54.6 | 18.6 → 17.9 | 153.9 → 130.9 | 209 → 75 | 328 → 198 |
| ocean r96 | 4671 → 1011 | 5870 → 1225 | 226 → 200 | 66.7 → 62.2 | 446 → 379 | 674 → 156 | 1093 → 573 |
| gas r32 | 164 → 37 | 278 → 127 | 7.3 → 7.0 | 0.7 → 0.4 | 23.6 → 23.5 | 61 → 42 | 73 → 56 |
| gas r40 | 326 → 64 | 451 → 148 | 14.2 → 14.2 | 1.2 → 0.7 | 38.1 → 37.9 | 98 → 58 | 110 → 77 |
| gas r48 | 584 → 125 | 781 → 212 | 31.1 → 27.8 | 1.6 → 1.0 | 63.6 → 59.0 | 137 → 61 | 170 → 104 |
| gas r64 | 1398 → 287 | 1744 → 420 | 74.5 → 70.2 | 2.7 → 1.8 | 141.6 → 121.7 | 209 → 75 | 327 → 196 |
| gas r96 | 4830 → 1011 | 6061 → 1261 | 331 → 312 | 11.1 → 10.7 | 427 → 415 | 674 → 156 | 1090 → 568 |

r36 is within noise in both runs: ocean step4 34.5 → 38.0 and gas 29.5 → 29.5 ms. Native SHA uses hardware SHA-NI, so the native world_tag (2–52 ms) is not the browser wall.

**4. Chrome (headless, Low 960×540; raf / presented fps, step ms; init s).**

| case | before (6475973) | after |
|---|---|---|
| ocean r32 | 44.7 / 15.1, 14.1; 0.36 | 45.0 / 15.0, 11.7; 0.14 |
| ocean r36 | 28.1 / 15.0, 30.6; 0.50 | 31.3–41.8 / 15.0, 16.6–26.1; 0.19 |
| ocean r38 | 12.4 / 12.4, 75.0; 0.56 | 29.5 / 15.1, 26.1; 0.22 |
| **ocean r40** | 7.6 / 7.6, 122; 0.66 | **24.3–30.0 / 14.8–15.0, 28.8–38.7; 0.26** |
| ocean r42 | — | 13.3 / 13.3, 71.1; 0.28 |
| ocean r48 | 4.43, 208; 1.16 | 6.00, 156; 0.43 |
| ocean r64 | 1.93, 471; 2.6 | 2.53, 372; 1.1 |
| ocean r96 | 0.47, 2003; 10.6 | 0.60, 1620; 4.6 |
| gas r32 | 58.8 / 15.0, 7.0; 0.34 | 60.0 / 15.0, 6.4; 0.14 |
| gas r36 | 43.6 / 14.9, 12.3; 0.50 | 45.3–50.5 / 14.9–15.0, 9.9–11.3; 0.18 |
| **gas r40** | 35.3 / 14.8, 19.7; 0.71 | **39.4–44.4 / 15.0, 14.0–17.2; 0.24–0.28** |
| gas r44 | 17.1 / 15.3, 51.4; 0.91 | 30.0 / 15.0, 26.2; 0.32 |
| gas r48 | 8.27, 106; 1.17 | 13.6, 67.9; 0.43 |
| gas r64 | 3.17, 278; 2.8 | 3.57, 258; 1.1 |
| gas r96 | 0.60, 1562; 10.8 | 0.70, 1420; 4.5 |

Ranges are repeat runs (sweep, repeat, and the shipped :8765 build). Every run ends `books=ok energy_drift=0 error=none`. node/V8 step4 (wasm, warm 8, n 16) before → after: ocean r38 61.7 → 51.4, ocean r40 69.5 → 58.5, gas r40 45.5 → 41.7, gas r48 71.2 → 67.9 ms. Browser init is 2.5–2.7× faster at every radius.

**5. Cap: `CHEM_RADIUS_MAX` 36 → 40 (e01710c).** Under the §6a rule, ocean is still the binding preset: it holds about 15 presented fps through r40 (14.8–15.0 across four runs, 58–61 ms per presented frame, the same margin r36 had at 6475973) and drops to 13.3 at r42. Gas holds 15 through r44. The slider (`max="40"`), `playNum(…, 2, 40)` and the Rust clamp match.

**6. Exactness evidence.**
- p6 goldens: `cargo test -p bitblock-core --release -- --ignored` passes (3/3).
- The parity harness (`render_scale/parity6b`, cap lifted in a scratch copy) compares 6475973 against this wave byte for byte: chem (242 lines), play (180) and big (80) are all identical (md5 4e28b61e… / 654b17e5… / 706eb371…), re-run after every code commit.
- A wasm parity script compares world, frame and books between the 6475973 and current wasm builds on 7 cases (ocean/gas r12–r48, default world): all SAME.
- The default play hashes on :8765 are unchanged: world `lqpba5lv…`, frame `5l3xbpo3…`.

**7. Next wall.**
- (a) Ocean in the browser: software SHA-256 `world_tag` is about 26% of ocean wasm time at r40 and grows with N. An incremental or Merkle tag remains Owen's protocol call.
- (b) Render, about 26% at r40: `raster_triangle_spans` plus `draw_bit_with`. Below 1 px the enclosed cull is not exact, so further gains there are LOD (non-exact, opt-in only).
- (c) Gas past r44: convect swaps and heat/nb6 per-cell work are now cache-bound, not branch-bound. The next exact step would be a locality reorder (rsort-ordered gather) or a static neighbour table (+24 B/cell, which conflicts with memory).
- (d) Genesis is still capped at r103 (`MAX_GENESIS_RADIUS`). r96 now runs in about 0.57 GB native hwm and 4.5 s browser init.

**Build.** wasm (5a8393a, ABI 17) md5 `fd7b983cfbfe7cea19478689cc770ea0` (1,402,147 B), play.html `89f14f23ef48cda68dccf68d314b4d7c`, bitblock-lab.html `0935072f3f9069d0487241a6693df5db`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged); www commit 9b8fc5a. Served at 127.0.0.1:8765 and self-playtested: default hashes unchanged; ocean r12, gas r20 and ocean/gas r36/r40 all `books=ok energy_drift=0 error=none`; the lab reports ABI 17; only the favicon returns 404. Screenshots: `/workspace/render_scale/s6b/ship/{ship_2_r40,ship_4_r40,ship_2_r36,ship_4_r36,smoke_gas_r20_8765_9b8fc5a}.png` and the sweep shots `/workspace/render_scale/s6b/chrome/{ref,cur}_*.png`. Soft DDA, bcc-voxel-demo and abi16 are untouched. No Desktop copy; main untouched.

## 6c. Exact chem locality: rsort-ordered convection, z-run heat; render left at its exact floor; play cap 40 → 42 (2026-11-17, branch `chem/render-scale-2026-11`; Owen "continue" pt247u) — every hash and book unchanged

The brief built on §6b's next walls: reorder chem storage for locality (gas past r44), exact render speedups above the 1 px floor, re-measure r40–r96, and raise the cap only if both presets hold about 15 presented fps. world_tag semantics are unchanged (Owen's standing choice). Before = 5a032e9 (§6b). After = fe536d3.

**1. Why not a global Morton or shell relayout of the cell arrays.** Every chem law that mutates while it scans (combine, combine2's hand families and network law, settle, flow) defines its result through index order 0..n, and each such scan draws its rolls in that order. Renumbering the cells would force each of those scans through a permutation back to the old order, giving back the locality. So this wave reorders data per pass instead, and only where the pass's own order is a separate array (convection walks `rsort`) or where order cannot matter (heat sums commute).

**2. Changes (all exact).**
- 2d9c7af — **convection roll table.** `wide_roll` is split into `wide_roll_cell(key, c)` (the salt and coordinate links, static for a cell under one salt) and `wide_roll_finish(cell, t)`. The topology keeps `wide_roll_cell` for every rsort entry under the convection key (`rsort_roll_cells`, built on first use; another key gets `None` and rolls per cell). Convection then reads one sequential word per roll instead of a random 24 B coordinate plus three mixes. Tests: `keyed_wide_roll_matches_the_five_link_chain` (extended) and `rsort_roll_table_matches_per_cell_rolls`.
- e66eb19 — **z-run heat.** When every `(x, y)` row of the domain is one contiguous z range (true for the ball; checked at build, with a density guard), `Topology::zruns` lists the rows. The heat pass walks them and finds face neighbours by arithmetic on the four rows beside each row plus `i ± 1`, in `NB6` order, with no slot-box probes. Holey or sparse domains keep `nb6`. The heat additions wrap in `i32`, so order cannot change a sum. The test checks every cell of every topology-test domain against `nb6` (mutation-checked).
- 26aab39 — heat divides by the face count through constant divisors (`div_by_live`: the same truncating quotient; V8 otherwise emits a trapping `i64.div_s`). Test: `constant_divisor_split_matches_the_plain_quotient`.
- fe6d0ca — **shell-ordered convection.** Material and heat are gathered into rsort order once per tick. `Topology::shell_up` holds the outward neighbour as an rsort position, so the scan reads memory in sequence. A parcel moves at most once per tick, so every swap sees the gathered values; the swaps are recorded and replayed in scan order, making the identical `set_mat` / heat-swap call sequence. The test checks `rsort[shell_up[k]] == rsort_up[k]`.
- fe536d3 — cap 40 → 42. 7326102 — www.

**3. Render: no exact change this wave.** A render-only profile (r40 ocean, 960×540) shows 30k bits keyed after the enclosed cull: 21.6k (72%) rejected by the hidden-bit test, 8.4k drawn, about 49.6k world pixels at every radius. Span rasterisation is 45% of render, `draw_bit_with` (projection, bounds, hidden test) 30%, and allocations under 0.5%. The three candidates the brief named were each checked:
- (a) **Chunk-level hidden cull:** needs a lower bound on every depth a chunk's bits can write. The per-bit bound (`nearest_possible_depth`) must include each bit's smallest drawable triangle area, because rounding on sliver triangles can produce a depth below the nearest vertex. An edge-on face sends that bound to 0, and a chunk-wide area bound would need a new floating-point proof.
- (b) **Skipping a face whose neighbour across it is on:** the crease-tie case already documented at the enclosed cull (an exact depth tie lets the lower-RGB rule pick its face).
- (c) **Fewer allocations:** nothing material left.
- Natively, the hottest render line is `f64::ceil` as a libm call (x86-64 baseline without SSE4.1); wasm has a native `f64.ceil`, so this does not affect the browser.

**4. Native (`radius_study`, ms; before → after).**

| case | chem ms | render ms | step4 ms | RSS hwm MB |
|---|---|---|---|---|
| ocean r40 | 13.1 → 12.7 | 18.8 → 25.6* | 38.9 → 37.7 | 78 → 79 |
| ocean r44 | 21.1 → 15.7 | 23.4 → 22.8 | 52.0 → 51.0 | 90 → 92 |
| ocean r48 | 22.7 → 20.3 | 29.3 → 28.1 | 64.8 → 65.9 | 104 → 105 |
| ocean r56 | 37.1 → 36.1 | 37.3 → 38.8 | 98.8 → 92.5 | 145 → 147 |
| ocean r64 | 58.0 → 53.2 | 46.1 → 43.7 | 129.8 → 137.2 | 198 → 201 |
| ocean r96 | 192 → 193 | 84.5 → 89.4 | 397 → 353 | 566 → 584 |
| gas r40 | 14.3 → 12.1 | 17.9 → 17.9 | 36.5 → 33.6 | 77 → 81 |
| gas r44 | 19.2 → 16.4 | 22.1 → 26.6* | 44.6 → 44.3 | 90 → 95 |
| gas r48 | 26.1 → 22.5 | 27.6 → 29.0 | 59.6 → 55.2 | 104 → 111 |
| gas r56 | 44.3 → 35.2 | 47.1 → 35.9 | 90.2 → 79.3 | 144 → 155 |
| gas r64 | 69.4 → 53.6 | 43.9 → 43.5 | 129.6 → 112.7 | 196 → 214 |
| gas r96 | 328 → 245 | 86.5 → 88.8 | 429 → 339 | 568 → 639 |

\* Render code is unchanged; single-sample render noise. Gas combine2+convect stage at r96: 193 → 122 ms; heat stage 66.7 → 53.0 ms. Peak RSS rises by up to 71 MB at r96 gas (rsort roll table and `shell_up`, 12 B/cell, plus per-tick gather scratch).

**5. Chrome (headless, Low 960×540; raf / presented fps, step ms).**

| case | before (5a032e9) | after |
|---|---|---|
| ocean r40 | 25.7 / 13.8, 34.8 | 29.4–30.0 / 15.0, 26.8–28.1 |
| **ocean r42** | 13.7 / 13.7, 68.7 | **19.6–21.5 / 15.0–15.2, 44–63** |
| ocean r44 | 8.4, 114 | 10.8, 86.4 |
| ocean r48 | 5.6, 168 | 6.2, 152 |
| ocean r56 | 3.8, 248 | 4.1, 231 |
| ocean r64 | 2.57, 365 | 2.67, 350 |
| ocean r96 | 0.60, 1600 | 0.60, 1576 |
| gas r40 | 41.8 / 15.0, 15.8 | 44.7 / 15.5, 13.1 |
| gas r44 | 29.1 / 15.2, 28.6 | 37.5 / 15.0, 18.3 |
| **gas r48** | 11.7 / 11.7, 78.5 | **24.5 / 15.1, 36.0** |
| gas r52 | 6.9, 134 | 10.5, 88.1 |
| gas r56 | 5.3, 174 | 6.4, 142 |
| gas r64 | 3.23, 284 | 4.20, 215 |
| gas r96 | 0.63, 1461 | 0.80, 1189 |

Every run ends `books=ok energy_drift=0 error=none`.

**6. Cap: `CHEM_RADIUS_MAX` 40 → 42 (fe536d3).** Ocean is still the binding preset: r42 holds 15.0–15.2 presented fps over five runs (sweep, three repeats and the shipped :8765 build), at 60.7–62.8 ms per presented frame, the margin r40 shipped with in §6b. r44 drops to 10.8. Gas now holds 15 through r48 (was r44). The slider (`max="42"`), `playNum(…, 2, 42)` and the Rust clamp match, and an r48 request on :8765 clamps to 42.

**7. Exactness evidence.** p6 goldens (`--ignored`) pass (3/3). Parity vs 5a032e9 (identical to the 6475973 reference) is byte-identical on chem (242 lines), play (180) and big (80), re-run after every code commit. wasm parity (7 cases: world, frame and books) is SAME. The default play hashes on :8765 are unchanged: world `lqpba5lv…`, frame `5l3xbpo3…`.

**8. Next wall.**
- (a) Ocean in the browser: the software SHA-256 `world_tag` (about 28% of ocean wasm at r44) plus render (about 23%). r44 ocean needs about 25% off its 86 ms step. The tag is Owen's protocol call (an incremental or Merkle tag).
- (b) Render: exact gains need a proof of a chunk-level depth bound that handles sliver triangles, or a cheaper exact rasteriser for one-to-two-pixel triangles (about 50k per frame at r40). Below 1 px only LOD remains, which would be non-exact and opt-in.
- (c) Gas past r48: combine2's network law and the relief passes now lead the gas chem profile, together with heat's per-cell work.
- (d) Memory: +12 B/cell for the convection tables; genesis is still capped at r103.

**Build.** wasm (fe536d3, ABI 17) md5 `0d59b9c6ee910ade3b532cbfc7f33ce5` (1,408,310 B), play.html `ab854c889305bcb03ac0840245b2a250`, bitblock-lab.html `42957f733018105e5a3103a6d8cbddbc`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged); www commit 7326102. Served at 127.0.0.1:8765 and self-playtested: default hashes unchanged; ocean r12, gas r20, ocean r40/r42 and gas r42 all `books=ok energy_drift=0 error=none`; the lab reports ABI 17; only the favicon returns 404. Screenshots: `/workspace/render_scale/s6c/ship/{ship_2_r42,ship_4_r42,ship_2_r40,smoke_gas_r20_8765_7326102}.png` and the sweep shots `/workspace/render_scale/s6c/chrome/{ref,cur,rep}_*.png`. Soft DDA, bcc-voxel-demo and abi16 are untouched. No Desktop copy; main untouched.

## 6d. Exact chem speedups for gas past r48 and ocean outside hashing/render; play cap 42 → 44 (2026-11-17, branch `chem/render-scale-2026-11`; Owen "continue" pt248u) — every hash and book unchanged

The brief covered gas past r48 (network law, relief passes, heat), trimming the convection tables, and exact ocean chem/edit wins outside hashing and render. It asked for r42–r96 to be re-measured, with a cap raise only if both presets hold about 15 presented fps (ocean is the limiter). world_tag and frame semantics are unchanged. Before = f71454f (§6c). After = 82ff68d (code), 661ffea (cap).

**1. Changes (all exact).**
- b2c16ea — **network law.** Intents go into a per-round list (pass B/C walk it, claims are reset by slot), and the per-cell scratch is sized only when some cell builds its cube. The round-0 skip state (`nnbs`, `vis_id`, `cursor`) is filled only when that round posted an intent. Both play presets have no hot reactive cell so a step touches no per-cell network scratch. That accounts for most of the RSS drop.
- 136bbed — **heat gathers.** The z-run path computes every cell's share `per` first, then adds the face neighbours' shares over contiguous row slices (`i ± 1` and the four beside rows). The face relation is symmetric and the sums wrap in i32. The heat field and share buffers are reused across steps (no per-tick 4 B/cell allocation). Test: `gathered_heat_matches_general_scatter` (row path vs the general scatter over extreme `dq` and random heat, four steps).
- 2ce3197 — **convection scratch.** The gathers and the swap list are reused across steps. A moved parcel is overwritten with a massless id, so the scan skips it exactly as the moved flag did (with a flag array kept as fallback when no id is massless).
- 951b215 — **topology trim.** `rsort_up[k] == rsort[shell_up[k]]`, so the cell-index copy is dropped (−4 B/cell of §6c's +12). The rsort roll table (8 B/cell) stays: rolling from the coordinate instead is a random 24 B read per candidate.
- b3151e8 — **gathered outward gas rise.** The Gravity::Core rise scans rsort-ordered materials with `shell_up`, trades in the gathered array, and replays its trades in scan order. It moves no heat, so in Driven mode it hands the gathered materials and heat to that tick's convection (one gather per tick instead of two).
- 962636c — **face lookups.** `Topology::face(i, k)` is `nb6(i)[k]`. Classic `stepFlow` takes down and sides from one `nb6`, and the water-cycle buried test and `stepSettle` use `face`. The topology test checks `face` against `offset` on every domain.
- ba61feb — **block material search.** Single-material index scans (hand reactions, net burn, condense, GYP/SED) ask `find_mat` for the next matching cell at or beyond the cursor. The block test is vectorisable. Each call happens after the previous visit's writes and nothing writes between a call and its visit, so the visits are exactly the cell-by-cell ones. Test: `block_material_search_matches_current_slice` (including writes ahead of and behind the cursor).
- 0d3a46d — **relief gates.** Uplift and plume need an in-domain AIR or WATER slot and only permute materials. Collapse needs an in-domain AIR. With none in the domain (the gas preset), the pass cannot fire, and its rolls are cell-keyed with no mutable RNG state.
- 82ff68d — the heat test. 661ffea — cap 42 → 44. 7d47569 — www.

**2. Native (`radius_study`, before → after, same session).**

| case | chem ms | step4 ms | RSS hwm MB |
|---|---|---|---|
| ocean r42 | 13.4 → 8.8 | 40.0 → 36.3 | 84 → 63 |
| ocean r44 | 15.1 → 10.7 | 45.5 → 40.4 | 93 → 69 |
| ocean r48 | 19.5 → 15.3 | 59.1 → 51.8 | 105 → 76 |
| ocean r56 | 30.7 → 20.0 | 83.7 → 70.9 | 147 → 99 |
| ocean r64 | 48.8 → 30.4 | 118.7 → 95.7 | 202 → 128 |
| ocean r96 | 178 → 115 | 337 → 282 | 584 → 415 |
| gas r42 | 14.0 → 8.4 | 37.0 → 31.3 | 86 → 67 |
| gas r44 | 16.2 → 15.1* | 44.3 → 36.6 | 95 → 74 |
| gas r48 | 22.9 → 12.8 | 54.6 → 44.2 | 111 → 82 |
| gas r56 | 36.0 → 20.9 | 80.0 → 63.2 | 155 → 109 |
| gas r64 | 53.5 → 31.1 | 105.9 → 81.8 | 214 → 145 |
| gas r96 | 213 → 128 | 320 → 241 | 639 → 415 |

\* Noisy sample (step4 and the neighbouring radii agree with the trend). Gas r96 fine stages after: heat 52 → 28, network 46 → 7, uplift + plume + collapse 26 → 0.6, convect + rise about 100 → 75 ms.

**3. Chrome (headless, Low 960×540; raf / presented fps, step ms; same session).**

| case | before (f71454f) | after |
|---|---|---|
| ocean r42 | 21.3 / 15.1, 43.4 | 29.5 / 15.0, 26.8 |
| **ocean r44** | 10.9, 88.4 | **29.4 / 15.0 (15.0 on 4 of 4 runs), 28.5–29.5** |
| ocean r46 | — | 13.8–14.3, 65–67 |
| ocean r48 | 6.2, 151 | 9.6, 98.0 |
| ocean r56 | 4.2, 224 | 5.2, 178 |
| ocean r64 | 2.83, 331 | 3.37, 278 |
| ocean r96 | 0.60, 1501 | 0.73, 1293 |
| gas r42 | 43.1 / 15.0, 14.7 | 52.1 / 15.1, 9.0 |
| **gas r44** | 30.0 / 14.9, 26.0 | **45.1 / 15.0, 11.7** |
| gas r48 | 29.4 / 15.0, 28.9 | 36.9 / 15.0, 17.0 |
| gas r52 | 9.9, 92.8 | 34.4 / 15.0, 21.6 |
| gas r56 | 6.6, 138 | 15.7 / 15.1, 56.0 |
| gas r64 | 4.37, 209 | 6.93, 123 |
| gas r96 | 0.80, 1156 | 1.03, 891 |

Every run ends `books=ok energy_drift=0 error=none`.

**4. Cap: `CHEM_RADIUS_MAX` 42 → 44 (661ffea).** Ocean r44 holds 15.0 presented fps on four of four runs (57–58 ms per presented frame), and on the shipped :8765 build too (15.07). Ocean r46 drops to 13.8–14.3. Gas now holds 15 through r56 (was r48). The slider (`max="44"`), `playNum(…, 2, 44)` and the Rust clamp match, and an r48 request on :8765 clamps to 44.

**5. Exactness evidence.** p6 goldens (`--ignored`) pass (3/3). Parity vs f71454f is byte-identical on chem (242 lines), play (180) and big (80), re-run after every code commit and at the tip. wasm parity is SAME on 7 cases (ship) plus ocean/gas r56/r64 (study builds). The default play hashes on :8765 are unchanged: world `lqpba5lv…`, frame `5l3xbpo3…`.

**6. Next wall.**
- (a) Ocean in the browser past r44: chem is now a small part of the ocean frame. The software SHA-256 `world_tag` and render dominate. r46 needs about 10% off a 65 ms step. Both are outside exact chem: the tag is Owen's protocol call, and render has no exact gain left at the 1 px floor (§6c).
- (b) Gas past r56: convection plus the rise (one random rsort gather per tick) and heat lead. The remaining exact levers are small (layout of the gather, branch-heavy density test).
- (c) Memory: the rsort roll table (8 B/cell) remains. Peak RSS is 25–35% below §6c anyway.
- Exact chem gains are close to exhausted for ocean: what is left in ocean chem is a few ms at r44.

**Build.** wasm (661ffea, ABI 17) md5 `d145f12e7f606ac8b97f4d95fa535b34` (1,416,268 B), play.html `3f485d24a255fc858779c48b3c4a9296`, bitblock-lab.html `96fc0f2d897ee3cf28482975e2c50962`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged); www commit 7d47569. Served at 127.0.0.1:8765 and self-playtested: default hashes unchanged; ocean and gas r44 `books=ok energy_drift=0 error=none`; ABI 17; only the favicon returns 404. Screenshots: `/workspace/render_scale/s6d/ship/{ship_2_r44,ship_4_r44,ship_2_r48}.png` and the sweep shots `/workspace/render_scale/s6d/chrome/{ref,new,rep}_*.png`. Soft DDA, bcc-voxel-demo and abi16 are untouched. No Desktop copy; main untouched.

## 6e. Incremental world fingerprint (v2 `world_tag`); play cap 44 → 54 (2026-11-17, branch `chem/render-scale-2026-11`; Owen: "Switch the world fingerprint to incremental, hashes can change once")

Owen allowed world hash values to change once. Everything else had to stay exact: game frame pixels, books, energy drift, chem state, and native/wasm parity. Before = 712d5a3 (§6d). After = 64bf815 (fingerprint and rebaseline), f571790 (index), 19291c6 (cap), 8a7d03b (www).

**1. Construction (`bitblock/world/chunk-merkle-sha256/v2`).** This is a two-level SHA-256 Merkle commitment over the occupied set, keyed by the world's own 16³ chunks:
- **Leaf.** `hash_canonical(World, "bitblock/world/chunk-bits-i64-be/v2", 536 B)` over the chunk coordinate (3 × i64 BE) and its 64 occupancy words (u64 BE, bit index `x + 16(y + 16z)`).
- **Root.** `hash_canonical(World, "bitblock/world/chunk-merkle-sha256/v2")` over the u64 BE count of non-empty chunks, then each non-empty chunk in ascending `(cx, cy, cz)` order as its coordinate (24 B) and leaf digest (32 B).
- **Canonical.** The tag is a pure function of the occupied coordinate set. Only non-empty chunks appear, coordinates are unique and sorted, both lengths are fixed per entry, and leaf and root use distinct schemas under the content-tag domain/length framing.
- **Caching.** Each chunk caches its leaf in a `OnceLock<[u8; 32]>` and the world caches its root in a `OnceLock<ContentTag>`. `Chunk::set_on/set_off` drop the leaf and `World::set_on/set_off` drop the root, but only when a bit actually changes. These are the only bit writers, so a cached value is always the value a fresh computation would give. Clones carry the caches, which is valid because the bits are copied with them.
- **Cost.** An unchanged world costs O(1). After edits it costs (changed chunks × one 536 B leaf) plus (one sort and one 56 B entry per allocated chunk). It never scans occupied coordinates; v1 hashed 24 B per occupied bit.
- **Strength.** Collision resistance reduces to SHA-256's, about 2^128 generic, through the injective framing. This is a Merkle tree, not an additive or XOR multiset hash, so there is no generalized-birthday weakness. It is the same assumption the content tags already make. It is not a signature or an authenticated-storage scheme, and historical v1 receipts are not re-verifiable under v2.
- **Determinism.** It uses only integer encodings and SHA-256, with no float, platform hasher order or pointer input. Rust ubuntu and windows CI, wasm (node and Chrome) and the two stdlib Python reimplementations (`tools/semantic_batch_vectors.py`, `tools/headless_replay_policy_vectors.py`) all agree.

**2. Tests.**
- `incremental_world_tag_matches_full_recompute_after_random_edits` runs 2,000 random on/off edits over 65³ around the origin, including no-ops, bedrock, `i64` extreme coordinates and deferred octree upkeep. After every edit it checks the cached tag against `world_tag_full_recompute`, which rebuilds the chunks from `iter_on()` with no cache, and checks a clone after a further edit. A separate case checks that emptying a chunk restores the original root.
- The existing insertion-order and dense random-world oracle tests now compare against the full recompute.
- Genesis (`History::new` over a fresh world), ocean upkeep and chem mirrors, and edits all go through the same `World::set_*`. The parity, play and big harnesses below exercise all three.

**3. Rebaseline (once).** Every pinned value derived from a world tag moved: world, genesis/receipt/event heads, evidence, the headless-replay genesis policy, frame-input tags that bind a world, and composite frames whose sidebar prints `WORLD ROOT` / `COORD HEAD` as text.
- Game frames are unchanged: every `game=` / gallery / demo frame tag and every render KAT kept its value. The 960×540 sidebar parity check hashes game pixels and sidebar pixels separately on 80 frames. Game pixels are identical 80/80. The sidebar differs 80/80, only through the printed tags.
- The default play frame `5l3xbpo3…` → `nx4b62u5…` (and symbol `araktfzr…` → `djgq2svf…`) for that reason, and only that reason.
- p6 goldens pin chem fingerprints, not world tags, so they pass unchanged (3/3, no rebaseline needed).
- The optimization v3 "rejected v2" negative control now pins its own frozen v1-world fields, so its historical tags (`yclrpejh…`, `wxmct7gi…`) stay put.
- `docs/cost-1-measurement-v1.json` is a dated v1 measurement and is left as recorded.

| old (v1) | new (v2) |
|---|---|
| `#bb.policy.sha256.eibnyx3zeig5myau5wyq5y4qsmp2pp6chclpeapjaiz5xojz7zcq` | `#bb.policy.sha256.sgtpi7mozzgscwu7naqghepq37pko4lrz4ka63kdhw5qmty3fnrq` |
| `#bb.receipt.sha256.6feyyp2ttluxp572qxdyfdzmwkgp4i3ad72usqvm32p3lc6ozo5a` | `#bb.receipt.sha256.os4co4gqd6jed4ttwmbkjozviqpb6ly2xtczuuizobzcc7fcsplq` |
| `#bb.receipt.sha256.46s52obcil7tsnn2cqngubtvosl4cy7ytkjpzls524y2ebefd4eq` | `#bb.receipt.sha256.ujbrxwlq6ovljlrmaqd2bpohvh2ezw2ypeytz6byh245erwgs3nq` |
| `#bb.world.sha256.lqpba5lvzmm5bnj2jfoa7dgtdnm2adv5eborbtvaquic36mtno2q` | `#bb.world.sha256.pmbj7za55emc3mjtjvom5domewgjf2rr7qezwpxaxyoohzs5e6gq` |
| `#bb.frame.sha256.5l3xbpo3zj2x65tqjinaliydy7qchd2kw6gsy3yi4gm6awvjrraa` | `#bb.frame.sha256.nx4b62u5ozefqrb5m5snhkhcvdtcwsljdgsvzhez5fppgv27unaa` |
| `#bb.world.sha256.x4emw7263xn5ajefvyissdm6tlv3lrd4miqr47rxsl7fj77h54eq` | `#bb.world.sha256.y6n5uuhnnaj25bfdepwlxziifs7einl4bexr5fcqskyvzenf3xba` |
| `#bb.frame.sha256.araktfzrajbhvmmwo7zgywisgcrbmaewtdxhdl3ebrbaw5syncka` | `#bb.frame.sha256.djgq2svfynmviug6ngvgsxcga7nhpjlj5qxkeu3wf5isqz5hg4yq` |
| `#bb.world.sha256.bnf5w54xbhb7qlshwchoac67vkzn5mt4vzrhvnoakys72slg43ua` | `#bb.world.sha256.xm3rmpec5pzo5h4dfxu2oa35lest5fizzdigrj3xlfbg3caarflq` |
| `#bb.receipt.sha256.hdnsryd4zq6gr5dkrtw2zjqvipqrkmjowtkzonmutapxiwln5xwq` | `#bb.receipt.sha256.5zbvdvpm3jczrti5nclnahhl5obwmg5zlbq3hsffz3xgddopmeoa` |
| `#bb.world.sha256.xplotpuf27scr7i25aabwijert6g6444ntkh2qqvtjpu3qme7cka` | `#bb.world.sha256.65vcqxuxnd4gzf3msg37cr5tuj6u2ytkugbjmnle3ywta5wssqeq` |
| `#bb.receipt.sha256.ezntrdbxgwqvwwond4rm7oarrzqkktoowuky6uicisbk5l7tybia` | `#bb.receipt.sha256.efbx3lzitz3hfpjp23urtnsqqqwmnfkjzsx7dtfisqcnh2n5mklq` |
| `#bb.world.sha256.6a2uby2p3md7hhmmepbl67rnefxvteb3znxsxnvozxoy4lbztqxq` | `#bb.world.sha256.4qebqribdfmkf53kxbzjwfe3vrfhmyddlv2emjmbhexvwtxpt2gq` |
| `#bb.world.sha256.ziy2sdalw4i2k5uszp32ldvf57moe44kv2q2fcs6dkxwavoertca` | `#bb.world.sha256.lpqtwwq5lo3dbyid2hrdvmdr3r6wfy5t5xnhd3grwcis5bhu4nra` |
| `#bb.receipt.sha256.qyjgyjutxxfupm6rfp33zm6wrr2i5g5z4z7vhyqqweob6krqf7ra` | `#bb.receipt.sha256.5cobj7m7rn4g3oysvktolank4g3io3iucunk5743plnrbxak6vua` |
| `#bb.receipt.sha256.vbevj4vyjwjk3ume6vkxkigm7vioe6ox65c4ws6r6ly5jqwg73ga` | `#bb.receipt.sha256.62we6nvy5lmpwoxfyl2fgiw3nyrfvpa6u5obnptd7er5nksizqtq` |
| `#bb.world.sha256.w6wlo572rc3u5v4mvsopcl7gxvkfxl6ntni7rk6qgr2ja5bdblzq` | `#bb.world.sha256.iy5q2bopf2llw4samvn5ifsbup3adpxu2lupt2vjp4qqqgx4s6iq` |
| `#bb.frame.sha256.fscl5apfua6xbkp7776f2rqtka2nknilpxcfyoznzjcxhw675woq` | `#bb.frame.sha256.eqvpp636o2wtilipgbginnmnpa3zyohkoyh7eab65c27pk7a5zoq` |
| `#bb.receipt.sha256.psal6njm4ndyap7wclfv55gnlxwwou4eq2rvfotazrxzragceusa` | `#bb.receipt.sha256.d7qzlret5ep47ewjymdfylx3lelasfvp5s6tmt72qjjh34bqwmea` |
| `#bb.receipt.sha256.fakwjpoid3eqaztgfahmlepmfawgi7yffed345eovjysn2izuf5a` | `#bb.receipt.sha256.iozg4ylucv37oyjoceamtnvaqwdavpfk366dlo44kividto2q32a` |
| `#bb.receipt.sha256.v4rixvqu53nkaraowd4z26s2r55fv4rnz6qnq2dbtjaccxiljffq` | `#bb.receipt.sha256.6bkjjhs2qprx3dfgkuamon5dei5v6gqcnqk6k7ugsjxnx4nc4l4a` |
| `#bb.receipt.sha256.kdqtkvuuljhq4xjmkft44bk46rd4rqnty2h6sbozjwg2o4nnjgmq` | `#bb.receipt.sha256.rmfiy36x6e4kisgi5n6qemrpa3o6lpnbtrfqmmkohzlrixezmbaq` |
| `#bb.receipt.sha256.li2iopqyi652bk4hdt75ad356z5iqryugxmxkefhfroxvhtpetja` | `#bb.receipt.sha256.rong4dap7mwzs2zztqioqo56ix2rmnjheutosombipvzgnt7dgiq` |
| `#bb.receipt.sha256.amsihpcd2ydi5243jdzuoa2g3a5tbh726k5zru6dfmyargeul6ja` | `#bb.receipt.sha256.exlpidihytljgbcr2vbgazsmifshk52eveiet5bfa32fzv6hfq6a` |
| `#bb.receipt.sha256.dzihycw2ucnfjaulmme6pqlgv3n4u65gn4bqi36xvzkmxymzn22q` | `#bb.receipt.sha256.suix4q6f7pj7qakempurczmewpjzq6hruzblpcueyezmiqwephha` |
| `#bb.event.sha256.znebuddob7z4u2vwyqe7bwhclvyeqtgya7252fm5qq7hewauh2kq` | `#bb.event.sha256.7juld2z7wmvaol3xbf2kdlniwzy4tcbr7gfy5yojwaofqkmgws3a` |
| `#bb.event.sha256.6kcbq6sgg7zr5nymes5da5ksm4kt2y6fpytmfwlbs3xmn4zdsx5q` | `#bb.event.sha256.qk4stjl5xesq72glgtz7thj4lk4s4zsbb6fp7oljjsp45xfylmtq` |
| `#bb.event.sha256.h7rmaq3zvlig2btqavazt37nhrk7el2pwulowcvhzmzbm245i6ua` | `#bb.event.sha256.qhwms7spp3gkyjy5534zvkknwlwzbcwjoizszruhokslatavip5q` |
| `#bb.event.sha256.zr3ehfiz5xxheosxvxydb27m4vpfibfbpjkg7cicuob6tsf46cja` | `#bb.event.sha256.c2aidqhefgmovroufymmonnh2yzk77dgg7z55xiw4cag54euht2q` |
| `#bb.event.sha256.gibrybl5w6zs3bq73t3e3qg6qjrhifcf2ce6xhnxyttsnukxvwma` | `#bb.event.sha256.j7h56ji37pgsj3s672a532cdm3eb3vu6zstx6pqehpwx36wbobwa` |
| `#bb.receipt.sha256.mragjur2xbgilfa66fdcrn33bklxh36s7xpem5lik466yfk3jgca` | `#bb.receipt.sha256.hezcoouaqcsgagnn2nsnrxgopeizt3tuzpgvddlhexupbo2jz4ka` |
| `#bb.world.sha256.5ar4um5zfetdzaeb34e4c4m6a5oevi5u3fo4id36pm3oye6atvpq` | `#bb.world.sha256.7wmhw54fbbgtuuf3hrgx6hdfqzeppt4excfkbh5yjef6iuavw3ga` |
| `#bb.world.sha256.sfnrtxhwwyni22q2b4xbw34gxhkhxtkbdofsrkfiaoq7qonlohda` | `#bb.world.sha256.7hwue53b53xdgouajkonocanl2j76nuecnavh5lhxyneykow2a5a` |
| `#bb.receipt.sha256.jj6zjbfciew3ep5o5erslaedi64s7ly2nyq6ksmal5wwfojnscta` | `#bb.receipt.sha256.7irgt5dkmlj3hasbl7s47arg6mf5g3yz6gq2chdhkfnnvbxto7aa` |
| `#bb.receipt.sha256.kbcaytlz5pwi7nz4dk6esnpgsdfdmow4azrya7rxdtyn4yg3qkcq` | `#bb.receipt.sha256.gdquf5jw6wpi3qzim3cp2b3qmq2m6qloxy7qcbgthpvcrrlzqwpq` |

(The default play world is `lqpba5lv…` → `pmbj7za5…`. The lab page known answers, the Python vectors, `tools/bbselftest.json` counts (+1 schema, protocol +2 dependents) and PROTOCOL.md were updated with the same mapping.)

**4. Exactness evidence.**
- Parity vs 712d5a3 with only world tags masked: chem (242 lines) and play (180) are byte-identical. On big (80), only the sidebar-bearing composite frames differ, with game pixels identical as above.
- Native == wasm on 7 cases (oc/gas r12–r44, ash r24: world, frame, books, drift).
- `books=ok energy_drift=0` everywhere.
- Every world tag differs from v1 in every line, as expected.

**5. world_tag time (native release, ms; cold = fresh world, after step = first tag after a chem step, repeat = unchanged).**

| case | v1 per tag | v2 cold | v2 after step | v2 repeat |
|---|---|---|---|---|
| ocean r44 | 5.28 | 0.13 | 0.053 | 0.000 |
| ocean r48 | 6.80 | 0.13 | 0.059 | 0.000 |
| ocean r56 | 10.86 | 0.18 | 0.098 | 0.000 |
| ocean r64 | 16.10 | 0.25 | 0.131 | 0.000 |
| ocean r96 | 54.4 | 0.75 | 0.346 | 0.000 |
| gas r44 / r96 | 5.30 / 53.9 | 0.10 / 0.73 | 0 (no bit change) | 0 |

Ocean changes occupancy on every chem tick (12/12) and gas on none (0/12). Native has SHA-NI. In wasm (software SHA-256) the v1 tag dominated the ocean step, as the Chrome table shows.

**6. Chrome (headless, Low 960×540, chem every 4 ticks; raf / presented fps, step ms; same session, v1 study build vs v2 study build).**

| case | before (712d5a3) | after |
|---|---|---|
| ocean r42 | 20.2 / 15.3, 48.7 | 45.1 / 15.1, 11.9 |
| ocean r44 | 12.0 / 11.4, 81.7 | 44.4 / 15.0, 12.5 |
| ocean r46 | 12.1, 77.0 | 33.3 / 15.7, 21.4 |
| ocean r48 | 8.4, 110.5 | 36.1–36.8 / 14.97 (2 runs), 17.5–19.1 |
| ocean r52 | 5.5, 165 | 26–36 / 15.0–15.2 (4 runs), 20–30 |
| **ocean r54** | — | **19–27 / 15.0–15.07 (4 runs), 30–44** |
| ocean r56 | 4.7, 198 | 12.0, 73.5 |
| ocean r64 | 3.40, 269 | 7.37, 112 |
| ocean r96 | 0.70, 1345 | 1.20, 770 |
| gas r42 | 48.8 / 15.0, 10.0 | 42.4 / 15.2, 13.7 |
| gas r44 | 43.2 / 14.8, 13.1 | 42.5 / 15.0, 13.5 |
| gas r48 | 35.3 / 14.3, 19.8 | 35.6–37.4 / 14.97–15.27 (2 runs), 16.7–19.2 |
| gas r52 | 24.7 / 15.2, 34.8 | 24–31 / 14.5–15.1 (4 runs), 25–38 |
| **gas r54** | — | **20–23 / 14.93–15.17 (4 runs), 36–43** |
| gas r56 | 12.2, 72.1 | 14.3, 61.2 |
| gas r64 | 5.73, 151 | 6.00, 141 |
| gas r96 | 1.00, 930 | 1.00, 887 |

Every run ends `books=ok energy_drift=0 error=none`. Ocean r44 step falls from 81.7 to 12.5 ms. Gas never changes occupancy, so v1 hashed nothing on its ticks either, and it is unchanged within noise.

**7. Cap: `CHEM_RADIUS_MAX` 44 → 54 (19291c6).** At r54 both presets hold about 15 presented fps on 3 study runs plus 1 shipped run each: ocean 15.00–15.07 and gas 14.93–15.17, at 52–57 ms per presented frame. r56 falls to 12.0 (ocean) and 14.3 (gas). The slider (`max="54"`), `playNum(…, 2, 54)` and the Rust clamp match, and an r60 request clamps to 54 on :8765.

**8. Next wall.** At r54 the ocean step is chem (convect/heat/combine) plus render. Past r56 both presets are bounded by the chem passes §6d named (convect gather, heat, combine2), not by hashing. The world tag is now about 0.1 ms native even at r96.

**Build.** wasm (8a7d03b, ABI 17) md5 `75007f8ee124e0ec04b94e063ba680ea` (1,424,935 B), play.html `c5524cd8ef3c60d613b091179b6ff57a`, bitblock-lab.html `ccb2e9a74e3f41a38ece6f6a0042a68e`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged). Served at 127.0.0.1:8765 and self-playtested:
- Default play `world=pmbj7za5… frame=nx4b62u5…`.
- Ocean and gas `books=ok energy_drift=0 error=none`, ABI 17. The lab page's load-time match flags are identical to the 712d5a3 page, and its only errors are favicon 404s.
- Screenshots: `/workspace/render_scale/s6e/ship/{ship_2_r54,ship_4_r54,ship_2_r60,smoke_gas_r12_8765}.png` and the sweep shots `/workspace/render_scale/s6e/chrome/{ref,new,rep}_*.png`.

Soft DDA, bcc-voxel-demo and abi16 are untouched. No Desktop copy; main untouched.

## 6f. Exact delta render, cheaper mirror/books, heat and convection; play cap 54 → 70 (2026-11-17, branch `chem/render-scale-2026-11`; Owen "yes lets keep scalling" pt251u) — every hash and book unchanged

The brief targeted convection gather, heat, combine2 and render with packed integer arrays, batching and culling. Everything had to stay exact vs ac4a049 (§6e): world v2 and frame hashes, p6 goldens, chem/play/big parity, native == wasm, `books=ok energy_drift=0`. The cap could only rise where ocean and gas both hold about 15 presented fps on four Chrome runs (Low 960×540, chem every 4 ticks). No hash changed, and no option needing a hash change came up.

**1. Changes (all exact).**
- 3366809 — **delta render for a static camera.** A frame is the per-pixel minimum by (depth, then RGB word) over the fragments of every on bit. So when only some bits change, the frame can be rebuilt by resetting just the pixels those bits owned and redrawing the bits whose screen boxes touch them.
  - The renderer keeps an owner key per pixel and a box index (`DeltaEntry`, grown 1 px, CSR per 4 px tile) recorded during the full traversal.
  - A delta frame does the following:
    - resets the pixels of the "loud" changed bits;
    - redraws the indexed bits that overlap a reset pixel;
    - draws the open changed bits everywhere (exact ties);
    - draws and records the fresh bits (turned on, plus the open on-neighbours of bits turned off).
  - Quiet bits are skipped: off and not near a toggle, or enclosed where the cull applies.
  - It falls back to a full render when:
    - the projection is not bit-identical;
    - the base is invalid (resize, all-dirty, camera inside a bit, reference traversal);
    - any vertex is behind the camera;
    - more than half the bits are loud;
    - the index grows past 1.5× the base.
  - `world_pixels` stays exact because no chem colour is BACKGROUND (`ChemColors::never_black`).
  - Play side:
    - Play uses it only while every change since the last render is a chem mirror (`delta_chain`, a tag check before and after).
    - Edits, full mirror scans, bench stages and the cut view fall back to the full render.
  - Tests:
    - drill-hole and floating-plate cases against a full render;
    - `delta_render_frames_equal_full_render_frames` (≥ 12 delta frames, frame-equal to full renders).
- 9641cd6 — **mirror and books.**
  - Mirror: the chem→play mirror keeps its FNV walk over `mats()` but reads coordinates and occupancy only for cells whose material changed. Lookups go through a cached-chunk `BitProbe` (chem world) and a copy of the last chunk's words (play world).
  - Books: `lane_totals`, `energy_total` and `material_count` sum the same i64 terms grouped by a 4-way material histogram. `check_books` uses the probe.
- 42451bc — **heat, combine2, convection.**
  - Heat:
    - `ZRuns.live` precomputes each cell's face-neighbour count.
    - The first sweep runs over whole-domain slices in i32 with `q / (6000/dq)` when `dq` divides 6000 (the same truncated rational; `0 ≤ per·live ≤ o ≤ q`). The i64 path is kept otherwise.
  - combine2: the N + 3 H scan is skipped when the domain holds no H. Every firing needs three H and the scan makes none.
  - Convection now runs in three phases, with the same swaps:
    1. A branchless density test over every gathered pair. No parcel has moved yet, and a parcel keeps its gathered values until it moves.
    2. The Driven rolls over the candidates, read in sequence from the rsort roll table. The roll is a pure function of cell and tick.
    3. The original-order scan over the survivors, skipping moved parcels as before.
- 7ccc99b — cap 54 → 70 and www refresh. 20238e3 / 46202ac — index regen (pub counts 1326 / 1329).

**2. Native per pass (r60, chem every 4 ticks, ms; ac4a049 → 42451bc).**

| item | before | after |
|---|---|---|
| ocean play step(4) frame | 59 | 37–39 (delta render + mirror) |
| gas play step(4) frame | 60 | 31–33 |
| heat pass | 6.4 | 2.6 |
| gas convect (scan + replay) | 10.4–11.2 | 5.2 |
| ocean combine2 | ~5 | ~3.5 (N + 3 H scan 1.5 → 0) |
| ocean chem step | 24.6 | 20.2 |
| gas chem step | 21.5 | 16.4 |

**3. Chrome (headless, Low 960×540; presented fps, step ms, main-thread busy).** Presented fps tops out at 15, the chem cadence (one chem step per 4 ticks at 60 Hz). Busy % is the headroom left.

| case | ac4a049 | 42451bc |
|---|---|---|
| ocean r60 | 11.3, 76 ms, 87% | 15.0, 6.9 ms, 38% |
| gas r60 | 9.6, 90 ms, 86% | 15.0, 9.5 ms, 47% |
| ocean r64 | 6.8, 125 ms | 14.5–14.7, 8.5 ms, 44–48% |
| gas r64 | 7.0, 121 ms | 14.8–15.3, 14.8 ms, 52–58% |
| ocean r68 | — | 14.6–15.8 (4 runs), 50–60% |
| gas r68 | — | 15.0 (4 runs), 62–68% |
| **ocean r70** | 5.7, 145 ms | **14.93–15.23 (4 runs), 14–17 ms, 54–61%** |
| **gas r70** | 5.0, 173 ms | **14.97–15.03 (4 runs), 18.5–19.4 ms, 69–71%** |
| ocean r72 | — | 13.3–14.4, 19–33 ms, 64–73% |
| gas r72 | — | 12.7–13.0, 54 ms, 81–82% |
| ocean r76 | — | 14.8, 37 ms |
| gas r76 | — | 5.6, 158 ms |
| ocean r80 | — | 7.9, 107 ms |
| gas r80 | — | 4.7, 194 ms |

Every run ends `books=ok energy_drift=0 error=none`.

**4. Cap: `CHEM_RADIUS_MAX` 54 → 70 (7ccc99b).** r70 holds about 15 presented fps on four of four runs for both presets. r72 drops gas to 12.7–13.0. The slider (`max="70"`, title updated), `playNum(…, 2, 70)` and the Rust clamp all match, and an r80 request on :8765 clamps to 70.

**5. Exactness evidence.**
- Parity vs ac4a049 is byte-identical after every code commit:
  - chem `8ef8f682…`
  - play `dd849e0b…`
  - big `9248e8d7…`
  - split `bc8ac3af…`
- p6 goldens (`--ignored`) pass (3/3).
- wasm (simd128 ship build) == native on ocean r60, gas r60 and gas r48 (`wasmstep` vs `wasmeq`).
- Default play hashes unchanged (world `pmbj7za5…`, frame `nx4b62u5…`, the play test constants).
- The lab's research and program checks report `match_expected=true`.
- No wasm SIMD intrinsics were needed: the batched loops are plain integer code that the compiler vectorises.

**6. Next wall.**
- (a) Gas past r70. The rsort gather of materials and heat (`gather_parcels`, a random read per cell) and `step_laws`' rise/convect scan lead the profile. After them come combine2's network/gas-rise parts and heat. Gas r72 also jumps to 54 ms a step, likely memory or cache size.
- (b) Ocean past r76: `step_laws` (flow, erode, dissolve, water ~2.5 ms each at r60), the mirror's `is_on`, and the books totals (`energy_total`, `lane_totals`).
- (c) Delta render only helps with a still camera. A moving camera pays the full render, the §6e cost.
- Exact ideas still open:
  - keeping the materials and heat in rsort order, which removes the gather but is a large layout change;
  - an activity bitset per chunk for flow/erode;
  - keeping the books totals incrementally in `set_mat`.

**Build.** wasm (7ccc99b, ABI 17) md5 `0c247aa20e66733c488974cefe40f5da` (1,458,319 B), play.html `a27df0e80c908f6c0ae359684e1e2451`, bitblock-lab.html `1700fb34341ef2d5ce69a2709ccc7516`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged). Served at 127.0.0.1:8765 and self-playtested: ocean and gas r70 `books=ok energy_drift=0 error=none` at 15.0 / 15.1 presented fps, and r80 clamps to 70. Only the favicon returns 404.
- Screenshots: `/workspace/render_scale/s6f/playtest/{default,chem2_r70,chem4_r70,chem4_r80}.png`.
- Sweep shots: `/workspace/render_scale/s6f/chrome/{ref,e1,e1rep*,e1r70_*}_*.png`.

Soft DDA, bcc-voxel-demo and abi16 are untouched. There is no Desktop copy, and main is untouched.

## 6g. Status books in one pass, straight convection gathers; play cap 70 → 72 (2026-11-17, branch `chem/render-scale-2026-11`; Owen "keep going, but make sure to update my desktop after every pass" pt252u) — every hash and book unchanged

This wave is one shippable pass, so Owen's Desktop can be updated after it. The exactness bar is the same as §6f, against b55c24e: parity byte-identical, p6 goldens, native == wasm, the default play world `pmbj7za5…` / frame `nx4b62u5…`, and `books=ok energy_drift=0`. No hash changed, and no win needed a hash change.

**1. Changes (all exact).**
- 76de12c — **status books.** The browser loop asks for both status lines at 10 Hz (`STATUS_UI_MS = 100`), so this cost ran ten times a second.
  - Before, the chem readout recomputed the material histogram about seven times, summed the heat three times, and probed every cell's 24-byte coordinate through the chunk map.
  - `ChemWorld::book_totals` now computes material count, lane totals and drift, and energy total and drift from one histogram and one heat sum. These are the same i64 terms as the separate calls. `check_books_with` reuses them.
  - The occupancy check walks the topology's z-runs, with one chunk lookup per 16-cell z block. In that block z + 1 steps one 16×16 layer, so the bit index steps by 256. The cells are the same, in the same order, so the first failure is the same.
  - Without a topology (before the first step, or after cells change) it keeps the cell walk. When a material lacks a table row, it keeps the original walk, so a `BadMaterial` error stays exact.
  - Test `books_occupancy_walk_by_runs_matches_cell_walk`: 37 bits turned off from the last solid cell backwards, crossing chunk borders at negative coordinates. Each run walk must equal the cell walk and name that cell. Changing the layer step to 16 fails it.
- 8252a42 — **convection gathers.** `gather_parcels` (handed from the gas rise to convection) fills the materials and the heat as two straight `extend`s over `rsort`, instead of a push pair per cell. The values are the same.
- 0a5bae2 — cap 70 → 72 and www refresh.

**2. Native (before → after).**

| item | before | after |
|---|---|---|
| chem status (debug readout), ocean / gas r72 | 11.1 / 11.8 ms | 2.5 / 2.6 ms |
| play status line, ocean / gas r72 | 1.8 / 2.1 ms | 0.8 / 0.9 ms |
| both at 10 Hz, gas r72 | ~139 ms per second | ~35 ms per second |
| gas r60 gather (rise → convect) | 2.33 ms | 1.47 ms |
| gas r72 gather | — | 2.94 ms (scan 7.4, replay 3.2, 243k swaps) |

The status cost runs outside the page's `step_ms`/`busy` counters, so it shows up only as raf and presented-fps headroom.

**3. Chrome (headless, Low 960×540, chem every 4 ticks; presented fps, step ms, busy).**

| case | b55c24e (§6f ship) | 0a5bae2 |
|---|---|---|
| ocean r70 | 15.30, 12.2, 52% | 14.87, 12.1, 54% |
| gas r70 | 15.00, 18.9, 69% | 15.07, 21.1, 74% |
| ocean r72 | 15.87 (§6f: 13.3–14.4) | **14.67–15.13 (4 runs), 13.3–19.9** |
| gas r72 | 15.20 (§6f: 12.7–13.0) | **14.40–15.40 (4 runs), 21.4–33.8, 73–85%** |
| ocean r74 | 15.43, 20.5 | 15.13, 15.8 |
| gas r74 | 8.6, 107 | 13.2, 59.6, 90% |

Every run ends `books=ok energy_drift=0 error=none`. The §6f build at r72 swings between about 13 and 15 from run to run; with this pass all four runs on both presets hold 14.4 or more.

**4. Cap: `CHEM_RADIUS_MAX` 70 → 72 (0a5bae2).** r72 holds about 15 presented fps on four of four runs for both presets (lowest gas run 14.40). Gas r74 falls to 13.2. The slider (`max="72"`, title updated), `playNum(…, 2, 72)` and the Rust clamp all match, and an r80 request on :8765 clamps to 72.

**5. Exactness evidence.**
- Parity vs b55c24e is byte-identical after each code commit: chem `8ef8f682…`, play `dd849e0b…` (its lines carry `books`, `energy_drift` and `lanes`), big `9248e8d7…`, split `bc8ac3af…`.
- p6 goldens 3/3; core 323 and wasm 78 tests pass.
- Ship wasm == native world and frame on ocean r60, gas r60, gas r48, ocean r72 and gas r72.
- On :8765 Chrome reads the default world `pmbj7za5…` and frame `nx4b62u5…`, ABI 17. The lab page reports `match_expected=true`.

**6. Next wall.**
- (a) Gas past r72:
  - convection's ordered scan (7.4 ms at r72) and its swap replay (3.2 ms, 243k random set_mat / heat swaps);
  - the rsort gather (2.9 ms);
  - combine2's network (1.4 ms at r60) and heat.
  - Busy is already 73–85% at r72. r74 jumps to 60–107 ms steps once the frame no longer fits in a 15 Hz slot.
- (b) Ocean past r74: flow, erode, dissolve and water (~2.5 ms each at r60), and the mirror's FNV walk and `is_on`.
- (c) Exact ideas still open:
  - keep the materials (and heat) persistently in rsort order through `set_mat`;
  - a per-chunk activity bitset for the ocean relief passes;
  - incremental material hash in the mirror, which needs a careful collision argument, so it is left out.

**Build.** wasm (0a5bae2, ABI 17) md5 `374ad1abb7b8a60cdf17cf2293b9697c` (1,459,637 B), play.html `4fb9ae85faea099f9403cee1d34baec9`, bitblock-lab.html `684beaf1b31de06044ac2615d545a691`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged).
- Served at 127.0.0.1:8765 and self-playtested: ocean and gas r72 `books=ok energy_drift=0 error=none`, r80 clamps to 72, and only the favicon returns 404.
- Screenshots: `/workspace/render_scale/s6g/playtest/{default,chem2_r72,chem4_r72,chem2_r80}.png`.
- Sweep shots: `/workspace/render_scale/s6g/chrome/{base,g1,g1rep*,g2rep*}_*.png`.

Soft DDA, bcc-voxel-demo and abi16 are untouched. There is no Desktop copy (Owen's Desktop is handled outside this branch), and main is untouched.

## 6h. Exact material-search gates for the quiet passes and the network law; play cap 72 → 74 (2026-11-17, branch `chem/render-scale-2026-11`; Owen "keep going, but make sure to update my desktop after every pass" pt252u) — every hash and book unchanged

This wave is one shippable pass. Its first priority was real headroom for gas at r72, where §6g's lowest run was 14.40 at 73–85% busy. The exactness bar is the same as §6g, against 35e428c. No hash changed, and no win needed a hash change.

**1. Changes (all exact).**
- e8cca67 — **quiet-pass gates and lighter writes.**
  - Several passes walked every cell to test one material:
    - phase (WATER / ICE);
    - Driven evaporation and degas (WATER);
    - erode and weather (ROCK / MIN).
  - They now visit those cells through `find_mat` / new `find_mat2`, the block search that §6d proved visits exactly the cell-by-cell set: each call happens after the previous visit's writes.
  - weather builds its per-cell sky mask only when a ROCK/MIN exists.
  - Coreward flow and settle return when no WATER / SED exists. Only those cells move, the passes create none, and so a gas planet no longer allocates their per-cell flags.
  - `set_mat` reads the 24-byte coordinate only when occupancy changes. A material-to-material write, like every convection swap, reads none.
  - The convection candidate buffer only grows: every slot below `len` is written before it is read.
  - Test `block_two_material_search_matches_current_slice`.
- 1bcddba — **network gate.**
  - In round 0 every cell is a fresh start. A cold cell (q < 250), or one whose material reacts in no mounted row, posts nothing and writes no scratch.
  - So with no hot reactive cell the round posts no intent and the pass returns `(0 firings, 1 round)` with no other change. A 64-cell block search answers that first, and any hot reactive cell runs the original pass.
- cad821c — cap 72 → 74 and www refresh.

**2. Native (`chem_fine`, r72).**

| item | 35e428c | 1bcddba |
|---|---|---|
| gas chem step | 26.5 ms | 21.4 ms |
| gas quiet passes (phase, water, flow, erode, weather, degas, settle) | 5.9 ms | ~0.8 ms (erode and weather 0.4 each; the rest under 0.3) |
| gas network | 2.3 ms | 0.8 ms |
| gas convection swap replay | 3.15 ms | 1.92 ms |
| ocean combine2 stage | 5.7–6.3 ms | 4.3 ms |
| ocean chem step | 32.5–34.0 ms | 32.7 ms (ocean has water and rock, so its quiet passes are not quiet) |

**3. Chrome (headless, Low 960×540, chem every 4 ticks; presented fps, step ms, busy).**

| case | §6g ship (0a5bae2) | 1bcddba study build |
|---|---|---|
| ocean r72 | 14.67–15.13, 13–20 ms, 60–70% | 14.57–15.27 (4 runs), 12.8–15.6 ms, 56–64% |
| **gas r72** | 14.40–15.40, 21–34 ms, **73–85%** | **14.97–15.10 (4 runs), 9.1–11.4 ms, 47–53%** |
| **ocean r74** | — | **14.53–15.40 (4 runs), 15.5–16.4 ms, 63–66%** |
| **gas r74** | 13.2, 60 ms, 90% | **15.00–15.87 (4 runs), 11.5–12.2 ms, 53–55%** |
| ocean r76 | — | 14.43–15.57 (4 runs), 15–30 ms, 65–81% |
| gas r76 | — | 14.87–15.03 (4 runs), 12.5–14.8 ms, 57–63% |
| ocean r78 | — | 13.10–15.03 (4 runs), 19–37 ms, 70–83% |
| gas r78 | — | 14.93–15.03 (4 runs), 15–16 ms, 64–66% |
| ocean / gas r80 | — | 14.90 / 15.27, 25 / 22 ms, 77 / 73% |
| ocean / gas r84 | — | 10.97 / 10.07, 86 / 108 ms |

Every run ends `books=ok energy_drift=0 error=none`.

**4. Cap: `CHEM_RADIUS_MAX` 72 → 74 (cad821c).**
- Gas now has wide headroom: at r72 busy fell from about 80% to about 50%, and gas holds 15 through r78.
- Ocean is the limiter again. Its lowest r74 run (14.53) is the same as its lowest r72 run on this build (14.57), at about the same load (63–66% vs 56–64%). So the dips under 15 do not track CPU load, and r74 adds no ocean risk over r72.
- r76 ocean dips to 14.43 at up to 81% busy, and r78 to 13.10.
- The slider (`max="74"`, title updated), `playNum(…, 2, 74)` and the Rust clamp all match, and an r80 request on :8765 clamps to 74.
- This is a judgment call against the "prefer lowest ≥ 14.7" guideline: no ocean radius from r72 up meets 14.7 on every run, either before or after this pass.

**5. Exactness evidence.**
- Parity vs 35e428c is byte-identical after each code commit: chem `8ef8f682…`, play `dd849e0b…`, big `9248e8d7…`, split `bc8ac3af…`. The chem set covers the rocky and ocean presets, so water, ice and rock all go through the gated passes.
- p6 goldens 3/3; core 324 (+1 new) and wasm 78 tests pass.
- Ship wasm == native world and frame on ocean r60, gas r60, gas r48, ocean r74 and gas r74.
- Chrome on :8765 reads the default world `pmbj7za5…` and frame `nx4b62u5…`, ABI 17. The lab page reports `match_expected=true`.

**6. Next wall.**
- (a) Ocean past r74: the chem step (32 ms native at r72) is dominated by heat, flow, erode/dissolve and the water cycle, all with real work on an ocean planet. The mirror walk adds to it. A per-chunk activity bitset is the next exact lever.
- (b) Gas past r78: convection (phase-1 density test 4.5 ms, gather 2.9 ms), heat 4.3 ms, the gas rise. Persistent radius-ordered materials would remove the gather.
- (c) The incremental mirror material hash is still skipped, as Owen asked.

**Build.** wasm (cad821c, ABI 17) md5 `8b806315e5cc5253bf8847d3f458c5f0` (1,463,021 B), play.html `99998a6d39f320f111e0baf56358b4d6`, bitblock-lab.html `25d607f48711ffa5c2ee6533e8b73f39`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged).
- Served at 127.0.0.1:8765 and self-playtested: ocean and gas r74 `books=ok energy_drift=0 error=none`, r80 clamps to 74, and only the favicon returns 404.
- Screenshots: `/workspace/render_scale/s6h/playtest/{default,chem2_r74,chem4_r74,chem4_r80}.png`.
- Sweep shots: `/workspace/render_scale/s6h/chrome/`.

Soft DDA, bcc-voxel-demo and abi16 are untouched. There is no Desktop copy, and main is untouched.

## 6i. Exact AIR-driven activity for the ocean passes; play cap 74 → 76 (2026-11-17, branch `chem/render-scale-2026-11`; Owen "keep going, but make sure to update my desktop after every pass" pt252u) — every hash and book unchanged

This wave is one shippable pass on ocean, the limiter after §6h (Owen accepted r74; ocean's dips do not track load, noise floor ~14.5). The exactness bar is the same as §6h, against 1bcd506: same cells, same order, same roll draws. No hash changed, and no win needed a hash change.

**Why AIR.** An ocean planet at r72 is 1.56 M cells: WATER 431 k, ROCK / MINERAL / GYPSUM 193 k each, and only ~1 k AIR at genesis, growing by ~100 a tick through evaporation (15 k by tick 150, ~1 %). Classic flow moves WATER only into an AIR face neighbour, and dissolve needs an AIR face, so on this planet nearly every visit of those two full scans is a no-op. The per-chunk activity the brief asked for is, for these passes, exactly "cells beside AIR": tracking it per cell is cheaper and finer than per chunk.

**1. Changes (all exact).**
- bf08840 — **AIR-driven sparse scans, row-wise erode, block masks.**
  - `sparse_air()` lists the AIR cells by the 64-cell block search, or `None` past 1/8 of the domain (the full loops then run unchanged).
  - Classic flow (`ysort` order): the visits are the WATER faces of the AIR cells, taken in `ysort` order through a new lazily built inverse `Topology::ysort_pos`, plus, after each move, the faces of the AIR cell the move leaves (the move's target is one of them) pushed on a min-heap when they lie later in the scan. Every skipped visit of the full scan is a no-op (not WATER, or no AIR face when visited); a cell is visited at most once at its own scan position, as before. The visit body is the old one, factored as `flow_cell`.
  - Dissolve: intents read the snapshot, so the visits are just the GYPSUM faces of the AIR cells, sorted ascending and deduplicated; claims and commits are unchanged.
  - Classic erode over z-runs: a row's exposure (a face off the domain, or on AIR/WATER) comes from contiguous slices of the row and its four neighbouring rows. The pass only turns ROCK/MINERAL into SED, neither of them AIR/WATER, so no visit changes another cell's exposure; rolls are drawn per exposed cell in index order as before.
  - Phase and Classic evaporation: a visit writes only its own cell and decides on its own material and heat, so a 64-cell `hit_mask` of (material, Q) gives a block's visits up front.
  - Tests `sparse_air_and_row_scans_match_the_whole_domain_loops` (scrambled worlds at 1, 4, 9 and 30 % AIR, storm/calm/rain ticks, compared with a test switch that forces the plain loops; three mutations of the new code each fail it) and `hit_mask_packs_the_cell_test`.
- 236a439 — a block with no WATER/ICE skips the heat read (gas has no water: phase back to 0.09 ms).
- c091402 — cap 74 → 76 and www refresh. bc1315a, b974a47 — test fix (debug shift) and rustfmt.

**2. Native (`season_prof`, r74, 150 ticks averaged per season window; ms).**

| stage | 1bcd506 storm / calm | b974a47 storm / calm |
|---|---|---|
| ocean phase | 2.31 / 2.20 | 1.64 / 1.64 |
| ocean water cycle | 4.26 / 3.96 | 3.43 / 3.47 |
| ocean erode (storm only) | 5.22 | 3.02 |
| ocean dissolve (storm only) | 4.75 | 0.31 |
| ocean flow | 7.60 / 7.71 | 4.04 / 5.17 |
| **ocean chem step** | **44.1 / 23.4** | **31.9 / 19.6** |
| gas chem step | 36.6 / 21.4 | 34.7 / 21.3 (unchanged) |

`play_loop_prof` r74 (four ticks with chem, mirror and render, early ticks = storm): ocean 58.5 → 44.3 ms, gas 36.3 → 34.7 ms. Flow is now bound by its ~15 k moves a tick (two occupancy writes each), not its scan.

**3. Chrome (headless, Low 960×540, chem every 4 ticks; presented fps, step ms, busy; study build of 236a439).**

| case | §6h (1bcddba) | §6i, 4 runs |
|---|---|---|
| **ocean r76** | 14.43–15.57, 15–30 ms, 65–81% | **14.93, 15.10, 15.60, 15.53; 15.9–24.2 ms, 66–76%** |
| **gas r76** | 14.87–15.03 | **14.90, 15.27, 15.07, 14.97; 15.9–17.7 ms, 65–69%** |
| ocean r78 | 13.10–15.03 | 15.03, 13.20, 14.33, 13.87; 26–41 ms, 77–85% |
| gas r78 | 14.93–15.03 | 15.43, 15.17, 12.93, 15.17; 21–47 ms, 73–78% |

Every run ends `books=ok energy_drift=0 error=none`.

**4. Cap: `CHEM_RADIUS_MAX` 74 → 76 (c091402).** At r76 both presets hold ~15 on four runs and ocean's lowest (14.93) is above the r72/r74 noise floor (~14.5). r78 fails on both (ocean 13.20, gas 12.93). The slider (`max="76"`, title), `playNum(…, 2, 76)` and the Rust clamp match; an r80 request on :8765 clamps to 76.

**5. Exactness evidence.**
- Parity vs 1bcd506 is byte-identical after each code commit: chem `8ef8f682…`, play `dd849e0b…`, big `9248e8d7…`, split `bc8ac3af…`.
- A 600-tick run of every preset at r36 (both dynamics settings; fingerprint, energy and AIR count every 60 ticks, two and a half seasons, AIR rising to 25.7 k, near the 1/8 sparse limit; the unit test covers the fallback at 30 % AIR) is identical to 1bcd506.
- p6 goldens 3/3; core and wasm tests pass; local CI green.
- Ship wasm == native world and frame on ocean / gas r76 and r40, presets 0 and 1 at r30, and long runs (ocean r40 1,100 steps to tick 4,416; gas r30 600 steps).
- Chrome on :8765 reads the default world `pmbj7za5…` and frame `nx4b62u5…`, ABI 17; the lab page reports `match_expected=true` twice.

**6. Next wall.**
- (a) Ocean past r76: heat (4.6 ms), the combine2 stage (3.7 ms: N scan, network, gas rise gather), flow's moves (~15 k a tick, each two occupancy writes through the chunk map), evaporation's heat mask (memory-bound over Q) and the mirror walk.
- (b) Gas past r76: convection (density test and gather) and heat; persistent radius-ordered materials remain the lever.
- (c) The incremental mirror material hash is still skipped, as Owen asked.

**Build.** wasm (c091402, ABI 17) md5 `c1c9a6e8b286f04c3866840988bce2ab` (1,471,693 B), play.html `912b608fbbe4195a4e15b8bac5b5936c`, bitblock-lab.html `dd9a9ccde675931a5817ec90d825b556`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged).
- Served at 127.0.0.1:8765 and self-playtested: ocean and gas r76 `books=ok energy_drift=0 error=none`, r80 clamps to 76, and only the favicon returns 404.
- Screenshots: `/workspace/render_scale/s6i/playtest/{default,chem2_r76,chem4_r76,chem4_r80}.png`.
- Sweep shots: `/workspace/render_scale/s6i/chrome/`.

Soft DDA, bcc-voxel-demo and abi16 are untouched. There is no Desktop copy, and main is untouched.

## 6j. Exact gas-rise gate, row-wise evaporation, branch-free heat shares; play cap stays 76 (2026-11-17, branch `chem/render-scale-2026-11`; Owen "keep going, but make sure to update my desktop after every pass" pt252u) — every hash and book unchanged

This wave is one shippable pass on what stops both presets at r78. Exactness is checked against 4a88cfb. No hash changed, and no win needed a hash change. Every pushed commit was checked locally first: `cargo fmt --check`, clippy `-D warnings`, debug workspace tests, the goldens and the index selftest.

**1. Changes (all exact; 76de3d2).**
- **Gas-rise gate.** The outward gas rise trades a rising cell only with AIR or WATER, and a trade keeps both counts. With neither in the domain (the gas planet) the scan trades nothing, so it and its gather are skipped. Convection then gathers the same parcels itself.
- **Row-wise Classic evaporation.** For each z-run, a row's surface WATER (WATER whose cell above, in the next row up, is not WATER) comes from the two row slices before the row's first visit. Only surface WATER reads its heat and rolls. A visit writes only its own cell, which is neither another cell of the row nor a cell above one.
- **Heat shares.** One branch-free sweep shares every cell as if it had six neighbours. Then the rim cells (`space_faces`, exactly the cells with fewer than six) are re-shared by their live count before any share is read. This covers every `dq` that divides 6000; other `dq` keep the general path.
- **Tests.**
  - New `gas_rise_gate_matches_the_full_scan`, run with and without an AIR cell.
  - `sparse_air_and_row_scans_match_the_whole_domain_loops` now covers the row-wise evaporation.
  - `gathered_heat_matches_general_scatter` gains dq 300 and 2000.
  - Skipping the rim patch fails the heat test. Breaking the buried test fails the scan test.
- 516cf53 — www refresh (the cap is unchanged).

**Tried and dropped (slower, reverted):**
- Fusing the convection gather with its density test, run outermost first: gas combine2 went from 17.7 to 23.5 ms.
- A scatter-order gather through an inverse rsort: 15.4 → 17.6 ms.
- The +y gas rise over z-runs instead of the rise list: ocean combine2 went from 4.3 to 5.3–6.1 ms. Its cost is the trades, not the list.

**2. Native (`season_prof` r78, cap lifted, 150 ticks; calm window ms).**

| stage | 4a88cfb | 76de3d2 |
|---|---|---|
| ocean heat | 5.32 | 4.42 |
| ocean water cycle | 3.94 | 2.43 (rain window 3.97 → 2.18) |
| ocean chem step | 23.0 | 20.6 |
| gas heat | 6.15 | 5.29 |
| gas combine2 + convection | 16.85 | 15.58 |
| gas chem step | 25.1 | 22.9 |

- Gas phases at r78 on 4a88cfb:
  - rise gather 4.2 ms and rise scan 2.5 ms. The scan is now gone on a gas planet.
  - convection density 5.5, preroll 1.0, survivor scan 0.9, replay 2.2 ms (423 k candidates, 200 k swaps).
- Ocean combine2: hand families 0.4, network 0.9, burn/condense 0.1, +y rise 3.2 ms (trades).
- Ocean flow: about 2.2 ms of its 6.3 ms is the ~15 k moves' occupancy writes and 1.1 ms is building the AIR-face candidates.
- `play_loop_prof` r78: ocean 47.0 → 46.0 ms, gas 37.6 → 35.0 ms.

**3. Chrome (headless, Low 960×540, chem every 4 ticks; presented fps / busy). New and 4a88cfb study builds interleaved in one session.**

| case | 4a88cfb (4 runs) | 76de3d2 (4 runs) | 76de3d2 second set |
|---|---|---|---|
| ocean r78 | 15.90, 15.80, 15.30, 15.57; 57–62% | 15.07, 15.00, **13.40**, 14.90; 54–63% | 15.00, 15.20, 15.00, 15.03; 54–61% |
| gas r78 | 15.00, 15.03, 15.03, 15.03; 59–65% | 15.03, 15.03, 15.03, 15.03; 54–59% | 15.00, 15.07, 15.03, 15.03; 55–56% |
| ocean r80 | 13.83, 15.47, 13.97, 14.37; 63–75% | 13.93, 15.37, 13.60, 14.33; 63–72% | — |
| gas r80 | 15.13, 15.07, 15.03, 15.17; 66–71% | 15.30, 15.10, 15.10, 14.80; 60–66% | — |

Every run ends `books=ok energy_drift=0 error=none`.

**4. Cap: stays at 76.**
- The pass cuts the load: busy at r78 is 54–59% vs 57–65% on 4a88cfb. But ocean's first r78 set has a 13.40 run, below the ~14.5 floor.
- This session runs faster than §6i's: 4a88cfb itself holds r78, where §6i measured 13.20 for ocean and 12.93 for gas. So ocean's presented dips track neither load nor build.
- r78 is an option for Owen: 7 of 8 runs on this build are at or above 14.90 for ocean, and all 8 hold for gas. It is not taken, because the four-run rule failed.

**5. Exactness evidence.**
- Parity vs 4a88cfb is byte-identical: chem `8ef8f682…`, play `dd849e0b…`, big `9248e8d7…`, split `bc8ac3af…`.
- 600-tick runs of every preset at r36 match 1bcd506 / 4a88cfb.
- p6 goldens 3/3. Debug and release tests pass.
- Ship wasm == native world and frame on:
  - ocean and gas r76;
  - ocean r40 over 1,100 steps and gas r30 over 600 steps;
  - presets 0 and 1 at r30.
- :8765 serves the default world `pmbj7za5…` and frame `nx4b62u5…`, ABI 17.
- The lab page prints the same match lines as every earlier build (`match_expected=true` twice; the `match_after_*` / `dolly` / `pitch` lines are false on load by design).

**6. Next wall.**
- (a) Ocean's presented dips: they do not track busy, and fewer chem ms did not remove them. Next is the frame cadence and renderer: draw_bit, the delta index, the 10 Hz status books (check_books, book totals and energy sum are about 300 samples per profile).
- (b) Gas convection: gather 4.2 ms plus density 5.5 ms. Persistent radius-ordered materials remain the lever; q is rewritten by heat every tick, so only the material half can persist.
- (c) Flow's moves: two occupancy writes each. The chunk bounds after a write sequence depend on its order, so a batch needs care.

**Build.** wasm (516cf53, ABI 17) md5 `93a7888c057e85d1da3973df2bd6d155` (1,475,369 B), play.html `abd5705119193a462c5d09a762ab613f`, bitblock-lab.html `e3c24a7bb3a2b4230bbe25b0f4feab50`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged).
- Served at 127.0.0.1:8765 and self-playtested: ocean and gas r76 `books=ok energy_drift=0 error=none`, r80 clamps to 76, and only the favicon returns 404.
- Screenshots: `/workspace/render_scale/s6j/playtest/{default,chem2_r76,chem4_r76,chem4_r80}.png`.
- Sweep shots: `/workspace/render_scale/s6j/chrome/`.

Soft DDA, bcc-voxel-demo and abi16 are untouched. There is no Desktop copy, and main is untouched.

## 6k. Ocean's presented dips were periodic full re-renders: exact delta index compaction; play cap 76 → 80 (2026-11-17, branch `chem/render-scale-2026-11`; Owen "keep going, but make sure to update my desktop after every pass" pt252u) — every hash and book unchanged

This wave is one shippable pass on ocean's presented-fps dips that did not follow load. Exactness is checked against dfbc726. No hash changed, and no win needed a hash change. Every pushed commit was checked locally first (`cargo fmt --check`, clippy `-D warnings` with and without `protocol-research`, debug workspace tests with `protocol-research`, the goldens, index drift and the index selftest), and CI is green on both.

**1. Diagnosis (headless Chrome frame cadence, then native).**
- A per-frame trace of the play loop (rAF time and gap, ticks, step / paint / status ms, presented count), long tasks, JS heap, V8 GC trace events and a named-wasm CPU profile, ocean r78 on dfbc726:
  - Chem frames take ~40 ms. Every 11–16 chem steps one takes 110–140 ms. The next rAF then carries 7–8 ticks, which spans two chem steps, so one presented frame is lost and the frame interval reaches ~150 ms.
  - GC is negligible (a few µs; heap 7–19 MB, flat). Long tasks are exactly the step spikes. The status refresh is 5.2–5.6 ms at ~7.5 Hz. Paint is 0.3 ms.
  - The play page's `presented_fps` is the last ~500 ms window at the moment it is read (7 or 8 frames), and the sweeps average three such snapshots. A snapshot containing one spike reads 12–13 fps, which is where single runs like §6j's 13.40 came from.
- Native `spike_prof` (step(4) per chem frame) with temporary stage timers: chem has no spikes (`spike_chem`: median 20.9, p99 28.7 ms at r78). The spikes are `render_game`: a delta frame normally costs ~10 ms, but every 11–16 steps it costs ~72 ms. The delta index (`delta_index`, the grown raster boxes of every bit the last full traversal drew plus the bits each delta frame added) passed 1.5× the full traversal's entries + 4096. Then `render_tinted_delta` fell back to a full redraw to rebuild it. About 11 k entries were appended per ocean frame, mostly repeats of bits already indexed (340 k entries held 232 k distinct bits).

**2. Changes.**
- 7bfd552 — **Delta index compaction (exact).** When the index outgrows the tile lists, `compact_delta_index` drops the entries of bits now off and every repeat of an indexed bit (first entry kept; a per-chunk bitset finds repeats). It then rebuilds the tile lists over what remains, instead of redrawing the frame. The full path stays for an index still above 1.5× the last traversal's after compaction.
  - Why the frames are the same: a delta frame reads the index only for on bits, an entry is a function of its bit and the projection only (`grown_box` of the projected vertices), and a bit that turns on again is recorded by the frame that sees it toggle. So the compacted index holds a box for every on bit the old one did. The chaining argument of `render_tinted_delta` is per frame and does not depend on how long the chain is.
  - `delta_render_matches_full_render` now also runs with no slack and 30-frame chains (12 compactions, and the full path after some), comparing pixels, depths and `world_pixels` against a fresh full render every frame. Truncating the compacted index to the old tile lists fails it.
  - `PlaySession::delta_counts` reports delta frames and compactions (diagnostics only).
- 02176b4 — **Cap 76 → 80; status refresh off painted frames; www refresh.** The 10 Hz status refresh (it sums the books over every chem cell) now skips a frame that just stepped and painted, and runs on a later frame, at most one period late. This is JS scheduling only: the same wasm calls and text.

**3. Native (cap lifted; ocean r78, step(4) per chem frame, 400 frames).**

| | dfbc726 | 7bfd552 |
|---|---|---|
| median / p90 / p99 step | 29.2 / 41.8 / 106.7 ms | 29.4 / 39.8 / 52.4 ms |
| frames > 1.6× median (after warm-up) | 21 (~100 ms, full re-render) | 7 (45–52 ms, compaction ~8 ms + tile lists) |
| delta render, mean | ~10 ms + 72 ms spikes | 4.8 ms |

- `delta_eq` (delta-render session vs full-render session, the same inputs, output buffer compared every step and status every 25 steps): equal on ocean r78 for 200 steps (199 delta frames, 9 compactions), gas r78 for 200 steps (0 compactions; gas never reached the old fallback), and ocean r76 for 120 steps (10 compactions).

**4. Chrome (headless, Low 960×540, chem every 4 ticks; `presented_fps` averaged over 3 snapshots, busy, and the largest step in the traced 7.5 s window). dfbc726 and 02176b4-code study builds interleaved in one session.**

| case | dfbc726 (4 runs) | §6k (4 runs) | §6k second set |
|---|---|---|---|
| ocean r78 | 15.00, 15.07, 14.93, 15.00; 53–55%; step max 111–114 ms | 15.00, 14.93, 15.03, 14.97; 55%; step max 51–52 ms | — |
| gas r78 | 14.90, 15.07, 14.97, 15.07; 53–58% | 15.03, 15.03, 15.03, 15.00; 54–55% | — |
| ocean r80 | 14.30, 15.83, 15.93, 15.87; 64–72%; step max 136–142 ms | 15.00, 15.03, 15.03, 15.03; 58–61%; step max 60–67 ms | 14.97, 15.03, 15.03, 15.03; 59–61% |
| gas r80 | 15.00, 15.03, 15.07, 15.03; 60–63% | 14.97, 15.03, 14.67, 15.03; 61–66% | 15.03, 15.00, 15.00, 15.00; 59–61% |
| ocean r82 | — | 15.00, 15.00, 15.03, 14.73; 62–65% | — |
| gas r82 | — | 13.10, 12.77, 12.63, 12.67 (first snapshot 7.9–9.2) | — |

- Readings above 15 on dfbc726 r80 come from snapshot windows that close just after a burst of frames following a spike. The trace's presented rate there is 14.64–14.74.
- Every run ends `books=ok energy_drift=0`.

**5. Cap: 76 → 80.** Both presets hold ~15 presented fps at r80 over two sets of four runs: ocean's minimum is 14.97 and gas's is 14.67. Ocean now holds even at r82. Gas is the binding preset: for its first ~100 chem steps the convection stirring recolours most of the envelope, so its delta frames redraw a lot. Natively the delta render climbs to ~36 ms at r80 between steps 30 and 90, then falls to ~10 ms. At r80 the tail of that hump touches the start of the measured window (single 120–150 ms steps; one run's traced rate is 13.98 while its snapshots read 14.97). At r82 it fills the first snapshot.

**6. Exactness evidence (vs dfbc726).**
- Parity byte-identical: chem `8ef8f682…`, play `dd849e0b…`, big `9248e8d7…`, split `bc8ac3af…`.
- 600-tick runs of every preset at r36 match the §6i reference file. p6 goldens 3/3. Debug (with `protocol-research`) and release tests pass.
- Ship wasm == native world and frame tags (and `books=ok energy_drift=0`) on ocean and gas r80, ocean r40 over 1,100 steps, gas r30 over 600 steps, and presets 0 and 1 at r30 over 200 steps.
- :8765 serves the default world `pmbj7za5…` and frame `nx4b62u5…`, ABI 17. The lab page prints the same lines as dfbc726.

**7. Next wall.**
- (a) Gas's early stirring: delta frames that redraw most of the envelope for ~100 steps. That cost is real content change. Options are a cheaper redraw path for tint-only changes (no occupancy change, so no reset/neighbour work), or capping how many bits a delta frame redraws before handing the rest to the next frame. The second would change frames, so it is a question for Owen.
- (b) Gas convection: gather 4.2 ms (materials 1.4 ms, heat 2.2 ms) plus density test 5.5 ms native at r78. Persistent radius-ordered materials would save at most ~1.4 ms less upkeep on ~400 k `set_mat` per tick. Convection parks moved parcels in its copy, so it would still need a scratch copy. Heat is rewritten every tick, so it cannot persist. Deferred to the next pass.
- (c) Ocean at r82+: chem ~21 ms plus render ~5 ms native per frame. Flow's occupancy writes and the +y rise trades remain the largest chem items.

**Build.** wasm (02176b4, ABI 17) md5 `dde5277b3873930d00a7ec829b7ad744` (1,477,610 B), play.html `baa488a807c3052867f5375785734f0a`, bitblock-lab.html `4dfcd0240d67acb69cc8933bf201a238`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged).
- Served at 127.0.0.1:8765 and self-playtested: ocean and gas r80 `books=ok energy_drift=0 error=none`, r84 clamps to 80, and only the favicon returns 404.
- Screenshots: `/workspace/render_scale/s6k/playtest/{default,chem2_r80,chem4_r80,chem4_r84}.png`.
- Traces, profiles and sweeps: `/workspace/render_scale/s6k/` (`chrome/`, `sweep{1,2,3_r82}.txt`, `ana.py`, `prof_spikes.py`, `mkstudy.sh`); bins `spike_prof`, `spike_chem`, `delta_eq` in `scale_measure`.

Soft DDA, bcc-voxel-demo and abi16 are untouched. There is no Desktop copy, and main is untouched.

## 6l. Gas's first seconds were tint-only delta frames redrawing the envelope: an exact recolour pass; cap stays 80 (2026-11-17, branch `chem/render-scale-2026-11`; Owen "keep going, but make sure to update my desktop after every pass" pt252u) — every hash and book unchanged

This wave is one shippable pass on gas, the limiting preset after §6k. Exactness is checked against 27ec0bb. No hash changed, and no win needed a hash change. Every pushed commit was checked locally first (`cargo fmt --check`, clippy `-D warnings` with and without `protocol-research`, debug workspace tests with `protocol-research`, the goldens, index drift and the index selftest), and CI is green on both.

**1. Diagnosis (native, gas r80, temporary phase timers in `render_tinted_delta`).**
- During the early stirring (chem steps ~30–90) every changed bit is tint-only: no toggles, ~180–208 k changed bits per chem step, ~15–22 k of them exposed ("loud").
- The old path reset every pixel a changed bit held (only ~200–1,300 pixels), then drew every loud bit plus every indexed bit whose box reached a reset pixel: 30–83 k draws. The gas envelope is open throughout, so ~55 indexed bits stack behind each reset pixel.
- A reset pixel is set to the background at infinite depth, which also sets its tile and coarse depth bounds to infinity. The hidden-bit test then cannot cull the bits stacked behind, so each one is rasterised.
- At the peak: classify ~8 ms, collect ~5 ms, draw ~22 ms.

**2. Change: 0f9f794 — the recolour pass (exact).** When no changed bit toggled, `render_tinted_delta` hands the frame to `recolour_delta`:
- Geometry is unchanged, so each bit draws exactly the fragments (pixel, depth) it drew before, only with new colours, and no pixel's stored depth can change. Only fragments at exactly a pixel's stored depth compete with it.
- **Step 1.** Every loud bit is drawn over the old frame in marking mode. Where a fragment meets the stored depth of a pixel its own bit holds (owner buffer), it replaces the stored colour in place. Everywhere else it is an ordinary `plot` (keep the lesser by depth, then RGB word).
- **Tie record.** A new per-pixel flag `tie` says whether a second fragment may sit at exactly the stored depth. Every equal-depth meeting in `plot` and in the span writer sets it, and a strictly nearer write clears it. Only `false` is a promise. It holds because every fragment at a pixel's final depth reaches the writers (the culls and the hidden-bit test skip only bits strictly behind), and every delta frame draws every bit that gains a fragment.
- **Marks.** An in-place recolour to a higher RGB word on a pixel whose `tie` admits another fragment marks that pixel: another bit could now win the tie.
- **Step 2.** At marked pixels, every indexed or loud bit whose box reaches one is drawn again with ordinary `plot`. A bit bounded strictly behind every marked pixel of its box is skipped, because the rest of the frame already holds its minima. Depths never change and no pixel turns background, so tile and coarse bounds stay valid and `world_pixels` stays exact.
- Classification now reads one cached word set per chunk (open words where the enclosed-bit cull applies, else the on bits) instead of a hashed `is_on` per bit plus a `HashMap` of open words, and skips the toggled-neighbour set when nothing toggled.
- The full argument is in the doc comments of `recolour_delta` and the `tie` field. Toggle frames keep the §6k path unchanged.
- Tests:
  - New `delta_recolour_matches_full_render` covers a solid ball, a sparse random block (gas-like, most bits exposed) and a slab, at 24–161 px (down to the single-pixel path). It runs 16-frame recolour chains with near-equal palette words, so ties flip both ways, plus an occasional toggle frame. Every frame is compared with a fresh full render (pixels, depth bits, `world_pixels`), and it asserts the pass ran ≥100 times and marked pixels.
  - Both delta tests also check that the delta frame's tie record never denies a tie the full frame sees.

**3. Native (cap lifted; step(4) per chem frame, 120 frames; mean over chem steps 25–95 = the stirring hump).**

| gas | 27ec0bb render | 0f9f794 render | 27ec0bb step | 0f9f794 step |
|---|---|---|---|---|
| r80 | 29.7 ms (p90 36.6) | 9.5 ms (p90 9.4 over the run) | 70.0 ms | 45.6 ms |
| r82 | 39.4 ms (p90 48.2) | 12.9 ms (p90 13.7) | 81.3 ms | 52.5 ms |

- At the hump peak the recolour pass costs classify ~5.5 ms plus step 1 ~3.8 ms. Gas marked 0 pixels in every sampled frame of the hump, so step 2 did not run there. The tests do exercise it.
- Ocean is unchanged (render 3.4–5.1 ms mean). Its frames toggle bits, so they take the §6k path. A full render costs the same as before (105–144 ms at r80–r82): the tie flag adds one byte store per write.
- `delta_eq` (delta session vs full-render session, output buffer every step, status every 25 steps) is equal on gas r80 and r82 (150 steps), ocean r80 (150 steps, 6 compactions), ocean r82 (150 steps, 5 compactions), ocean r60, gas r36 (200 steps) and preset 1 r80 and r82.

**4. Chrome (headless, Low 960×540, chem every 4 ticks; `presented_fps` averaged over 3 snapshots, busy; "fresh" = measured from 2 s after the radius change instead of 7 s, so the window covers the stirring hump).**

| case | §6k 27ec0bb | §6l set 1 (4 runs) | §6l set 2 (4 runs) |
|---|---|---|---|
| ocean r80 | (§6k: 14.97–15.03) | 15.00, 14.90, 15.00, 15.43; 59–61%; step max 61–86 ms | 15.00, 15.00, 15.03, 15.00; 58–59%; step max 62–66 ms |
| gas r80 | (§6k: 14.67–15.03) | 14.97, 15.00, 15.03, 15.03; 57–63%; step max 50–67 ms | 15.03, 15.03, 15.00, 15.00; 58–60%; step max 48–52 ms |
| gas r80 fresh | 11.57, 10.07 (first snapshot 5.9–6.2); step max 178–183 ms | 15.00, 15.03, 15.00, 15.07; 75–81%; step max 65–86 ms | 15.07, 14.87, 15.00, 15.03; 75–78%; step max 62–77 ms |
| gas r82 fresh | 8.43, 8.80 (first two snapshots 4.3–5.9) | 15.07, 12.77, 13.47, 13.37, 12.93, 12.93 (first snapshot 7.6–9.6 in five of six) | — |

- The 15.43 ocean reading is a snapshot window closing just after a burst; its traced rate is 14.97.
- Every run ends `books=ok energy_drift=0`.

**5. Cap: stays 80.** r80 now holds on both presets over two sets of four runs, including a fresh gas load's first seconds, which dipped to ~10 fps at 27ec0bb. Fresh gas at r82 still dips in its first snapshot, so r82 fails the rule. Natively the r82 hump step is ~52 ms: chem ~29 ms, mirror ~10 ms, render ~13 ms. In wasm that is over the 66 ms chem budget.

**6. Exactness evidence (vs 27ec0bb).**
- Parity byte-identical: chem `8ef8f682…`, play `dd849e0b…`, big `9248e8d7…`, split `bc8ac3af…`.
- 600-tick runs of every preset at r36 match the §6i reference file. p6 goldens 3/3 (release). Debug workspace tests (with `protocol-research`) pass.
- The shipped wasm, the 27ec0bb wasm and native print the same world and frame tags (and `books=ok energy_drift=0`) on gas r80 (40 warm-up steps), gas r72 (120), ocean r40 (20), preset 1 r80 (30) and preset 0 r36 (60).
- :8765 serves the default world `pmbj7za5…` and frame `nx4b62u5…`, ABI 17. The lab page prints the same lines as 27ec0bb.

**7. Next wall.**
- (a) Gas chem itself: ~27–31 ms native at r80–r82 (convection gather 4.2 ms plus density test 5.5 ms at r78 are the largest items). The mirror adds ~9–10 ms during the hump: the FNV material hash over every domain cell, plus ~200 k changed-bit pushes. Classification is now ~5.5 ms of the ~9.5 ms render.
- (b) Spreading the hump's redraw over several frames would change frames. With the recolour pass it is no longer needed at r80. It remains an option for Owen only.
- (c) Ocean at r82+: unchanged from §6k (flow occupancy writes and the +y rise trades).

**Build.** wasm (2fba429, ABI 17) md5 `3f06883cfd270b69a62d6e9c79e7a142` (1,481,745 B), play.html `d7f39b5a51a785b921fd99b7f9437786`, bitblock-lab.html `c0b70a0289a3b58a967501faa3c4b5b8`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged).
- The radius input's title now says a fresh gas load holds at r80. There is no other page change.
- Served at 127.0.0.1:8765 and self-playtested: ocean and gas r80 `books=ok energy_drift=0 error=none` at 15.0 presented fps, r84 clamps to 80, and only the favicon returns 404.
- Screenshots: `/workspace/render_scale/s6l/playtest/{default,chem2_r80,chem4_r80,chem4_r84}.png`.
- Sweeps, A/B and traces: `/workspace/render_scale/s6l/` (`sweep_r80.txt`, `sw_fresh82.txt`, `ab_t/`, `chrome/`, `ab.sh`, `ana_ab.py`).

Soft DDA, bcc-voxel-demo and abi16 are untouched. There is no Desktop copy, and main is untouched.

## 6m. Gas chem and mirror made cheaper, exactly; cap raised to 82 (2026-11-17, branch `chem/render-scale-2026-11`; Owen "keep going, but make sure to update my desktop after every pass" pt252u) — every hash and book unchanged

This wave is one shippable pass on gas, the limiting preset after §6l, measured with fresh gas loads at r82+. Exactness is checked against 61bf191. No hash changed, and no win needed a hash change. Each commit passed the full local pre-push check first (`cargo fmt --check`, clippy `-D warnings` with and without `protocol-research`, debug workspace tests with `protocol-research`, the goldens, index drift and the index selftest).

**1. Change: 63cf03d (exact).**
- **Convection density test in domain order.** `convect_pass` used to gather every parcel's material and heat into rsort order (scattered reads), then test each pair against its outward neighbour.
  - The test now runs once per domain cell, in domain order, against `Topology::cell_up` (new, built on first use: `cell_up[rsort[k]] == rsort[shell_up[k]]`). The outward neighbour is a few rows or planes away, so the reads stay near each other. The predicate is unchanged, and the result goes into one pass bit per cell.
  - Candidates are the rsort positions whose cell's bit is set, still in rsort order. The driven-roll filter is unchanged. The reverse scan skips a pair one of whose parcels already moved, using a moved bit per rsort position.
  - Why this is exact: a parcel moves at most once per tick, so every test the scan reaches before either parcel of its pair moved reads the values the domain held when the pass started. The old code's in-place "park a massless id" skip is the same as the moved bit. The swaps are replayed with the same `set_mat` / heat-swap calls in the same order.
  - The outward gas rise no longer hands gathered heat to convection (it gathers only materials), so the `gathered` handoff is gone.
- **Mirror.** `mirror_chem` still computes the FNV material hash over every domain cell; it is not shortcut. The per-cell diff is now a 64-bit change mask per block, and each set bit reads occupancy (want/have) through `World::probe` cursors. The toggles are queued in a reused `mirror_edits` buffer and applied afterwards in the same order. Each domain cell is visited once, and an edit flips only its own bit, so reading occupancy before the edits is equivalent.
- **Classify.** The per-chunk word cache keeps every opened chunk's words by index (`open_words` plus `open_cache`), so switching chunks no longer copies 512 B.

**2. Native (cap lifted; instrumented `spike_prof`, step(4) per chem frame; mean over chem steps 25–95 = the stirring hump).**

| case | 61bf191 step | 63cf03d step | chem | mirror | render |
|---|---|---|---|---|---|
| gas r80 | 43.2 ms | 36.9 ms | 26.3 → 22.2 | 7.8 → 6.2 | 8.7 → 8.1 |
| gas r82 | 49.9 ms | 43.6 ms | 28.5 → 23.9 | 8.2 → 6.9 | 12.7 → 12.3 |
| ocean r82 | 39.5 ms | 39.5 ms | 28.0 → 28.1 | 3.6 → 3.6 | 7.1 → 7.1 |

Ocean does not convect, so its chem is unchanged. Its limiters are unchanged from §6k.

**3. Chrome (headless, Low 960×540, chem every 4 ticks; `presented_fps` averaged over 3 snapshots, busy; every run is "fresh", measured from 2 s after the radius change, so the window covers gas's stirring hump). Study build with the cap lifted.**

| case | §6l 61bf191 | §6m set 1 (4 runs) | §6m set 2 (4 runs) |
|---|---|---|---|
| gas r82 fresh | 12.77–15.07 (first snapshot 7.6–9.6 in five of six) | 15.00, 15.03, 14.83, 15.10; 74–76% | 15.03, 15.07, 15.07, 15.03; 72–78% |
| ocean r82 fresh | (§6k: 14.73–15.03) | 15.00, 14.77, 14.93, 15.00; 68–70% | 15.00, 14.93, 14.97, 14.73; 67–70% |
| gas r84 fresh | — | 13.37 (first snapshot 9.4), 15.03, 15.10, 15.10; 81–86% | 15.20, 15.13, 14.70 (first snapshot 13.1), 15.13; 82–85% |
| ocean r84 fresh | — | 14.87, 14.83, 14.80, 14.90; 73–76% | — |

- The lowest r82 single snapshot is 14.2 (ocean, mid-window).
- Every run ends `books=ok energy_drift=0`.

**4. Cap: 80 → 82** (7c8090a: `CHEM_RADIUS_MAX`, the page's `max` and title, and the JS clamp). r82 holds ~15 fps on both presets over two sets of four fresh runs. At r84, a fresh gas load's first window dips in two of eight runs, so r84 fails the rule.

**5. Exactness evidence (vs 61bf191).**
- Parity byte-identical: chem `8ef8f682…`, play `dd849e0b…`, big `9248e8d7…`, split `bc8ac3af…`.
- 600-tick runs of every preset at r36 (with and without dynamics) match the §6i reference file. p6 goldens pass (release). Debug workspace tests (with `protocol-research`) pass, including the convection tests.
- `delta_eq` output is byte-identical to §6l's on gas r80 and r82 and ocean r80 and r82 (150 steps each; the same final frame tags), ocean r60, gas r36 (200 steps) and preset 1 r80 and r82. That covers 600 ticks of r82 gas convection.
- The shipped wasm, the 61bf191 wasm and native print the same world and frame tags (and `books=ok energy_drift=0`) on preset 0 r36 (60 warm-up steps), preset 1 r40 (60), ocean r80 (40) and gas r80 (40) and r36 (120). On ocean r82 and gas r82 (30 each), native equals the shipped wasm; the 61bf191 wasm clamps those radii to 80.
- :8765 serves the default world `pmbj7za5…` and frame `nx4b62u5…`, ABI 17. The lab page prints the same lines as 61bf191.

**6. Next wall.**
- (a) Gas r84: the first fresh window can still dip. The hump step is ~44 ms native at r82: chem ~24 ms (heat ~5 ms, convection, rise), render ~12 ms, mirror ~7 ms (the FNV hash plus ~200 k changed-bit pushes).
- (b) Ocean r84 holds 14.80–14.90, so it is close too. Its chem (~28 ms at r82) is now the larger of the two presets' chem: flow occupancy writes and the +y rise trades.
- (c) Spreading the hump's redraw over frames would change frames. It remains an option for Owen only.

**Build.** wasm (7c8090a, ABI 17) md5 `a32e490544429c85be6b938043be2172` (1,483,299 B), play.html `ff96093bf1fc443fc0b4df542c2a4dc6`, bitblock-lab.html `2d4f8b55327babc44942271aa57d9d77`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged).
- Served at 127.0.0.1:8765 and self-playtested: ocean and gas r82 `books=ok energy_drift=0 error=none` at 15.0 presented fps, a fresh gas r82 load included (15.00 and 15.07), and r86 clamps to 82. Only the favicon returns 404.
- Screenshots: `/workspace/render_scale/s6m/playtest/{default,chem2_r82,chem4_r82,chem4_r86}.png`.
- Sweeps and A/B: `/workspace/render_scale/s6m/` (`chrome/`, `chrome_8765/`, `ab1/`, `ab3/`, `ab4/`, `eq/`).

Soft DDA, bcc-voxel-demo and abi16 are untouched. There is no Desktop copy, and main is untouched.

## 6n. Radius box shows the clamped radius; convection rolls and swaps in domain order, tint-only boxes on demand; cap raised to 84 (2026-11-17, branch `chem/render-scale-2026-11`; Owen "keep going, but make sure to update my desktop after every pass" pt252u) — every hash and book unchanged

This wave is one shippable pass: a small page fix, then fresh gas at r84. Exactness is checked against a8e1a81. No hash changed, and no win needed a hash change. Each commit passed the full local pre-push check first and was pushed on its own, so each has its own CI run, all green.

**1. UI fix: e9ac8c9 (JS only).** Before this, a radius above the cap (or below 2, or fractional) left the box showing the requested value while the world was built at the clamped one. `playChemRadiusVal` now writes the radius it hands to wasm back into the box, and a radius change normalises the box even when no chem world is selected. Checked in Chrome on :8765:

| request | box | status |
|---|---|---|
| 88 (cap 84) | 84 | radius=84 |
| 1 | 2 | radius=2 |
| 40.7 | 40 | radius=40 |
| 90 (empty world selected) | 84 | — |

**2. Diagnosis (native, gas r84, temporary per-pass timers; per chem step over ticks 25–95).**
- Chem ~26 ms in total: convection ~14 ms (density test 5.9, candidates and rolls 3.8, scan 1.3, swaps 3.0), heat 5.4, network 1.2, rise 0.8.
- Render ~17 ms: classification ~8.4 ms for ~215 k tint-only changed bits, and the recolour draw ~8.8 ms for ~55 k exposed ones. Classification projected all 8 cube vertices of every exposed bit to build its grown box, but a tint-only frame reads that box only when its recolour marks a pixel, and gas marked none in the hump.
- Mirror ~7–8 ms: the FNV material hash (a serial chain over every domain cell) plus ~215 k changed-bit probes. It is left as is: the hash decides repainting, and replacing it would change behaviour where it collides.

**3. Change: 8156cbd (exact).**
- **Convection rolls.** The topology's wide-roll table is now in domain order (`cell_roll_cells`). The cells that pass the density test are rolled right after it, and the survivors are marked by rsort position (new `Topology::rsort_pos`, the inverse of `rsort`). The marking has no branch (the roll fails half the time). The scan reads that bitset from the highest position down, which is the old candidate order.
- **Swaps.** The scan marks each swap by its lower cell, and the swaps are then applied in domain order. They are disjoint pairs (each parcel moves at most once), and each `set_mat` reads only its own cell's old value, so the materials, heat and occupancy bits come out the same as in scan order.
- **Density test.** Each mass is scaled once per material (×1010 below, ×1000 above). Wrapping multiplication is associative and commutative, so `mb*1010*ta < ma*1000*tb` gives the same products as `mb*ta*1010 < ma*tb*1000`. The zero-mass test still reads the unscaled mass.
- **Tint-only grown boxes on demand** (`render_tinted_delta`, `recolour_delta`). In a tint-only frame, the box of an exposed bit whose centre lies deeper than 2 in view space is not built during classification. Every vertex of such a bit is in front of the camera: it is at most √3/2 from the centre, and the depth is the same dot product. `recolour_delta` builds the box with the same projection code only when marks exist. Any other bit, and every toggle frame, builds the box as before, including the fallback to a full render when a vertex is behind the camera.
- **Tests.** The roll-table test now checks the domain-order table, `rsort_pos` and `cell_up` against `rsort` and `shell_up`. `delta_recolour_matches_full_render` (which marks pixels) and the convection tests pass.

**4. Native (cap lifted; instrumented `spike_prof`, step(4) per chem frame; mean over chem steps 25–95).**

| case | a8e1a81 step | §6n step | chem | mirror | render |
|---|---|---|---|---|---|
| gas r82 | 44.2 ms | 38.8 ms | 23.9 → 21.4 | 7.6 → 7.3 | 12.2 → 9.6 |
| gas r84 | 51.5 ms | 44.3 ms | 25.9 → 23.2 | 8.4 → 8.0 | 16.8 → 12.7 |
| ocean r84 | 45.4 ms | 44.3 ms | 31.6 → 30.9 | 4.0 → 4.0 | 9.0 → 8.5 |

**5. Chrome (headless, Low 960×540, chem every 4 ticks; `presented_fps` averaged over 3 snapshots, busy; every run is "fresh", measured from 2 s after the radius change). Study build with the cap lifted.**

| case | §6m a8e1a81 | §6n set 1 (4 runs) | §6n set 2 (4 runs) |
|---|---|---|---|
| gas r84 fresh | 13.37 (first snapshot 9.4), 15.03, 15.10, 15.10 / 15.20, 15.13, 14.70 (first snapshot 13.1), 15.13 | 15.00, 15.17, 15.00, 15.07; 75–78% | 15.07, 15.33, 15.00, 15.03; 75–80% |
| ocean r84 fresh | 14.87, 14.83, 14.80, 14.90 | 14.97, 15.03, 15.27, 15.03; 73–78% | 15.03, 14.87, 15.00, 14.90; 75–76% |
| gas r86 fresh | — | 12.33, 13.30, 14.23, 13.40 (first snapshot 7.6–11.1); 85–86% | — |

- The lowest r84 single snapshot is 14.6 (ocean).
- Every run ends `books=ok energy_drift=0`.

**6. Cap: 82 → 84** (853c7b1: `CHEM_RADIUS_MAX`, the page's `max` and title, the JS clamp). r84 holds ~15 fps on both presets over two sets of four fresh runs. At r86, a fresh gas load's first window dips in every run, so r86 fails the rule. Gas still limits, so ocean chem was not taken up this pass.

**7. Exactness evidence (vs a8e1a81).**
- Parity byte-identical: chem `8ef8f682…`, play `dd849e0b…`, big `9248e8d7…`, split `bc8ac3af…`.
- 600-tick runs of every preset at r36 (with and without dynamics) match the §6i reference file. p6 goldens pass (release). Debug workspace tests (with `protocol-research`) pass.
- `delta_eq` output is byte-identical to §6l/§6m's on gas r80 and r82, ocean r80 and r82, ocean r60, gas r36 and preset 1 r80 and r82. On gas and ocean r84 (150 steps), it is byte-identical to a8e1a81 built natively (the same final frame tags, 4 compactions on ocean).
- The shipped wasm, the a8e1a81 wasm and native print the same world and frame tags (and `books=ok energy_drift=0`) on preset 0 r36, preset 1 r40, ocean and gas r80 and r82, and gas r36. On ocean and gas r84, native equals the shipped wasm; the a8e1a81 wasm clamps those radii to 82.
- :8765 serves the default world `pmbj7za5…` and frame `nx4b62u5…`, ABI 17. The lab page prints the same lines as before.

**8. Next wall.**
- (a) r86: a fresh gas load's first window dips to 7.6–11 fps. At r84 the gas hump step is ~44 ms native: chem ~23 (convection ~10, heat ~5.4), render ~13 (the recolour draw ~9), mirror ~8 (the FNV hash plus the changed-bit probes).
- (b) Ocean is now as costly as gas at r84 (~44 ms native; chem ~31). Its limiters are flow occupancy writes and the +y rise trades.
- (c) Replacing the mirror's FNV repaint test with an exact material comparison would differ only where the hash collides. It remains an option for Owen only, like spreading the hump's redraw over frames.

**Build.** wasm (853c7b1, ABI 17) md5 `479d29fb0c4f4df93b84ef00da8ecb7a` (1,484,991 B), play.html `54794458754a3087c276f46b4c96286c`, bitblock-lab.html `b949e3e7abf6cfe23fdcc3c5e50fa7e6`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged).
- Served at 127.0.0.1:8765 and self-playtested: ocean and gas r84 `books=ok energy_drift=0 error=none` at 15.0 presented fps. Two live fresh runs each read gas 15.30 and 15.07 and ocean 14.80 and 14.83. Requesting r88 clamps to 84, and the box shows 84. Only the favicon returns 404.
- Screenshots: `/workspace/render_scale/s6n/playtest/{default,chem2_r84,chem4_r84,chem4_r88,radius_box_r88}.png`.
- Sweeps and A/B: `/workspace/render_scale/s6n/` (`chrome/`, `chrome_8765/`, `ab0/`, `ab1/`, `ab2/`, `eq/`).

Soft DDA, bcc-voxel-demo and abi16 are untouched. There is no Desktop copy, and main is untouched.

## 6o. Ocean chem: sparse +y gas rise, N-family skip, bitset flow queue, single-quotient heat shares; cap raised to 86 (2026-11-17, branch `chem/render-scale-2026-11`; Owen "keep going, but make sure to update my desktop after every pass" pt252u) — every hash and book unchanged

This wave is one shippable pass: ocean chem at r84, then fresh gas and ocean at r86. Exactness is checked against 4f49d83. No hash or frame changed, and the mirror's FNV repaint test and frame spreading were left alone. Each commit passed the full local pre-push check first and was pushed on its own, so each has its own CI run, all green.

**1. Diagnosis (native, ocean r84, temporary per-stage timers; per chem step over ticks 25–95).** Chem ~31 ms in total. The three costliest stages:
- `combine2` 9.1 ms: hand reactions 4.3, of which most was the N-family cube scans (≈25 k N cells, but a single H cell in the world); the reaction network 1.2; the +y rise 3.7, a walk over the whole `topo.rise` list.
- `flow` 7.2 ms, with its candidate `Vec` and `BinaryHeap`.
- `heat` 5.3 ms.

Water 2.8 and phase 2.3 make up most of the rest.

**2. Change: a0d62b8 (exact).**
- **N-family skip.** Every N-family reaction needs at least three H cells in one cube. If the world holds fewer than three H cells, no cube can react, so the scan is skipped. (`full_scans()` keeps the old scan for tests.)
- **Sparse +y rise** (`gas_rise_up`, Classic water cycle only; core gravity still uses `gas_rise_outward`). The pass returns at once if there is no AIR or WATER. Otherwise one pass over the domain marks, by position in the rise list (new `Topology::rise_pos`, the inverse of `rise`), each pair whose lower cell is rising at the start of the tick. The marked pairs are then visited in ascending list order. Each pair re-checks the live materials, the shell block and the roll exactly as before. This visits the same pairs in the same order:
  - A swap writes only its own two cells, and each cell is the lower cell of at most one +y pair.
  - The list runs from the top row down, so the pair whose lower cell is a given pair's upper cell was visited earlier. No later swap can therefore move gas into a lower cell that has not been visited yet.
  - So a pair whose lower cell is not rising at the start is still not rising when its turn comes. It is skipped, as the old loop's first test skips it.
- **Bitset flow queue.** The sparse flow path's candidate list and heap are now a bitset over `ysort` positions. It is seeded with the WATER neighbours of AIR cells. The sweep takes the lowest set bit, and a move pushes only positions above the current one. So the visit order is the same ascending, each-once order the heap gave.
- **Heat shares, one quotient.** `(q / d).max(0) / 6 == q.max(0) / (6·d)` for positive `d`, so each share is one division, and the remainder is `q − 6·share`.
- **CO2 count.** Summed by 255-cell chunks (the same integer).
- **Test.** `sparse_rise_and_n_skip_match_the_full_scans` runs `combine2_pass` with fast and full scans on scrambled Classic worlds (AIR, WATER, STEAM, O2, CO2, N, NH3, ROCK; 0–4 H cells; ticks 5–131). It requires identical materials, heat and reports, with rises in every case. The §6i sparse-flow test covers the flow queue.

**3. Native (cap lifted; instrumented `spike_prof`, step(4) per chem frame; mean over chem steps 25–95).**

| case | 4f49d83 step | §6o step | chem | mirror | render |
|---|---|---|---|---|---|
| ocean r84 | 44.3 ms | 35.8 ms | 30.9 → 22.8 | 4.0 → 3.9 | 8.7 → 8.4 |
| ocean r86 | 43.2 ms | 38.1 ms | 28.7 → 24.1 | 4.2 → 4.1 | 9.5 → 9.2 |
| gas r86 | 49.6 ms | 48.1 ms | 24.7 → 23.3 | 8.5 → 8.2 | 16.0 → 16.1 |

Ocean r84 per stage after the change: flow 5.9, combine2 4.6, heat 4.0, water 2.8, phase 2.3 (22.5 ms in all).

**4. Chrome (headless, Low 960×540, chem every 4 ticks; `presented_fps` averaged over 3 snapshots, busy; every run is "fresh", measured from 2 s after the radius change). Study build with the cap lifted.**

| case | §6n 4f49d83 | §6o set 1 (4 runs) | §6o set 2 (4 runs) |
|---|---|---|---|
| gas r86 fresh | 12.33, 13.30, 14.23, 13.40 (first snapshot 7.6–11.1); 85–86% | 15.10, 15.13, 15.27, 15.17; 80–84% | 15.13, 15.20, 15.13, 15.13; 82–83% |
| ocean r86 fresh | — | 15.00, 14.97, 15.03, 15.20; 63–69% | 15.03, 15.03, 15.00, 15.07; 63–68% |
| gas r88 fresh | — | 12.10, 11.27 (first snapshot 7.0–7.8); 86–87% | — |
| ocean r88 fresh | — | 14.47, 15.67; 72–74% | — |

- The lowest r86 single snapshot is 14.9 (ocean). Every run ends `books=ok energy_drift=0`.
- In a wasm CPU profile of gas r86, `step_laws` (convection, inlined) takes ~33% of samples, `draw_bit_with` ~13%, `heat_pass` ~9%, idle ~10%.

**5. Cap: 84 → 86** (8b39ea2: `CHEM_RADIUS_MAX`, the page's `max` and title, the JS clamp). r86 holds ~15 fps on both presets over two sets of four fresh runs. At r88, a fresh gas load's first window dips in every run (7.0–7.8 fps), so r88 fails the rule.

**6. Exactness evidence (vs 4f49d83).**
- Parity byte-identical: chem `8ef8f682…`, play `dd849e0b…`, big `9248e8d7…`, split `bc8ac3af…`.
- 600-tick runs of every preset at r36 (with and without dynamics) match the §6i reference file. Core tests pass (release, 329). The full pre-push check passes.
- `delta_eq` output is byte-identical:
  - to §6l's on gas r80 and r82, ocean r80 and r82, ocean r60, gas r36, and preset 1 r80 and r82;
  - to §6n's on gas and ocean r84;
  - to 4f49d83 built natively on gas and ocean r86 (150 steps).
- The shipped wasm, the 4f49d83 wasm (`479d29fb…`) and native print the same world and frame tags (with `books=ok`) on ocean and gas r84, gas r36 (warm 5) and preset 1 r60. On ocean and gas r86, native equals the shipped wasm; the 4f49d83 wasm clamps those radii to 84.
- :8765 serves the default world `pmbj7za5…` and frame `nx4b62u5…`, ABI 17. The lab page prints the same lines as before.

**7. Next wall.**
- (a) r88: a fresh gas load's first window dips to 7–8 fps. At r86 the gas hump step is ~48 ms native: chem ~23 (convection, mostly the bandwidth-bound density test ~5.4; heat ~4), render ~16 (classification ~4.5, recolour draw ~11.5 for ~80 k bits at ~150 ns each, 8 exact vertex projections per bit), mirror ~8.
- (b) Ocean r86 is ~38 ms native (chem ~24: flow ~6, combine2 ~5, heat ~4). It has toggle-frame render spikes (p90 ~24 ms, max ~60).
- (c) Replacing the mirror's FNV repaint test with an exact material comparison, or spreading the hump's redraw over frames, remain options for Owen only.

**Build.** wasm (8b39ea2, ABI 17) md5 `268110ad3a792888b5040f94a1b820b5` (1,487,153 B), play.html `3163918a371b65228c69ef174a6dc6e3`, bitblock-lab.html `79876256a14b22f4a59a7d4245192401`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged).
- Served at 127.0.0.1:8765 and self-playtested: ocean r86 (15.0 presented fps) and gas r86 (15.1) both read `books=ok energy_drift=0 error=none`. Requesting r90 clamps to 86, and the box shows 86 (1 → 2, 40.7 → 40). Only the favicon returns 404.
- Screenshots: `/workspace/render_scale/s6o/play/{default,chem2_r86,chem4_r86,chem4_r90,radius_box_r90}.png`.
- Sweeps and A/B: `/workspace/render_scale/s6o/` (`chrome/`, `prof/`, `ab0/`, `ab1/`, `eq/`).

Soft DDA, bcc-voxel-demo and abi16 are untouched. There is no Desktop copy, and main is untouched.

## 6p. Recolour reuses strict hidden-bit proofs; each bit recorded once in the delta index; cap raised to 88 (2026-11-18, branch `chem/render-scale-2026-11`; Owen "keep going, but make sure to update my desktop after every pass" pt252u) — every hash and book unchanged

This wave is one shippable pass: fresh gas r88's first window, then ocean's spikes on frames where bits switch on or off. Exactness is checked against 16f8dda. No hash or frame changed. The mirror's FNV repaint test and frame spreading were left alone. Each commit passed the full local pre-push check first and was pushed on its own, so each has its own CI run, all green.

**1. Diagnosis (native, cap lifted, temporary counters and timers).**
- **Gas r88:** about 20 ms of the 55 ms step is render, almost all of it the recolour pass of tint-only frames.
  - Of ~99.2 k loud (exposed, retinted) bits per frame, 98.9 k fail the exact hidden-bit test: they lie behind the near surface.
  - Each still paid 8 vertex projections and the test, ~15 ms in all. Drawing the ~300 visible ones costs 0.3 ms.
- **Ocean r88:** every chem frame switches bits on or off. Besides the first-window load, two spikes stand out:
  - **Compaction:** ~55–60 ms native at steps ~44 and ~90, about 20 ms to compact plus ~35 ms to rebuild the per-tile lists.
  - **Duplicate entries:** 85% of the index tail that compaction dropped (~0.5 M of 0.6 M entries) were repeat entries of bits already indexed. They were re-recorded as "fresh" bits each frame (bits that turned on, and on neighbours of bits that turned off). That growth also forced wasm memory growth: Chrome reported 9–17 ms of GC in every ocean run.

**2. Change 1: 643d368 (exact).** Tint-only frames reuse strict hidden-bit proofs.
- When the marking pass's exact hidden-bit test proves that a bit loses every depth comparison strictly, the bit is remembered (`recolour_hidden`).
- A tint-only frame changes no geometry and no depth, so a later tint-only frame skips that bit: it would write and mark nothing.
- Every full render, resize and frame that switches a bit clears the proofs first, so they are only reused under the depths they were proven against.
- `delta_recolour_matches_full_render` compares 16-frame chains against fresh full renders, with every skipped bit rasterised and checked to write nothing. It now also requires that proofs were reused.

**3. Change 2: a8af425 (exact).** Each bit is recorded once in the delta index.
- A delta frame no longer re-records a bit that already has an entry (`IndexedBits`, a per-chunk bitset). It is built on the first recording delta frame after a full traversal, and compaction rebuilds it from the bits it keeps.
- An entry depends only on its bit and the (bit-identical) projection, so a repeat entry adds nothing. Compaction already kept only a bit's first entry. A delta frame reads entries only for bits that are on, and a bit drawn twice gives the same frame as drawn once.
- The index grows far more slowly, so compactions become rare (ocean r88: 2 → 0 in 150 steps; ocean r60: 8 → 1) and the wasm heap stops growing (Chrome GC 0 ms).
- `delta_render_matches_full_render` still requires compactions (at least 3; fewer now happen).

**4. Native (cap lifted; instrumented `spike_prof`, step(4) per chem frame; mean over chem steps 25–95).**

| case | 16f8dda step | §6p step | chem | mirror | render (mean / p90 / max) |
|---|---|---|---|---|---|
| gas r86 | 47.8 ms | 38.8 ms | 23.1 → 23.2 | 8.1 → 8.0 | 16.1 / 16.6 / 34 → 7.2 / 7.4 / 30 |
| gas r88 | 55.3 ms | 41.9 ms | 25.3 → 24.8 | 9.5 → 8.6 | 20.0 / 20.9 / 48 → 8.0 / 9.5 / 38 |
| ocean r86 | 38.8 ms | 36.7 ms | 24.3 → 24.2 | 4.2 → 4.2 | 9.5 / 24.4 / 58 → 7.6 / 21.0 / 63 |
| ocean r88 | 44.1 ms | 39.3 ms | 27.5 → 25.8 | 4.6 → 4.5 | 11.2 / 29.8 / 75 → 8.2 / 25.3 / 75 |

- Ocean r88 steps above 1.6× the median: 22 → 12. The 88–97 ms spikes at steps 29, 44, 89 and 90 are gone. What remains is the first window after load (steps 1–12, 60–106 ms).

**5. Chrome (headless, Low 960×540, chem every 4 ticks; `presented_fps` averaged over 3 snapshots, busy; every run is "fresh", measured from 2 s after the radius change). Study build with the cap lifted, from the shipped code.**

| case | §6o 16f8dda | §6p set 1 (4 runs) | §6p set 2 (4 runs) |
|---|---|---|---|
| gas r88 fresh | 12.10, 11.27 (first snapshot 7.0–7.8) | 15.10, 15.07, 15.03, 15.07; 78–79% | 15.13, 15.03, 15.07, 15.03; 78–80% |
| ocean r88 fresh | 14.47, 15.67 | 15.03, 15.00, 15.00, 15.37; 68–70% | 15.03, 15.00, 15.00, 15.00; 68% |
| gas r90 fresh | — | 12.73 (first snapshot 9.2), 15.20; 84–86% | — |
| ocean r90 fresh | — | 15.00, 15.00; 73% | — |

- Every run ends `books=ok energy_drift=0`. Chrome GC is 0 ms in every r88 run (9–17 ms per ocean run before change 2).
- With change 1 alone, ocean r88 still dipped mid-window (14.40–14.60, snapshots 13.1–13.8, in 2 of 4 runs); change 2 removed those dips.

**6. Cap: 86 → 88** (aac4bea: `CHEM_RADIUS_MAX`, the page's `max` and title, the JS clamp). r88 holds ~15 fps on both presets over two sets of four fresh runs. At r90, a fresh gas load's first window still dips, so r90 fails the rule.

**7. Exactness evidence (vs 16f8dda).**
- Parity byte-identical: chem `8ef8f682…`, play `dd849e0b…`, big `9248e8d7…`, split `bc8ac3af…`.
- 600-tick runs of every preset at r36 match the §6i reference file. Core tests pass (release, 329), and the full pre-push check passes.
- `delta_eq` (full rgba and status compared on every step) is equal on every case: preset 1 r80/r82, ocean r60/r80/r82/r84/r86/r88, and gas r36/r80/r82/r84/r86/r88.
  - It is byte-identical to the earlier references on every gas and preset 1 case, and to 16f8dda built natively on gas r88.
  - On ocean, the outputs differ from the references only in the diagnostic compaction count. The final frame tags are the same.
- The shipped wasm, the 16f8dda wasm (`268110ad…`) and native print the same world and frame tags (with `books=ok`) on ocean and gas r86, gas r36 (warm 5) and preset 1 r60. On ocean and gas r88, native equals the shipped wasm; the 16f8dda wasm clamps those radii to 86.
- :8765 serves the default world `pmbj7za5…` and frame `nx4b62u5…`, ABI 17. The lab page prints the same lines as before.

**8. Next wall.**
- (a) r90: a fresh gas load's first window dips (9.2 in one of two runs; busy 84–86%). At r88 the gas hump step is ~42 ms native: chem ~25 (convection, heat), mirror ~9 (the FNV hash plus changed-bit probes), render ~8.
- (b) Ocean's first window after load (steps 1–12, 60–106 ms native) is the bulk of what is left of its spikes. It is driven by the large early redraw lists of frames that switch bits.
- (c) Replacing the mirror's FNV repaint test with an exact material comparison, or spreading the hump's redraw over frames, remain options for Owen only.

**Build.** wasm (aac4bea, ABI 17) md5 `211d5d8b9163fcd0846107a9e51f838b` (1,488,507 B), play.html `a1cefd1dbbe7d8e3160150b8a46f8795`, bitblock-lab.html `d5c2d076e344c944880515b7fa3c820d`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged).
- Served at 127.0.0.1:8765 and self-playtested: ocean r88 (15.0 presented fps) and gas r88 (15.1) both read `books=ok energy_drift=0 error=none`. Requesting r92 clamps to 88, and the box shows 88 (1 → 2, 40.7 → 40). Only the favicon returns 404.
- Screenshots: `/workspace/render_scale/s6p/play/{default,chem2_r88,chem4_r88,chem4_r92,radius_box_r92}.png`.
- Sweeps and A/B: `/workspace/render_scale/s6p/` (`chrome/`, `prof/`, `ab0/`–`ab3/`, `abcache/`, `eq/`, `eq2/`).

Soft DDA, bcc-voxel-demo and abi16 are untouched. There is no Desktop copy, and main is untouched.

## 6q. Delta classify without per-neighbour hashing; cap stays 88 (2026-11-18, branch `chem/render-scale-2026-11`) — every hash and book unchanged

This wave is one shippable pass on two first windows: gas r90's, and ocean's right after load. Exactness is checked against 56be670. No hash or frame changed. The mirror's FNV repaint test and frame spreading were left alone. Each commit passed the full local pre-push check first and was pushed on its own, so each has its own CI run, all green.

**1. Diagnosis (Chrome CPU profiles of fresh r90 loads, split per `bitblock_play_step`; cap lifted in a study build).**
- **Gas r90's first window is a threshold effect.** For the first ~2.5 s one sim tick costs ~64–66 ms in wasm (chem ~39, mirror ~12, render ~13), right at the 66.7 ms budget of 15 Hz.
  - When one frame runs late, the next frame catches up with two ticks (~130 ms, about 7.5 fps), and the loop stays in that double-tick state until the tick cost falls (to ~50 ms by 5 s).
  - So a fresh run either holds 15 fps or dips to 7–13 fps in its first snapshot. The 56be670 build behaves the same way.
- **Natively, the step-2 spike is the per-tile list build** (`build_delta_csr`, ~50 ms at gas r90) on the first delta frame. The step-32 spike in one earlier run did not recur.
- **Within render, part of the classify cost was hashing.**
  - The set of bits within one step of a toggled bit was a `CoordMap` of 27 neighbours per toggled bit.
  - The per-chunk "exposed words" lookup used a last-chunk cache over a `CoordMap`. A busy tick's changed bits jump between nearby chunks, so most lookups hashed.
  - In ocean's first window this hashing was ~1.2 ms per tick in wasm.

**2. Change: 45e01fa (exact).** The classify step no longer hashes per neighbour.
- The toggled-neighbour set is now an `IndexedBits` (per-chunk bit words, with new `contains` and `clear`). It is kept on the renderer for its storage.
- `IndexedBits` and the classify step's exposed-words lookup both keep a 64-way cache of recent chunks, direct-mapped by the chunk's low coordinate bits, in front of their maps.
- Both are pure caches of the same lookups, so the sets, the index, the frames and the hashes are unchanged. `delta_index` membership and compaction use the same `IndexedBits` (and its `clear`).

**Tried and dropped (all exact, none shipped):**
- **Hidden proofs kept across frames that switch bits.** Validity was tracked per tile through a reset-frame serial. Ocean's early redraw got slightly cheaper, but gas's draw cost rose from ~5.5 to ~7 ms per tick in wasm (proof inserts and lookups on frames whose resets invalidate them).
- **The same 64-way chunk cache in `World::probe`** (the mirror's occupancy probes). No measurable change: the mirror's cost there is memory, not hashing.

**3. Native (cap lifted; instrumented `spike_prof`, step(4) per chem frame; mean over chem steps 25–95; gVisor sandbox, noisy).**

| case | 56be670 step | §6q step | chem | mirror | render (mean / p90 / max) |
|---|---|---|---|---|---|
| ocean r88 | 39.7 ms | 38.1 ms | 26.2 → 25.2 | 4.5 → 4.3 | 8.2 / 25.3 / 91 → 7.8 / 23.9 / 74 |
| ocean r90 | 44.5 ms | 40.8 ms | 28.5 → 26.6 | 4.9 → 4.7 | 10.3 / 31.0 / 93 → 8.7 / 29.9 / 88 |
| gas r88 | 41.5 ms | 40.6 ms | 24.7 → 24.7 | 8.6 → 8.0 | 7.7 / 8.7 / 36 → 7.5 / 8.4 / 35 |
| gas r90 | 44.5 ms | 45.6 ms | 26.3 → 27.1 | 8.5 → 9.0 | 9.4 / 10.3 / 57 → 9.1 / 10.1 / 43 |

- First window, native (total / render per step, steps 1–12): ocean r90 step 1 went from 164 / 93 to 123 / 88 ms; steps 2–12 are within noise. Gas r90 step 2 went from 93 / 57 to 77 / 43 ms.

**4. Chrome per-tick cost (wasm, fresh r90; mean over single-tick steps of the first 2.5 s; two profiled runs per build).**

| case | 56be670 | §6q (shipped) |
|---|---|---|
| ocean r90: all / render / classify hashing | 57.4, 55.7 / 10.6, 9.9 / 1.2 ms | 55.6, 55.2 / 9.6, 9.6 / 0.0 ms |
| gas r90: all / render / classify hashing | 63.6, 65.2 / 12.0, 13.2 / 1.4–1.9 ms | 66.2, 67.0 / 13.6, 13.4 / 1.1–1.3 ms (within run-to-run noise) |

- The gain is real on ocean: ~1 ms per tick of render in the first window, and the hashing is gone. Gas's first window did not move measurably: its hashing is a small share, and chem plus the mirror make up ~51 of its ~65 ms.

**5. Chrome cadence (headless, Low 960×540, chem every 4 ticks; `presented_fps` averaged over 3 snapshots; each run fresh, measured from 2 s after the radius change). Study build of 45e01fa with the cap lifted.**

| case | set 1 (4 runs) | set 2 (4 runs) |
|---|---|---|
| gas r90 fresh | 15.13, 12.53 (first snapshot 7.4), 15.27, 13.93 (11.5); busy 81–85% | 15.13, 15.37, 15.10, 14.67 (13.8); 81–87% |
| ocean r90 fresh | 14.87, 14.97, 14.83, 15.03; 71–76% | 15.00, 15.03, 15.03, 15.00; 71–78% |
| gas r88 fresh (shipped www) | 15.03, 15.07, 15.03, 14.93; 75–76% | — |
| ocean r88 fresh (shipped www) | 15.00, 15.03, 15.00, 15.03; 66–67% | — |

- Every run ends `books=ok energy_drift=0`.

**6. Cap: stays at 88.** Ocean r90 holds ~15 fps over both sets. Gas r90 still drops into the double-tick state in 2 of 8 fresh runs (first snapshots 7.4 and 11.5), so r90 fails the rule. 2095d8e refreshes the www for the new code; the cap is unchanged.

**7. Exactness evidence (vs 56be670).**
- Parity byte-identical: chem `8ef8f682…`, play `dd849e0b…`, big `9248e8d7…`, split `bc8ac3af…`.
- 600-tick runs of every preset at r36 match the §6i reference file. Render tests pass, and the full pre-push check passes (fmt, clippy both feature sets, workspace tests, release goldens, index drift, bbgraph selftest).
- `delta_eq` (full rgba and status compared on every step) is equal on all 16 cases: preset 1 r80/r82, ocean r60/r80–r90, and gas r36/r80–r90.
  - The outputs match the §6p references byte for byte (compaction counts included) on every case up to r88.
  - At r90 they match 56be670 built natively.
- The shipped wasm, the 56be670 wasm (`211d5d8b…`) and native print the same world and frame tags (with `books=ok`) on ocean r88, gas r88 and preset 1 r60 (warm 30, 20 steps).
- :8765 serves the default world `pmbj7za5…` and frame `nx4b62u5…`, ABI 17. The lab page prints the same lines as §6l.

**8. Next wall.**
- (a) Gas r90's first window: ~65 ms per tick in wasm against a 66.7 ms budget. Chem is ~39 ms (`step_laws`, heat pass, combine pass) and the mirror ~12 ms (the FNV hash over ~3 M domain cells plus the changed-bit occupancy probes). The render share (~13) is no longer the main lever. Lifting r90 needs a few ms out of chem, or the mirror's share.
- (b) The first delta frame's per-tile list build (~50 ms native at r90) is still a one-off spike just after load.
- (c) Replacing the mirror's FNV repaint test with an exact material comparison, spreading work over frames, or a catch-up policy that does not double-tick after one late frame remain options for Owen only. The last would change which frames are rendered, so the frame tags would change.

**Build.** wasm (2095d8e, ABI 17) md5 `56bffeeba01cb896e617bb88f5c6b76e` (1,504,423 B), play.html `1c9bf5fd62ceaac5ecc0fbe427265077`, bitblock-lab.html `222d3671a311493310e1a60b43f64cea`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged).
- Served at 127.0.0.1:8765 and self-playtested: ocean r88 (15.0 presented fps) and gas r88 (15.1) both read `books=ok energy_drift=0 error=none`. Requesting r92 clamps to 88, and the box shows 88 (1 → 2, 40.7 → 40). Only the favicon returns 404.
- Screenshots: `/workspace/render_scale/s6q/play/{default,chem2_r88,chem4_r88,chem4_r92,radius_box_r92}.png`.
- Sweeps, profiles and A/B: `/workspace/render_scale/s6q/` (`chrome/`, `chrome_final/`, `ab0/`–`ab5/`, `eq/`).

Soft DDA, bcc-voxel-demo and abi16 are untouched. There is no Desktop copy, and main is untouched.

## 6r. Mirror uses the material's occupancy; chunk caches fit a domain row; cap raised to 90 (2026-11-18, branch `chem/render-scale-2026-11`) — every hash and book unchanged

This wave is one shippable exact pass, checked against b93b429. The gain came from the mirror's non-FNV costs and the renderer's chunk lookups, not from changing chem laws. The mirror's FNV check, frame spreading and double-tick catch-up are untouched. Each commit passed the full local pre-push gate and was pushed separately with its own CI run.

**1. Wasm/Chrome diagnosis.** Fresh gas r90, CPU profiles of the first 2.5 s; temporary `inline(never)` builds identify passes that the normal build inlines into `step_laws` and `sim_tick`.
- Within chem, convection dominates: ~55% of chem samples (790 ms of 1434 ms), heat ~18% (264), combine2 ~10% (150). Driven erosion and weather scan the full domain for another ~7% (101).
- Within the mirror, ~40% of samples were `BitProbe::is_on` (162 of 405 ms); the rest includes the unchanged FNV material hash and changed-cell collection.
- Every non-air chem material has its occupancy bit on. Every air cell outside bedrock has its bit off: all writes go through `set_mat`, which maintains that invariant. The mirror was still reading the chem world's bit for each changed cell, although it already held that cell's material.
- The §6q recent-chunk caches were 4 x 4 x 4. A domain row runs `z` fastest across ~12 chunks at r90, so they collided. A whole row fits in 4 x 4 x 16.

**2. Changes (exact).**
- **ea801a3:** the mirror gets chem occupancy from `ChemWorld::occupancy_of(material, coord)`, falling back to the bit probe only for air at the bedrock cell. A debug assertion compares every result to the actual bit. The play world's occupancy is still probed, so external play edits are treated the same way. Same ordered edits and changed-bit list; the FNV loop is byte-for-byte unchanged.
- **2f92081:** the delta classify and `IndexedBits` recent-chunk caches grow from 64 to 256 ways (4 x 4 x 16); `World::probe` gets the same cache behind its existing last-chunk slot. They only remember what the maps answered while the world is immutably borrowed, so collisions/misses can change only lookup cost.
- **Not shipped:** convection loop micro-optimisations (hoisting the roll-table `Option` out of its loop, reducing density-test table loads). The native saving was small and the wasm profile did not justify including them. Chem laws are unchanged. The first-delta-frame CSR build is unchanged too.

**3. Wasm/Chrome A/B (probe builds; first snapshot begins 2 s after the radius change, Low 960×540).**

| gas r90 window | b93b429, 3 runs | §6r, 3 runs |
|---|---|---|
| first 2.5 s, ms per chem tick | 60.7, 60.6, 60.1 | 58.6, 57.9, 58.6 |
| next 2.5 s | 59.5, 58.1, 58.8 | 53.9, 53.8, 54.7 |
| last 2.5 s | 46.9, 46.9, 47.0 | 45.5, 45.0, 46.3 |

- This leaves ~8 ms under 66.7 in the first window, vs ~6 ms for the base in these runs (earlier §6q runs measured ~65 ms). Absolute costs vary with the run; the alternating A/B shows a real ~2–5 ms saving.
- CPU profiles of the normal builds corroborate where the saving went: first-window `BitProbe::is_on` self time 151 → 94 ms per 2.5 s, classify hash-table self time 38 → below the top 14 (less than 13), with more idle time. Chem's share did not improve.

**4. Native A/B (instrumented `spike_prof`, cap lifted, mean over chem steps 25–95; sandbox timing is noisy).**

| case | b93b429 step | §6r step | chem | mirror | render mean / p90 / max |
|---|---|---|---|---|---|
| gas r88 | 40.6 ms | 38.6 ms | 24.7 → 24.5 | 8.0 → 7.1 | 7.5 / 8.4 / 35 → 6.7 / 8.1 / 35 |
| gas r90 | 45.6 ms | 41.8 ms | 27.1 → 26.0 | 9.0 → 7.7 | 9.1 / 10.1 / 43 → 7.8 / 9.2 / 44 |
| ocean r88 | 38.1 ms | 38.4 ms | 25.2 → 25.6 | 4.3 → 4.3 | 7.8 / 23.9 / 74 → 7.8 / 24.0 / 73 |
| ocean r90 | 40.8 ms | 42.4 ms | 26.6 → 27.8 | 4.7 → 5.0 | 8.7 / 29.9 / 88 → 8.7 / 29.1 / 89 |

- Gas's mirror and render save ~2.6 ms at r90. Ocean's overall time is within noise. The first-delta-frame spike remains: gas r90 step 2 77 → 78 ms; ocean r90 step 1 123 → 123 ms.

**5. Chrome cadence (normal study build of 2f92081 with cap lifted; Low 960×540, chem every 4 ticks, 2 s settle, each run fresh; mean `presented_fps` of 3 snapshots).**

| case | set 1 (4 runs) | set 2 (4 runs) |
|---|---|---|
| gas r90 | 15.37, 15.13, 14.93, 15.00; busy 81–83% | 15.07, 15.07, 15.10, 14.93; 79–80% |
| ocean r90 | 15.03, 15.20, 15.03, 15.07; 73–76% | 15.00, 15.00, 15.00, 15.00; 72–74% |
| gas r92 (probe) | 13.77 (first 11.2), 13.03 (middle 9.1); 85–87% | — |
| ocean r92 (probe) | 15.07, 15.37; 75–81% | — |

- All runs end `books=ok energy_drift=0`. The r90 gas first snapshots are 14.6–16.0, no 7–13 fps trough.
- Same-conditions base comparison: b93b429 gas r90 15.17, **12.53 (first 7.5)**, 15.07, 15.40; §6r an extra four runs 15.03, 15.27, 15.23, 15.03. The base's intermittent double-tick state still reproduces, and did not reproduce in any of the 12 §6r r90 gas runs.

**6. Cap: 88 → 90 (46d8c14, with www refresh).** Both presets hold ~15 fps over two sets of four fresh runs. r92 does not pass: gas dips into the first/middle window again. `CHEM_RADIUS_MAX`, the input max/title and the JS clamp are all 90.

**7. Exactness vs b93b429.**
- Parity md5s unchanged: chem `8ef8f6828310e98ad1ed5c3e997d5a27`, play `dd849e0b7f30eeeec7f0eda8c64be363`, big `9248e8d75489a64a1468400da6a1c51a`, split `bc8ac3afa8d7d53571373bba315ca32d`.
- `fp_long 36 600` matches `s6i/fp_ref.txt` byte for byte.
- `delta_eq` rgba and status equal every step in all 16 cases; every output byte-identical to §6q, compaction counts included (preset 1 r80/r82; ocean r60/r80/r82/r84/r86/r88/r90; gas r36/r80/r82/r84/r86/r88/r90).
- Native, shipped wasm and b93b429 wasm (`56bffeeb…`) agree on world/frame/books for ocean and gas r88, preset 1 r60 (warm 30, 20 steps). At r90 native equals shipped; the base wasm clamps to 88.
- Full pre-push gate passes (fmt, clippy both feature sets, workspace tests, release goldens, index drift, bbgraph selftest). Debug checks the new occupancy shortcut against the actual bit on every mirrored change. Lab lines match §6l.

**8. Next wall.** Gas r92's first/middle window still tips over. Convection is ~55% of gas chem, especially the density pass, wide rolls and the outermost-first candidate scan. The mirror's untouched FNV scan still costs several ms over ~3 M material ids. The first delta frame's tile-list build remains a one-off ~50 ms native spike. Any change to FNV, frame spreading or catch-up stays Owen's call.

**Build.** wasm (46d8c14, ABI 17) md5 `267e7b12a5487ef30d0e9204ac9b7bc0` (1,532,949 B), play.html `806759cec0309e4bc8854cc0408d2c27`, bitblock-lab.html `7a0f166651ff6e8f667afc6961244013`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged).
- :8765 playtest: default world `pmbj7za5…`, frame `nx4b62u5…`, ABI 17. Ocean r90 15.0 and gas r90 15.1 fps, both `books=ok energy_drift=0 error=none`. Requesting r94 clamps to 90 and the box shows 90; 1 → 2 and 40.7 → 40. Only favicon 404.
- Screenshots visually checked: `/workspace/render_scale/s6r/play/{default,chem2_r90,chem4_r90,chem4_r94,radius_box_r94}.png`.
- Profiles/A/B/exactness: `/workspace/render_scale/s6r/{chrome,chrome_final,ab1,eq}/`.

Soft DDA, bcc-voxel-demo and abi16 untouched. No Desktop copy, merge, remote deletion, hash or frame change; 8790/8791 left alone.

## 6s. Exact material comparison, spread heavy redraws, no double tick; cap stays 90 (2026-11-18, branch `chem/render-scale-2026-11`) — every hash and book unchanged

This wave covers the three items Owen approved after §6r: (1) the mirror's FNV check becomes an exact material comparison, (2) heavy redraws are spread across frames, (3) the double tick after one late frame stops. Two latent bugs found by the new tests were fixed in their own commits. Each commit passed the full local pre-push gate (now also gated on the bbgraph selftest) and was pushed separately with its own green CI run.

**1. Commits.**

| commit | change | CI |
|---|---|---|
| ea7c151 | (1) mirror: exact per-block material comparison replaces the FNV `mat_hash`; `ChemPlay::material_changes` counts changed cells | [44287247549](https://github.com/Blobicue/bitblock/actions/runs/44287247549) |
| d423e9a | (3) `PlaySession::step_paced` / `bitblock_play_step_paced` (ABI 17, additive): at most one chem step per frame, the rest of a late frame's ticks dropped; JS uses it and shows `dropped=` | [44288191010](https://github.com/Blobicue/bitblock/actions/runs/44288191010) |
| 3f75849 | fix: dig / place keep the mirror cache in step (a cell refilled with its old material before the next mirror was skipped; pre-existing) | [44290238686](https://github.com/Blobicue/bitblock/actions/runs/44290238686) |
| 9951fae | (2) dig / place join the delta chain (edit frames are delta frames); a still-camera full render builds the delta tile lists right after itself | [44291034047](https://github.com/Blobicue/bitblock/actions/runs/44291034047) |
| edfd8d0 | fix: delta redraws keep the tie record of the full render | [44300528861](https://github.com/Blobicue/bitblock/actions/runs/44300528861) |
| d9b3967 | paced frames checked against full renders; www refresh | [44301372843](https://github.com/Blobicue/bitblock/actions/runs/44301372843) |

**2. Cause and fix.**
- *Double tick.* A late frame accumulated ≥ 5 ticks, so the next frame ran two chem steps (~2 × 45 ms in gas r90 wasm) and was late again: the §6q/§6r 7–13 fps troughs. `step_paced` stops before a second chem-due tick in one call and drops the remaining ticks. The ticks that run are exactly those `fixed_tick` would run, so world, history and books are unchanged; only live presentation timing differs.
- *Heavy redraws.* A dig or place forced a full re-render (~345–385 ms native gas/ocean r90 960×540). Each applied edit now records its bit as an occupancy change while the delta chain holds, so the edit frame is a delta frame (~53–93 ms native, `dig_bench`). A place outside the chem domain grows it; that frame stays a full render.
- *Tie record (edfd8d0).* After 9951fae, a dig next to the camera left gas r90 native steps at ~88–92 ms, against ~68–70 ms after the full render the dig used to force. A delta frame redraws every indexed bit whose box reaches a reset pixel; at the pixels it kept it met its own stored fragment and set `tie` for good (also when a bit arrived twice). Every later recolour then marked those pixels and redrew their tiles. Now each bit is drawn once per pass, redraws add the pixels they write to the mask, and a bit meeting its own fragment outside the mask leaves `tie` alone. Pixels and depths are unchanged.

**3. Native (960×540, ms per `step(4)`, `dig_after` / `dig_prof`).**

| gas r90 | before dig | dig step | steady after |
|---|---|---|---|
| 8e498f9 (base) | 42–44 | 382–426 | 67–71 |
| 9951fae | 42–44 | 122–126 | 86–92 |
| edfd8d0 | 42–44 | 126–131 | 69–72 |

- Ocean r90 is unaffected by edfd8d0 (~46–51 → ~39–47 after a dig).
- What remains after a dig is chem, not render: gas r90 chem 26.4 → ~49 ms per step after one dig, render back to ~9.6 ms (7.7 before).

**4. Chrome cadence (normal study build of edfd8d0 with the cap lifted; Low 960×540, chem every 4 ticks, each run fresh; mean `presented_fps` of 3 snapshots).**

| case | set 1 (4 runs) | set 2 (4 runs) |
|---|---|---|
| gas r92 | 15.27, 15.07, 14.63, 14.93; busy 87–89% | 14.83 (first 13.7), 15.20, 14.87, 14.73; 86–88% |
| ocean r92 | 15.47, 15.60, 15.50, 15.47; 81% | 15.57, 15.57, 15.57, 15.50; 79–82% |
| gas r94 (probe) | 13.90 (first 12.7), 14.17 | — |
| ocean r94 (probe) | 15.10, 14.97 | — |
| gas r96 (probe) | 13.10, 13.60 | — |
| ocean r96 (probe) | 14.27, 14.33 | — |

- All runs end `books=ok energy_drift=0`. The 9951fae study build gave the same picture (gas r92 15.03–15.23 with one 14.37, first window down to 13.1).
- The base 8e498f9 at r92: gas 12.13 (first 7.2), 8.57 (first 3.9), 13.43, 12.67. The double tick no longer reproduces in any §6s run.

**5. Cap: stays 90.** Ocean holds at r92–r94. Gas r92 averages ~14.9 but dips to 13.1–13.7 in its first window in several runs, so it is not a clean ~15. r94/r96 fall below in gas (and ocean r96).

**6. Exactness vs 8e498f9.**
- Parity md5s unchanged: chem `8ef8f6828310e98ad1ed5c3e997d5a27`, play `dd849e0b7f30eeeec7f0eda8c64be363`, big `9248e8d75489a64a1468400da6a1c51a`, split `bc8ac3afa8d7d53571373bba315ca32d`.
- `fp_long 36 600` matches `s6i/fp_ref.txt` byte for byte.
- All 16 `delta_eq` outputs are byte-identical to §6r's, frame tags included.
- `wasm_parity` 8e498f9 wasm vs shipped wasm: world/frame/books are the same in 7/7 cases. Native == shipped == base wasm at warm 30 / 20 steps for ocean and gas r90, preset 1 r60 and ocean r88.
- New tests:
  - `step_paced_runs_at_most_one_chem_step_and_the_same_ticks`: each paced frame equals a full render of the same world.
  - `edit_frames_are_deltas_equal_to_full_frames`: delta and full sessions with digs and places give equal frames every step. This is the convergence test.
  - `digs_refilled_before_the_next_mirror_stay_mirrored`.
  - `material_only_change_repaints_chem_frame` (now on `material_changes`).
  - `delta_redraws_keep_the_full_tie_record`: fails without edfd8d0.
- **Frame hashes: no re-baseline needed; old = new.** `step` and `fixed_tick` are unchanged and every frame stays bit-identical to a full render. Default world `pmbj7za5…` → `pmbj7za5…`, frame `nx4b62u5…` → `nx4b62u5…`. Gas r90 frame `kbm7k6qv…` and ocean r90 `lj3wf7ny…` are unchanged.

**7. Next wall.**
- Chem cost after a dig: a gas r90 dig roughly doubles chem per step (convection around the hole), so gas r90 with digs presents ~12.2–12.9 fps (base 5.8–6.4).
- Gas r92's first window.
- Convection (~55% of gas chem).

**Build.** wasm (d9b3967, ABI 17) md5 `3734f5bcb42a094a9a27486e25446b45` (1,540,180 B), play.html `153dbfd5878ad4a13041848ae7180ea7`, bitblock-lab.html `7e3a5f7028813976182bca62b3956638`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged).
- :8765 playtest: default world `pmbj7za5…`, frame `nx4b62u5…`, ABI 17.
  - Ocean r90 15.0 and gas r90 14.8 fps, both `books=ok energy_drift=0 error=none`.
  - r94 clamps to 90. Only favicon 404.
  - Pointer-locked dig ×3 / place ×2: ocean 15–17 fps, with one 2.0 window on the domain-growing place (full render), settled 15.0. Gas 12.2–12.9 fps, `books=ok` throughout.
- Screenshots visually checked, no holes or tearing: `/workspace/render_scale/s6s/play3/*.png`.
- Data: `/workspace/render_scale/s6s/{STATUS.md,eq3,parity3.log,wasmparity3.txt,sweep_s3_*.txt,chrome_s3}`.

Soft DDA, bcc-voxel-demo and abi16 untouched. No Desktop copy, merge, remote deletion, hash or frame change; 8790/8791 left alone.

## 6t. Domain-growing place without a stall; dug gas planet at its undug chem cost; O(n) topology build; cap stays 90 (2026-11-18, branch `chem/render-scale-2026-11`) — every hash and book unchanged

Priority 1: a place that grows the chem domain still forced a full redraw, which caused a ~2 fps window in ocean. Priority 2: gas chem after a dig (26 → 49 ms/step), and gas r92's first window. Each commit passed the full local pre-push gate and was pushed separately, with its own green CI run.

**1. Commits.**

| commit | change | CI |
|---|---|---|
| 05af081 | play: a place that grows the chem domain inserts its cell into the mirror cache, so the frame stays a delta; the edit test now asserts every edit frame, growth included, is a delta equal to a full frame | [44306302544](https://github.com/Blobicue/bitblock/actions/runs/44306302544) |
| d7379ce | chem: `insert_cell` keeps the cached topology and updates it in place (`Topology::inserted`, linear passes, no sort); it falls back to a rebuild for a hashed lookup, a cell outside the box, or a core that moves | [44306572305](https://github.com/Blobicue/bitblock/actions/runs/44306572305) |
| cb23566 | chem: collapse, uplift and plume visit only the cells by the few AIR/WATER cells of a dug gas planet | [44307860265](https://github.com/Blobicue/bitblock/actions/runs/44307860265) |
| cce209b | chem: the outward gas rise visits only the pairs under the few AIR/WATER cells | [44309230179](https://github.com/Blobicue/bitblock/actions/runs/44309230179) |
| 41135c9 | chem: ysort / rise / rsort are counting sorts; the radial frame fills `cell_up` / `rsort_pos` (an insert carries them over) | [44311023786](https://github.com/Blobicue/bitblock/actions/runs/44311023786) |
| 7c071bf | www refresh | [44312279970](https://github.com/Blobicue/bitblock/actions/runs/44312279970) |

**2. Cause and fix.**
- *Place stall.* Growing the domain made the mirror cache one cell short, which forced a full scan and a full render (05af081). Growing it also dropped the chem topology, so the next step rebuilt it, about 900 ms native at r90 (d7379ce). Now the mirror gains the cell in place, and the topology is the old one with its indices shifted and the new cell's entries put in their sorted places. It is exactly what a rebuild gives, checked by Debug equality against a rebuild on plain, sky and radial worlds.
- *Gas after a dig.* A gas planet has no AIR or WATER, so collapse, uplift, plume and the outward rise all took their no-fluid early-outs. One dug AIR cell defeated those early-outs, and all four laws then scanned the whole domain.
  - Collapse now visits only the cells whose inward step lands on an AIR cell. These are the only cells that can fall, because a target written during the pass is marked moved.
  - Uplift and plume visit the cells below a fluid cell, plus the cells below any cell a firing turned fluid.
  - The outward rise starts from the rsort positions under a fluid cell and adds the cells just inside each trade's lower cell, outermost first, with each trade applied in place.
  - In all four cases the visits, the order and the state each visit sees are the same as before. Above a fluid-count limit (n/128 for collapse, n/16 for uplift/plume, n/64 for the rise), the old dense loops run.
- *First step.* The first step after genesis built the topology: three comparison sorts with random key reads, plus the radial frame. The domain is kept in ascending (x, y, z) order, so a stable counting sort by y gives ysort and rise, and a counting sort of ysort by r2 gives rsort. The keys are distinct, so these are the same lists. Sortedness is checked first; otherwise the code falls back to the old sorts. Outward neighbours are computed in domain order and kept as `cell_up`, and the ranks as `rsort_pos`.

**3. Native (r90 960×540 unless noted).**

| measurement | before §6t | after |
|---|---|---|
| step after a domain-growing place, ocean (`place_bench`) | 1304 ms | edit 107–138 ms, next 64–77 ms |
| step after a domain-growing place, gas | 1423–1456 ms | edit 93–143 ms, next 115–157 ms |
| gas chem per step, before → after one dig (`dig_laws`, per law) | 26.5 → 49.0 ms (collapse +9.6, uplift +4.2, plume +3.6, rise +4.4) | 26.5 → 25.7 ms |
| gas r92 first chem step (`first_window`) | 1284 ms (topology ~900) | 379 ms (topology ~300) |
| gas r92 steady chem | 28–29 ms, convection 18–19 | unchanged (convection is a dense law on a dense gas planet) |

**4. Chrome: the domain-growing place (`placefps6t.mjs`, :8765 build vs the §6s build; pointer-locked dig/place ×3, 3 s after each edit).**

| r90 | §6s (cab5c6b) | §6t (7c071bf) |
|---|---|---|
| ocean, growing place | longest frame gap 1433 ms, first 0.5 s window 4.3 fps | gap 217 ms, 12.0 fps, then 15.0 |
| gas, growing places | gaps 1583 / 1567 ms, 3.4 / 3.0 fps | gaps 183 / 117 ms, 12.7 / 11.8 fps, then 15.0 |
| gas after digs (steady) | 11.5–12.9 fps | 15.0 fps |

**5. Chrome cadence (study build of 7c071bf with the cap lifted; Low 960×540; each run fresh; mean `presented_fps` of 3 snapshots, first window in brackets).**

| case | set 1 (4 runs) | set 2 (4 runs) |
|---|---|---|
| gas r92 | 14.67 (13.6), 15.13 (14.7), 14.73 (14.3), 14.73 (13.5); busy 88–91% | 15.17 (14.8), 15.10 (14.3), 14.93 (14.3), 15.17 (14.4); 87–90% |
| ocean r92 | 15.23, 15.40, 15.10, 15.47; 83–84% | 15.27, 15.40, 15.57, 15.43; 82–83% |

**Cap: stays 90.** Ocean r92 is clean. Gas r92 averages 14.9, but its first windows are 13.5–14.8 and set 1 averages below 15, so the rule does not pass. The first window is now set by steady convection (~18 ms native, ~65% of gas chem), not by the one-off topology build.

**6. Exactness.**
- After each chem commit: all 16 `delta_eq` outputs are byte-identical to §6r's, frame tags included, and `fp_long 36 600` equals `s6i/fp_ref.txt`. Parity md5s are unchanged: chem `8ef8f682…`, play `dd849e0b…`, big `9248e8d7…`, split `bc8ac3af…`.
- `wasm_parity`, §6s wasm vs §6t wasm: world, frame and books are the same in 7/7 cases.
- New tests:
  - `inserted_cell_keeps_the_rebuilt_topology`
  - `place_outside_the_domain_steps_as_with_a_rebuilt_topology`
  - `sparse_collapse_uplift_plume_rise_match_the_full_scans`: scrambled worlds with 1–40 fluid cells, plus the sparse path forced onto fluid-rich slabs and a sky ball; every law fires.
  - `gas_rise_gate_matches_the_full_scan`: now with 0, 1 and 60 fluid cells.
  - `counting_sorted_topology_matches_the_comparison_sorts`
  - `edit_frames_are_deltas_equal_to_full_frames`: every growth frame is a delta.
- **Frame hashes: no re-baseline; old = new.** Default world `pmbj7za5…` → `pmbj7za5…`, frame `nx4b62u5…` → `nx4b62u5…`.

**7. Next wall.**
- Gas convection's density pass and roll, about 18 ms native per step at r92: dense work on a fully gaseous ball. This is what gas r92's first window now waits on.
- About 120–250 ms of linear passes on a domain-growing place, one frame. It could be cut further by carrying the lookup slots and lazy roll tables in place.
- Genesis itself takes ~850 ms native at r92.

**Build.** wasm (7c071bf, ABI 17) md5 `d910845b8528c28e16a72979c8c81e54` (1,562,257 B), play.html `3b3cdc3b10a260a29f0528ed978c3c1a`, bitblock-lab.html `1e9e360c5e0dc80cde49d16fe9ba4b71`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged).
- :8765 playtest: default world `pmbj7za5…`, frame `nx4b62u5…`, ABI 17.
  - Ocean r90 15.0 and gas r90 15.1 fps, `books=ok energy_drift=0 error=none`.
  - r94 clamps to 90. Only favicon 404.
- Screenshots of every dig/place were checked by eye; no holes or tearing: `/workspace/render_scale/s6t/{place_new,playtest}/*.png`.
- Data: `/workspace/render_scale/s6t/{STATUS.md,eq1..eq4,parity1..4.log,wasm_parity.log,place_*.log,set1.log,set2.log}`.

Soft DDA, bcc-voxel-demo and abi16 untouched. No Desktop copy, merge, remote deletion, hash or frame change; 8790/8791 left alone.

## 6u. Exact mirror masks, indexed genesis, one-pass convection; gas r92 wasm step −3 ms, genesis −30%; cap stays 90 (2026-11-18, branch `chem/render-scale-2026-11`) — every hash and book unchanged

Focus: gas r92's first window (dense convection on an all-gas ball), then genesis. Each commit passed the full local pre-push gate and was pushed separately, with its own green CI run.

**1. Commits.**

| commit | change | CI |
|---|---|---|
| 26bff9b | play: the chem mirror skips both occupancy probes for a material change where neither the old nor the new id is AIR (such a cell is on in both worlds); new test `play_world_mirrors_chem_occupancy_through_edits` | [44317422232](https://github.com/Blobicue/bitblock/actions/runs/44317422232) |
| 8954761 | play: the mirror builds its change and solid masks 8 cells per u64 word (SWAR byte-nonzero), no per-cell branch; test `swar_mask_matches_bytewise_compare` | [44318395537](https://github.com/Blobicue/bitblock/actions/runs/44318395537) |
| 8c9166d | chem: genesis sorts a packed `(key, hash, d2<<32 \| index)` candidate key and writes layers into loop-index arrays; cells are emitted in loop order, which is ascending `Coord`, so the coordinate sort is gone; exact capacity reserved; test `indexed_genesis_matches_the_sorted_construction` (reference = the former construction) | [44319663511](https://github.com/Blobicue/bitblock/actions/runs/44319663511) |
| de8b425 | chem: convection's density test and roll run per 64-cell word in one pass (no per-cell pass bit array, slice iteration without per-cell bounds checks) | [44320694870](https://github.com/Blobicue/bitblock/actions/runs/44320694870) |
| 9e37e52 | www refresh | [44322417464](https://github.com/Blobicue/bitblock/actions/runs/44322417464) |

**2. Cause and fix.**
- *Mirror.* Gas convection swaps hundreds of thousands of parcels per step, and each changed cell paid two chunk-probe lookups. A gas/rock-to-gas/rock change never changes occupancy, and nothing else edits the play world, so the probes are skipped (asserted in debug and checked against the chem occupancy after every step of dig/place runs). The compare loop now works on 8 ids per word: `x ^ y` gives the changed bytes, and since AIR = 0 the byte-nonzero masks of both words give "solid".
- *Genesis.* Two comparison sorts of 40–48-byte tuples (candidates, then all cells by coordinate) plus Vec growth. The loop index is monotone in the offset, so a 24-byte key sorts in the same order. The final coordinate sort is the loop order itself.
- *Convection.* The test pass, the bit array and the roll pass were three walks; now there is one walk per word, rolling only the passing bits. This order and these values are unchanged (a roll is a pure function of cell and tick).
- simd128 was already on for the wasm build. The remaining density test is a table gather per cell, which simd128 has no gather for, so no explicit SIMD was added.

**3. Timings.**

| measurement | before §6u (de923b0) | after (9e37e52) |
|---|---|---|
| wasm gas r92 step (node, `wasmstep` 30+60, 3 interleaved runs, median) | 58.3–59.6 ms | 55.2–56.3 ms |
| wasm gas r92 init (genesis + setup) | 1176–1198 ms | 802–808 ms |
| wasm mirror_chem self (profile, 40 steps) | 879 samples | 551 samples |
| native gas r92 `ChemPlay::new` (genesis) | 855–860 ms | 561–618 ms |
| native gas r92 first step / steady / convection | 368–370 / 28–29 / 19 ms | 361–437 / 27–29 / 18–19 ms (convection is native-bound by the same memory traffic; the win is wasm-side) |

**4. Chrome cadence (study build of de8b425 with the cap lifted; Low 960×540; each run fresh; mean `presented_fps` of 3 snapshots, first window in brackets).**

| case | set 1 (4 runs) | set 2 (4 runs) |
|---|---|---|
| gas r92 | 15.00 (15), 15.00 (15), 15.10 (15.1), 14.33 (12.7); busy 81–86% | 15.27 (15.7), 15.00 (15), 15.13 (15.1), 15.00 (15); 81–84% |
| ocean r92 | 15.47 (16.4), 14.60 (13.8), 15.60 (16.8), 15.57 (16.8); 83–85% | 15.57, 15.57, 15.53, 15.50; 82–83% |

Gas busy fell from 87–91% (§6t) to 81–86%. **Cap: stays 90.** Set 1 has one gas run with a 12.7 first window (14.33 mean) and one ocean run with a 13.8 first window (14.60), so the rule (both sets, first windows included) does not pass.

Domain-growing place / dig at r90 on :8765 (`placefps6t.mjs`): ocean worst first 0.5 s window 13.4 fps (gap 167 ms), gas 12.7 (gaps 150 / 117 ms), then 15.0; digs 14.8–17.1.

**5. Exactness.**
- After each commit: all 16 `delta_eq` outputs and `fp_long 36 600` are byte-identical to §6r's, 17/17. Parity md5s are unchanged: chem `8ef8f682…`, play `dd849e0b…`, big `9248e8d7…`, split `bc8ac3af…`.
- `wasm_parity` §6t wasm vs §6u wasm: 7/7 SAME. Native `convected` counts per step are identical before and after.
- **Frame hashes: no re-baseline; old = new.** Default world `pmbj7za5…` → `pmbj7za5…`, frame `nx4b62u5…` → `nx4b62u5…`. Gas r92 node: world `vbv6ockz…`, frame `3yu6bcrb…` both builds.

**6. Next wall.**
- Gas convection: still ~18 ms native, ~19 ms wasm. That is a per-cell gather of mass and heat through `cell_up`. A real cut needs a different data layout: parcels kept in rsort order, so the outward neighbour is a small stride.
- The render of gas's many swapped cells (draw_bit_with ~10 ms/step wasm).
- The domain-growing place frame: 117–167 ms gap, from the linear topology passes.

**Build.** wasm (9e37e52, ABI 17) md5 `cfdc1473b4f46486ccf92c65fcd92448` (1,553,418 B), play.html `a00a0ae7cb1939071bbf05fff8f3423d`, bitblock-lab.html `542db2c57e2010dbc21362403503dd8b`, bitblock.html `814320ae5671a4f5c14ccc0015b1e5e7` (unchanged).
- :8765 playtest: default world `pmbj7za5…`, frame `nx4b62u5…`, ABI 17.
  - Ocean r90 15.0 and gas r90 14.7 fps, `books=ok energy_drift=0 error=none`.
  - r94 clamps to 90. Only favicon 404.
- Screenshots of every dig/place were checked by eye; no holes or tearing: `/workspace/render_scale/s6u/{place_2,place_4,playtest}/*.png`.
- Data: `/workspace/render_scale/s6u/{STATUS.md,eq1..eq4,parity1..4.log,wasm_parity_wt.log,node_ab.log,native_fw.log,set1.log,set2.log,place_*.log}`.

Soft DDA, bcc-voxel-demo and abi16 untouched. No Desktop copy, merge, remote deletion, hash or frame change; 8790/8791 left alone.

## 6. Non-goals
- Noise/Perlin/simplex heightmaps, biome maps, cave carvers or cloud textures as an authority over world
  contents (a debug visualisation that reads chem state is fine).
- Soft DDA / bcc-voxel-demo changes.
- Commits, pushes. (`CHEM_RADIUS_MAX` raised 20 → 32 in §5y after the exact network skip; default stays r=12.)
- Conflating LAZY-CHEM (reaction derivation) with spatial sparsity.

## 7. Open risks
- **Conservation across frozen regions**: dormant bricks must keep exact partial books; a wake must not double-
  count; heat flux at an active/dormant boundary must either be exactly zero (proof) or wake the dormant side.
- **Exactness of dormancy**: time-keyed rolls (storm/season windows, evaporation `tick>>3`) can fire later in a
  brick that was idle; the dormancy test must check eligibility, not just last-tick change.
- **Global orderings**: rain restart scan, rise/settle/flow y-order and the claim protocol are index-ordered;
  sparse results will diverge from planet6 parity unless redefined (keep parity mode as the oracle).
- **Vapor/clouds representation**: scalar pool today; spatial vapor changes the energy book (latent heat moves
  with the vapor) and needs an advection law that is itself conservative and integer.
- **Energy input design**: resolved for M1 by making the inbound ledger first-class (`input_q`, see §5a).
- **RNG aliasing**: 16-bit cell/tick roll keys repeat at r≳25 and after 65,536 ticks (Classic only; Driven uses
  the 64-bit `wide_roll`, §5b).
- **Render vs chem domain size**: render already culls to ~800 drawn bits of ~7k at r12; at MC scale render
  needs chunked meshes/LOD independent of the chem active set, and the occupancy mirror (`mirror_chem`)
  must diff only active bricks.
- Timings are native single-thread on a shared box; browser wasm ~1.75× slower; Chrome not checked here.

## 8. Recommended next build step
**Update 2026-11-16 (24):** radius scaling (§6a). The first wall past r32 was chem combine2 growing superlinearly from cache misses. Exact fixes: a precomputed outward slot per rsort entry (`rsort_up`), a boxed `nb6` fast path, and a reused network-law scratch (`NetScratch`). Gas chem at r64 went 206 → 71 ms native. The enclosed cull now applies exactly down to 1.25 px bits (r96 render 453 → 103 ms). Browser gas r64 went 1.37 → 3.37 fps. Play cap 32 → 36: ocean is the binding preset, holding 15 presented fps at r36 and dropping to 13 at r38. Every hash and book is unchanged (chemparity chem/play/big byte-identical, p6 goldens pass). Next wall: ocean SHA-256 world_tag per edit tick in wasm (a protocol decision for Owen), then octree edit upkeep.

**Update 2026-11-16 (23):** render scale (§5z). Two exact levers on branch `chem/render-scale-2026-11`. First, the renderer no longer submits bits whose whole 3×3×3 neighbourhood is on: a 6-face rule was measured and rejected as inexact at concave creases. Second, `world_tag` streams coordinates in order instead of collecting and sorting them. Frame hashes, world tags and chem books match 7634a08 on 13 play scenarios. Chrome Low 960×540: gas r32 raf 30 → 44, ocean r32 9.5 → 36 raf and 9.5 → 15 presented (the full chem cadence); native r32 render 35 → 14 ms. Next, in order: (1) chem inner loops / M2 at r32, since chem ≈ render now; (2) an exact chunk-level occlusion test; (3) Owen's call on an incremental world tag, which is a protocol change.

**Update 2026-10-29 (22):** bigger scale + gas planet (§5y). Exact network skip (`net.rs`, q<250 / non-reactive cells skip the cube build) halves step cost at every radius, parity 9/9 SAME; play radius cap 20 → 32. New opt-in `convect` law (ideal-gas buoyancy swaps, books unchanged) and gas preset 4 (Fe/Ni/Si/Mg core, H2S, NH3 plumes in CH4; sun8 + Driven + core gravity) in play. Chrome: gas r20 ~52 fps raf / 15 presented, r24 ~30, r32 ~6; books ok. The bottleneck is now the CPU renderer (~18 ms at r24, ~36 ms at r32 native 1600×900), not chem. No bands/storms claimed (no rotation). Next, in order: (1) renderer dirty-region repaint (needs OK to touch render); (2) M2 dormant bricks past r32; (3) rotation/Coriolis before any band claim; (4) the §5x list (tilt under core, Full+sun8 soak baseline, multi-seed).

**Update 2026-10-29 (21):** σ_h driver study (§5x) finished. Twelve paired Down vs Core soaks (e8/m1 × ocean/rocky + e8 noerode), books exact. Down σ_h is 1.6–2.2× Core because water still piles under −y (94–100% below core) and between-face height variance (`betwF` ~10 vs ~0) dominates; weather/scour ablation only partly closes the gap. The old high σ_h was mostly a −y artifact — do not tune erosion against it. Core ~2.9–3.7 (ocean) / ~2.3–2.5 (rocky) is the honest baseline. Next, in order: (1) tilt under core gravity to light the ±z caps; (2) Full + sun=8 + core as soak baseline + rocky matched; (3) multi-seed / r16; (4) opt-in play gravity/sweep toggle; (5) SED lithification / M2.

**Update 2026-10-28 (20):** coreward gravity measured (§5w, `Gravity::Core`, now the Driven default; `chem_soak grav=`, books exact, `grav=down` bit-identical to §5v). Driven flow and settle now fall toward the core (inward support set, sideways only to cells no more than one r² unit higher, inner cells first, pure swaps), gas rises outward, and the outward guard is auto-off. The ocean no longer pools below the core: 27–43% of water at y < core vs 91–100% in §5u/§5v, and evaporation/rain split ~even (evapL 34–49% vs 99.8%). The 0→5k upper-hemisphere loss is now evaporation, not flow (residual flow ≈ 0 vs 1,648 cells in fs8). Shed stays 0 on every H4/H8 run with the guard off, and a guard-forced A/B gives the same containment (more wet regions on one seed: 10 vs 1). Not fixed: at sun 16 the ocean still dries to one wet region (energy, not gravity); the matched `sun=8` run (`cge8`) is healthiest (water 518, 7 wet regions, ice ≈ 0, flat vapor). New issues: σ_h roughly halves (ocean cg8 3.71 vs fs8 6.02, cge8 2.93 vs fe8 5.55; rocky 2.48 vs 5.35), leftover water gathers in the unlit equinox ±z caps (74–86%), and play's Driven mode is coreward by default. Next, in order: (1) Full + `sun=8` + core gravity as the soak baseline, plus a rocky matched run; (2) tilt under core gravity to light the ±z caps; (3) multi-seed / r16 of cg8 / cgg8 / cge8; (4) a σ_h driver study (Down vs Core); (5) an opt-in play gravity/sweep toggle; (6) mix τ / radial degassing / SED lithification / M2.

**Update 2026-10-28 (19):** full-sphere sun measured (§5v, opt-in `SunSweep::Full`, `chem_soak sweep=`, books exact, Upper default bit-identical). The sun never sets, so the `−y` underside gets a day. Night-side ice is gone at sun 16 and much reduced at matched energy (`sun=8`: ice below core 332 → 42), and the underside ocean now evaporates (99.8% of evap from y < core). Shed stays 0 with the radial shell. But the water still sits below the core (329 of 363 at H8, 818 of 860 matched): a 0→5k budget shows flow under world −y gravity, not night, emptied the upper hemisphere (the Upper control does the same). So §5u's cold-trap diagnosis was mostly gravity. Full at the same `sun` doubles input and dries the ocean (water 825 → 363, vapor 1,298 → 2,054, H8 wet/dry 21× → 1.05×). The matched run keeps contrast (9.9×) and still trends like rs8. Tilt (410, 8-day year) changes little. Next, in order: (1) radial gravity for flow/settle so water stops pooling at world −y and the guard can go; (2) use Full + `sun=8` for future soaks, plus a rocky matched run; (3) multi-seed / r16 and a longer matched ocean H8; (4) an opt-in play sweep toggle; (5) mix τ / radial degassing / SED lithification / M2.

**Update 2026-10-28 (18):** full radial sky shell measured (§5u, opt-in `skyshape=radial`, Driven, books exact). AIR now fills every cell with R² < |c−centre|² ≤ (R+H)², and an outward guard keeps flow/settle/scour/dissolve/gas rise out of radial-only shell cells. Shed falls to ~0 (ocean 27–31% → 1.05% at H4 / 0% at H8; rocky 11–23% → 0%), spill stays 0, and polar/H0 runs are bit-identical to §5t. Ocean wet/dry contrast is much stronger (per-rim cloud 3.4× → 7.3× / 21×) and ocean σ_h rises (3.05 → 4.20 / 4.98). But the ocean migrates to the never-lit y < core hemisphere within 5k ticks (water below core 1,189 of 1,195 at H4, 788 of 788 at H8), because the sun only shines with sy > 0 and rain can now land on the underside: a night-side cold trap. Rocky contrast collapses (44× → 0.5× / 2.4×) and rocky evap decays (H4 8.8k → 2.7k, ice 20 → 91); ocean H8 vapor and evap are still climbing at 300k. Next, in order: (1) light the whole sphere (sun sweep / rotation axis or tilt) so the underside gets day; (2) radial gravity for flow/settle so the guard is unnecessary; (3) multi-seed / r16 and a longer ocean H8; (4) a mix τ sweep; (5) radial degassing; (6) SED lithification / M2.

**Update 2026-10-28 (17):** radial evaporation shipped (§5t, Driven only, books exact). A WATER cell evaporates when its outward step from the core is space or not WATER, which is the old `+y` test in rain's core frame. `EvapTop::Up` keeps §5s bit-identical, and a strict AIR-only variant (`CoreAir`) was measured and not shipped (evap −16 to −44%). Ocean evap recovers (H0 128k → 137k, above §5r's 132k; H8 80k → 85k). Rocky evap does not (H0 −4%, H8 −16%). Ocean H0 wet/dry contrast is weaker (per-rim 2.5× → 2.2×) while ocean H8 is stronger (2.1× → 3.4×). Spill stays 0, but shed rises on the oceans (23 → 31%, 17 → 27%). No runaway. Next, in order: (1) a full radial sky shell so the shed falls toward 0; (2) multi-seed / r16 soaks of the §5r–§5t stack; (3) a mix τ sweep; (4) radial degassing; (5) sun-side advection; (6) SED lithification / M2.

**Update 2026-10-28 (16):** rain toward the core shipped (§5s, Driven local vapor only, books exact). Rain lands where the outward step from the core is AIR or space over solid ground, so flanks and the underside catch their own rain. A region with no landing ground sheds its drop to the nearest region with ground, 1.1–1.6 hops away (`rain_shed`). The planet-wide spill is now 0 on all four r12 soaks, down from 9–23% of rain, but 11–23% of rain is shed locally instead. Oceans stay cloud-cooled and a bit cooler (ice 184 → 201 on H0, 365 → 408 on H8). Rocky dry ground is clearer (H8 dry cloud 0.16 → 0.08%). The wet/dry contrast is stronger on ocean H8 (per-rim 1.5× → 2.1×) but weaker on ocean H0 (3.4× → 2.5×). Evap is 3–22% slower, likely because evaporation still uses world +y tops. No runaway. Next, in order: (1) radial evaporation so flank puddles cycle; (2) a full radial sky shell so the shed falls toward 0; (3) a mix τ sweep; (4) multi-seed / r16 soaks; (5) sun-side advection; (6) SED lithification / M2.

**Update 2026-10-28 (15):** local vapor shipped (§5r, Driven only, books exact). The one booked `vapor` count is now attributed to 54 cube-sphere regions (n = 3, adjacent diffusion τ 2048, per-region rain `bin/512`), and cloud and greenhouse read the per-region column. A first soak showed underside regions stranding vapor (water cycle about 3.7× slower), so drops a region can't land now spill to the global landing line (`rain_spilled`, diagnostic). With that fix, evap is within 7–11% of §5q. Oceans stay cloud-cooled: cloud 6.18% / 2.90% of the sun vs 5.90% / 3.05%, ocean H0 ice 143 → 184. Over water/ice, cloud is 10.6% vs 5.0% elsewhere (ocean H0), and rocky dry ground is near-clear (0.58% / 0.16%). The rocky H0 total cloud/trap rose slightly (0.42 → 0.71%, 0.83 → 1.29%) as vapor piles over its puddles. The wet/dry split is real but moderate (ocean H8 per-rim 33 vs 21), which is classification and area, not a bug. No runaway. Next, in order: (1) a radial rain fall test to drop the spill; (2) multi-seed / r16 soaks of the full stack; (3) sun-side advection; (4) lithification of SED; (5) M2.

**Update 2026-10-28 (14):** cloud albedo from the vapor pool shipped (§5q, Driven + greenhouse only, books exact). Each sun ray first loses `cloud = 256·c/(c + 4096)` /1024 (c = vapor·1024/rim) to space, and then the surface albedo applies; reflected light is never injected. Ocean ice: H0 111 → 143, H8 303 → 349. Water-top Q: H0 961 → 852, H8 667 → 629. That lands near the §5o greenhouse baseline (138 / 930, 381 / 637), so the Fresnel warming is offset. Cloud reflects 5.9% (H0) and 3.1% (H8) of the ocean sun and under 0.5% on rocky. No runaway: ice and vapor flatten by 100–150k, and even max 512 stays bounded. Rocky H0 is unchanged and rocky H8 σ_h 3.64 → 3.89 (single seed). Next, in order: (1) multi-seed / r16 soaks of the full albedo + greenhouse + water-angle + cloud stack; (2) local (per-region) vapor so clouds sit over oceans; (3) lithification of SED; (4) M2.

**Update 2026-10-27 (13):** angle-dependent water albedo shipped (§5p, Driven only, books exact). Water now follows a Schlick-style curve, 0.06 at high sun rising to 1.0 at grazing (nadir 61, graze 1024, pow 5); ice stays 0.60; reflected light is never injected. In soaks the high-sun bin is 0.061 and the low-sun bin 0.23, with a flux-weighted mean of 0.08 vs 0.15. Ocean ice: H0 138 → 111, H8 381 → 303. Evap rises 7–10%. Ocean H8 σ_h 2.62 → 2.82. No runaway, and rocky is mostly unchanged. A mean-preserving variant (113, 1024, 3) lands back on §5o, so the warming comes from the lower mean, not the angle shape. Next, in order: (1) cloud albedo from the vapor pool, which also offsets the extra warming; (2) multi-seed / r16 soaks of albedo + greenhouse + water angle; (3) lithification of SED; (4) M2.

**Update 2026-10-27 (12):** vapor greenhouse shipped (§5o, Driven only, books exact). The vapor pool holds back up to 0.50 of outbound IR (`GH_VAPOR_HALF = 4096`) and back-radiates it onto the ground. Ocean H0 ice 188 → 138 (pre-albedo 134), ocean H8 ice 425 → 381 and water-top Q 594 → 637. No runaway: vapor and trap% plateau by 100k. Rocky trap < 1%, but rocky H8 σ_h 3.78 → 3.57 (single seed). Next, in order: (1) cloud albedo from the vapor pool; (2) multi-seed H8/H16 soaks of albedo + greenhouse; (3) lithification of SED; (4) M2.

**Update 2026-10-27 (11):** albedo shipped (§5n, Driven only, books exact). ICE reflects 0.60 and WATER 0.15 of the arriving sun; the reflected share is never injected. Ocean worlds: ice cells +40–52%, evap −10–16%, water-top Q −11–15%, ocean H8 σ_h 2.60 → 2.83; rocky H8 3.90 → 3.78 (single seed); 100k ceiling stays 0.

**Update 2026-10-27 (10):** whole-column plume shipped (§5m, Driven only, books exact). The cold column now subsides
and the hot column rises as blocks, guarded by `PLUME_HEADROOM = 1`, a flank gate and `PLUME_REPOSE = 4`. Effects:
- Rocky H8 σ_h goes 2.34 → 3.90 and land_r_std 1.56 → 2.81. Ocean H8 σ_h goes 2.17 → 2.60.
- The 100k ceiling count stays 0.
- By 300k, 3 rim columns hit the probe ceiling. They are pure SED stacks on the equatorial ledge, not lifted rock.

Next, in order:
1. Lithification of SED (or an angle-of-repose rule for SED / radial settle gravity), which also cures the rim ledge.
2. Multi-seed H ≥ 16 soaks (σ is still creeping at 300k; subsolar basin).
3. Albedo.
4. M2.

**Update 2026-10-27 (9):** transparent-air sun shipped (§5l). Next: relax uplift/plume caps or whole-column plume
lift (done in §5m), then lithification, multi-seed H ≥ 16, M2.

**Update 2026-10-27 (8):** void collapse shipped (§5k, Driven only, books exact): unsupported rock drops toward the
core into dry AIR. Raised land compacts (rocky H8 max_h 7.5 → 6.6, dome 0.65 → 0.08, ceiling 1 → 0) but σ_h does not
rise (1.97 → 1.99), so the "foam" hypothesis is largely discarded. Next, in order:
1. Transparent-air sun.
2. A stronger lateral driver: relax uplift/plume caps or whole-column (isostatic) plume lift.
3. Lithification of SED.
4. Multi-seed H ≥ 16 soaks.
5. M2.

**Update 2026-10-27 (7):** radial-frame uplift and plume shipped (§5i, Driven only; `radial_peer` +
`heated_from_below`, `RADIAL_PEER_SPAN = 8`). The changes:
- The dome is gone: rocky dome mean drops from 7.86 / 10.43 to 0.57 / 0.39 at 100k (H8 / H16), and ocean is off the ceiling.
- Rim-at-cap is mostly gone: rocky H8 has 1 column at 100k and 9 at 300k, against 25 and 50 for the scratch guard.
  Rocky H16 has 0 (max_h 13.8).
- Books are exact and the Classic goldens are unchanged.
- σ_h is lower (rocky 2.6–2.9, ocean ~2.2), because only lateral heat contrasts drive now.

Next, in order:
1. Void-collapse / rock-fall law.
2. Transparent-air sun.
3. Finer outward test if the H8 rim persists.
4. Multi-seed H ≥ 16 soaks.
5. M2.

**Update 2026-10-27 (6):** core sphere built (§5h, opt-in `core_r`, exact split, books exact) but it does **not** unpin
the dome: the pin comes from uplift/plume comparing same-y roots on a ball across a frozen radial heat staircase
(diffusion dead band ≈120 Q). A scratch radial guard removes the dome (rocky 100k dome 7.86 → 0.16) but moves the
pin to the rim. Next, in order: (a) radial-frame uplift/plume (compare along the local outward normal, or an
overburden limit) — Owen's pick, changes Driven; (b) transparent-air sun; (c) void-collapse / rock-fall law; then
multi-seed H ≥ 16 soaks and M2.

**Update 2026-10-27 (5):** the headroom shell shipped (§5g, opt-in `add_sky_shell`/`genesis_sky`, `chem_soak sky=H`)
with an outer-relief σ_h. Old σ was void depth (rocky outer surface flat to σ_h 0.37 unpadded); with H=8/16 rocky σ_h
reaches 5.85/7.07 at 300k, ocean ~4.5, books exact. Next, in order: (a) transparent-air sun (absorb at the first
non-AIR cell of the column) so the shell stops starving the water cycle; (b) spread M1 core input over a core sphere
(or limit uplift by overburden) so the hotspot dome stops pinning max_h at H; (c) a void-collapse / unsupported-rock
fall law so raised land compacts instead of foaming; then multi-seed soaks with H ≥ 16, then M2 on padded states.

**Update 2026-10-27 (4):** a longer-range driver shipped (§5e, `plumed`): deep convective overturn, cold roots draw
surface fluid down, hot columns rise 2–6 cells away. Late σ rocky 0.81 → ~1.40 (250–300k), ocean 1.39 → 1.66 at
50–150k but decaying to 1.50 by 300k as air converts to buried water. Next: an atmosphere headroom shell (M3-lite: a
space/air shell above the ball, so σ can measure outer relief instead of void depth), or a balancing degassing law
(hot buried water → vapor returning up) to stop the ocean air drain; then M2.

**Update 2026-10-27 (3):** the §5b follow-up shipped (§5c): dry weathering, scour and heat-driven uplift keep
Driven relief changing to 100k (σ ocean 1.21 → 1.39, rocky 0.375 → 0.711, still rising), books exact. Next: a 300k
soak to find saturation, a surface-only scan for the relief laws, then M2 on driven+relief soaked states.

**Update 2026-10-27 (2):** the vapor-driven water cycle shipped behind `WaterCycle::Driven` (§5b). Water cycle
unscripted; relief still stalls from sediment armour and no uplift. Next: the §5b minimal follow-up (dry
thermal weathering + scour, then uplift), and/or M2 using driven+M1 soaked states as exactness fixtures
(Driven's per-cell 64-bit rolls are also friendlier to partitioning than Classic's index-ordered rain scan).

**Update 2026-10-27:** the M1 energy input shipped (§5a). Next: make rain/erosion vapor-driven instead of calendar-driven (or go to M2 using M1's soaked states as fixtures).

*Original recommendation:* **M1: longer-soak emergence demo at r12–r16, with a param-gated booked energy input.** Reason: the soak shows
relief emerges and then the world freezes by ~tick 10k. Scaling a world that dies in 10k ticks only gives a
bigger frozen ball, and the sparse-active scaffold (M2) is easiest to validate against a world that keeps
moving in some places and sits still in others. M1 is cheap (headless, current dense path, no play UX change)
and it is the first point where "terrain from time" can be shown as a before/after cross-section. M2 follows
directly, using M1's soaked states as its exactness fixtures.

## 8b. Play opt-in: energy M1 + Driven water cycle (2026-10-27, uncommitted; Owen pick t221s1369)

Esc menu → chem options → **dynamics**: `energy` (off | M1 sun+core) and `water cycle` (classic | driven), next
to the cross-section cut. Defaults stay planet6 (off / classic), so default play is unchanged and its status text is
byte-identical. The choice applies live, and the client re-applies it on reseed, radius and custom-budget rebuilds
(the same pattern as the cut). Non-default settings add ` energy=m1` / ` cycle=driven` to the status before
`cut=`. Gallery and non-chem worlds ignore the setting. Wasm ABI stays **17** (additive):
`bitblock_play_chem_dynamics(energy, cycle) -> 1|0` and `bitblock_play_chem_dynamics_get() -> 1|e<<1|c<<2`
(0 outside chem). www rebuilt: wasm 1,287,220 B md5 `6effdafe939a9454015550198d8e9079`, play.html md5
`713841e74280d1796f9506825576cba1`. Smoke: `/workspace/dyn_measure/dyn_page_smoke.mjs`.

## 9. Files
- Bigger scale + gas planet (§5y): `/workspace/scale_measure/gas_scale/` (`bench_{pre,postskip}_sky6.txt`, `classic_play_postskip.txt`, `gas_bench_sky0.txt`, parity `run_parity.sh` + `{pre,post,post2}/`, `core_tests_post_{skip,convect}.txt`), bins `play_frame_bench`, `mirror_prof`, `play_stage_prof`, `wasm_frame_bench.mjs`; code in `chem/net.rs` (`is_reactive`, skip), `chem/p6.rs` (`convect`, `StepReport.convected`, `step_profiled_fine`), `chem/p6_rest.rs` (`convect_pass`, `CONVECT_*`, `LANE_MASS`), `chem/tests_p6.rs`, wasm `play_chem.rs` (preset 4, `CHEM_ENERGY_SUN8`, cap 32), `play.rs` (incremental `mirror_chem`), `lib.rs`, `pages/play.template.html`, `pages/play_client.js`; screenshots `gas_r20_play.png`, `gas_r20_xhalf.png` (this dir); backups `/workspace/backups_gas_scale/{pre,post}/`
- Coreward gravity (§5w): `/workspace/scale_measure/core_grav/` (soaks cg H0/H4/H8, cgg8 guard A/B, cgp8 polar, cge8 matched, idchk down, `smoke.txt`, `run_soaks.sh`, `make_summary.sh`, `hemi.py`, `flow.py`, `traj.py`, `summarize_rc.py`, `blocks_summary.txt`, `bin/chem_soak_cg`, tests); code in `chem/vapor.rs` (`Gravity`), `chem.rs` (`coreward`, guard auto), `chem/p6.rs`, `chem/p6_rest.rs` (`inward_target`, `side_target`, `settle_core`, `flow_core`), `chem/tests_p6.rs`; `chem_soak` arg 15 `grav=`, skyshape `radialguard`; backups `/workspace/backups_core_grav/{pre,post}/`
- Full-sphere sun (§5v): `/workspace/scale_measure/full_sun/` (soaks fs H0/H4/H8, fe8, ft8, idchk upper, `smoke_full.txt`, `make_summary.sh`, `hemi.py`, `flow.py`, `traj.py`, `summarize_rc.py`, `blocks_summary.txt`, `bin/chem_soak_fs`, tests); code in `chem/p6.rs` (`SunSweep`, `sun_dir3`, `track_mask`), `chem/p6_rest.rs` (`weather_pass`), `chem.rs` (`hemi_diag`), `chem/tests_p6.rs`; `chem_soak` arg 14 `sweep=`; backups `/workspace/backups_full_sun/{pre,post}/`
- Radial sky shell (§5u): `/workspace/scale_measure/radial_sky/` (soaks rs H4/H8, idchk, radialfree A/B, `discarded_v1/`, `make_summary.sh`, `summarize_rc.py`, `traj.py`, `blocks5.py`, `blocks_summary.txt`, `bin/chem_soak_rs`, tests); code in `chem.rs` (`add_radial_sky_shell`, `genesis_sky_radial`, `shell_blocked`, `set_outward_guard`), `chem/p6.rs`, `chem/p6_rest.rs`, `chem/tests_p6.rs`; `chem_soak` arg 13 `skyshape=`; backups `/workspace/backups_radial_sky/{pre,post}/`
- Radial evaporation (§5t): `/workspace/scale_measure/radial_evap/` (soaks re/reair, `ab/`, `summarize_rc.py`, `traj.py`, `blocks5.py`, `blocks_summary.txt`, `bin/chem_soak_re`, `bin/evap_probe`, tests); code in `chem/vapor.rs` (`EvapTop`), `chem/p6.rs`, `chem/p6_rest.rs` (`evap_frame`, `evap_open`), `chem.rs`, `chem/tests_p6.rs`, `wasm/src/play_chem.rs` (test); `chem_soak` arg 12 `evap=`; probe `src/bin/evap_probe.rs`; backups `/workspace/backups_radial_evap/{pre,post}/`
- Rain toward the core (§5s): `/workspace/scale_measure/rain_core/` (soaks rc/rcg/rct8k, `ab/`, `summarize_rc.py`, `blocks5.py`, `blocks_summary.txt`, `bin/chem_soak_rc`, tests); code in `chem/vapor.rs`, `chem/p6.rs`, `chem/p6_rest.rs`, `chem.rs`, `chem/tests_p6.rs`, `wasm/src/play_chem.rs` (test); `chem_soak` arg 11 `rain=`; backups `/workspace/backups_rain_core/{pre,post}/`
- Local vapor (§5r): `/workspace/scale_measure/local_vapor/` (soaks lv v2, `v1_stuck/`, `ab/`, `summarize_lv.py`, `blocks5.py`, `blocks_summary.txt`, `bin/chem_soak_lv2`, tests); code in `chem/vapor.rs` (new), `chem/p6.rs`, `chem/p6_rest.rs`, `chem.rs`, `chem/tests_p6.rs`; `chem_soak` arg 10 `vloc=`; backups `/workspace/backups_local_vapor/{pre,post,post2}/`
- Cloud albedo (§5q): `/workspace/scale_measure/cloud_albedo/` (soaks ca, sweep ca256/384/512 + ctrl_off, `blocks5.py`, `summarize.py`, `blocks_summary.txt`, `blocks_full.txt`, `bin/chem_soak_ca`, tests); code in `chem/p6.rs`, `chem.rs`, `chem/tests_p6.rs`; `chem_soak` arg 9 `cloud=`; backups `/workspace/backups_cloud_albedo/{pre,post}/`
- Water-angle albedo (§5p): `/workspace/scale_measure/water_albedo/` (soaks wa/mp/fixed, `blocks4.py`, `blocks_summary.txt`, `blocks_full.txt`, `bin/chem_soak_wa`, tests); code in `chem/p6.rs`, `chem.rs`, `chem/tests_p6.rs`; `chem_soak` arg 8 `walb=`; backups `/workspace/backups_water_albedo/{pre,post}/`
- Vapor greenhouse (§5o): `/workspace/scale_measure/greenhouse/` (soaks gh, probes, tune/tune2, `blocks3.py`, `blocks_summary.txt`, tests); code in `chem/p6.rs`, `chem.rs`, `chem/tests_p6.rs`; backups `/workspace/gameladder_chem_findings/backups_greenhouse/{pre,post}/`
- Albedo (§5n): `/workspace/scale_measure/albedo/` (soaks al/bl, probes, `blocks2.py`, `blocks_summary.txt`, tests); code in `chem/p6.rs`, `chem.rs`, `chem/tests_p6.rs`; backups `/workspace/gameladder_chem_findings/backups_albedo/{pre,post}/`
- `/workspace/scale_measure/src/bin/chem_scale_bench.rs`, `src/bin/chem_soak.rs`, `run_curve.sh`, `README.md`
- raw: `raw_small_run1.txt`, `raw_large_run1.txt`, `raw_full_run2.txt`, `curve_run1.csv`, `curve_run2.csv`,
  `soak_r12_rocky.txt`, `soak_r12_ocean.txt`
- M1: `/workspace/scale_measure/m1/soak_r12_{rocky,ocean}_{off,m1}.txt`, `soak_r16_{rocky,ocean}_m1.txt`;
  code in `/workspace/bitblock/crates/core/src/chem.rs`, `chem/p6.rs`, `chem/tests_p6.rs` (uncommitted);
  pre-edit backups in `/workspace/gameladder_chem_findings/backups_m1_energy/`
- Unscripted cycle: `/workspace/scale_measure/unscript/soak_r12_{ocean,rocky}_m1_{classic,driven}.txt`,
  `soak_r12_{ocean,rocky}_off_driven.txt`; `chem_soak` arg 6 = `classic|driven`, prints `entrained` and
  `rain_footprint_cells`; code in `chem.rs`, `chem/p6.rs`, `chem/p6_rest.rs`, `chem/net.rs`,
  `chem/tests_p6.rs` (uncommitted); backups in `/workspace/gameladder_chem_findings/backups_unscript_cycle/`
- Relief laws: `/workspace/scale_measure/relief/soak_r12_{ocean,rocky}_m1_driven_relief.txt`,
  `soak_r12_ocean_m1_classic_check.txt`; `chem_soak` prints `weathered,scoured,uplifted` and `bare_rock`; tuning aid
  `src/bin/relief_probe.rs`; code in `chem/p6.rs` (StepReport, Driven order), `chem/p6_rest.rs` (the three passes and
  constants), `chem/tests_p6.rs` (uncommitted); backups in `/workspace/gameladder_chem_findings/backups_relief_laws/`
- Play opt-in (§8b): `crates/wasm/src/{play_chem.rs,play.rs,lib.rs}`, `crates/wasm/pages/{play_client.js,play.template.html}`,
  rebuilt `crates/wasm/www/`; smokes + pre/post outputs in `/workspace/dyn_measure/`; backups in
  `/workspace/gameladder_chem_findings/backups_play_dynamics/`
- Relief driver (§5e): `/workspace/scale_measure/relief_driver/soak_r12_{ocean,rocky}_m1_driven_plume_300k.txt`,
  `probe_hist_{ocean,rocky}_100k.txt`, `blocks.py` (block means + book check), `sig.sh`, `trials/` (failed variants
  A–G hot-root plume, M/N overturn, baseline histograms), `tests_core_wasm.txt`; probe `src/bin/plume_probe.rs`;
  code in `chem/p6.rs` (StepReport.plumed, Driven order), `chem/p6_rest.rs` (`plume_pass` + constants),
  `chem/tests_p6.rs` (`driven_plume_builds_broad_relief_by_permutation`); backups in
  `/workspace/gameladder_chem_findings/backups_relief_driver/`
- Degassing (§5f): `/workspace/scale_measure/relief_driver/degas/soak_r12_{ocean,rocky}_m1_driven_degas_300k.txt`,
  `t1_ocean_30k.txt`, `water_probe_{ocean_plume_200k,rocky_plume_100k}.txt`, `cmp.sh`, `blocks.sh`, `tests_chem.txt`,
  `tests_core_wasm.txt`; probe `src/bin/water_probe.rs`; code in `chem/p6.rs` (StepReport.degassed, Driven order),
  `chem/p6_rest.rs` (`degas_pass` + constants), `chem/tests_p6.rs`; backups in
  `/workspace/gameladder_chem_findings/backups_degas/{pre,post}/`
- Sky shell (§5g): `/workspace/scale_measure/sky_shell/soak_r12_{ocean,rocky}_m1_driven_sky{0,8,16}_300k.txt`,
  `blocks.py`, `blocks_summary.txt`, `probe_{ocean,rocky}_sky{0,8,16}_100k.txt`, `probe_{ocean,rocky}_sky8_30k.txt`;
  `chem_soak` arg 7 `sky=H` + `relief:` line; probe `src/bin/sky_probe.rs`; code in `chem.rs` (`sky_height`, `core_pin`,
  `add_sky_shell`, `land_relief`, `LandRelief`, `genesis_sky`), `chem/p6.rs` (`topology()` honours `core_pin`),
  `chem/p6_rest.rs` (`weather_pass` sky-open lit gate), `chem/tests_p6.rs` (4 `sky_*` tests); backups in
  `/workspace/gameladder_chem_findings/backups_sky_shell/{pre,post}/`
- Core sphere (§5h): `/workspace/scale_measure/core_sphere/` (tests, `sweep/probe_*_cr{0,3,6,9}_30k.txt`,
  `probe_{rocky,ocean}_sky{8,16}_cr*_100k.txt`, `soak_r12_{rocky,ocean}_m1r6_driven_sky{8,16}_300k.txt`,
  `blocks_summary.txt`, `radial_guard_experiment.diff`, `radexp/` scratch outputs); code in `chem/p6.rs`
  (`EnergyInput.core_r`, `CORE_R_SPREAD`, `with_core_r`, `core_sphere`, `core_sphere_input`), `chem/tests_p6.rs`
  (3 tests); `chem_soak` `core_r=`/`m1rN`, `sky_probe` arg 5; backups in
  `/workspace/gameladder_chem_findings/backups_core_sphere/{pre,post}/`
- Radial frame (§5i): `/workspace/scale_measure/radial_frame/`:
  - v2: `probe_{rocky,ocean}_sky{8,16}_100k.txt`, `probe_rocky_sky{8,16}_300k.txt`,
    `v2soak_r12_{rocky,ocean}_m1_driven_sky{0,8,16}_300k.txt`, `blocks_summary.txt`
  - scratch guard at 300k: `radexp_probe_rocky_sky8_300k.txt`
  - v1 iteration (binaries, probes, soaks) in `iter/`
  - tests: `tests/tests_chem*.txt`, `tests/tests_core_wasm_*.txt`
  - code in `chem/p6_rest.rs` (`RADIAL_PEER_SPAN`, `radial_peer`, `heated_from_below`, uplift/plume gates) and
    `chem/tests_p6.rs` (`relief_plate_deep`, `radial_ball`, `polar_rise`, 2 new tests + 1 re-fixtured)
  - www rebuilt (wasm 1,302,349 B md5 `b360b2060c55a9d348d187bef03dd883`)
  - backups in `/workspace/gameladder_chem_findings/backups_radial_frame/{pre,post}/`
- Void collapse (§5k): `/workspace/scale_measure/void_collapse/` (soaks, probes, `blocks.py`, `blocks_summary.txt`,
  `tests/`); code in `chem/p6_rest.rs` (`inward_support`, `collapse_pass`), `chem/p6.rs`, `chem/tests_p6.rs`; backups in
  `/workspace/gameladder_chem_findings/backups_void_collapse/{pre,post}/`
- Transparent-air sun (§5l): `/workspace/scale_measure/sun_air/` (soaks, probes, `blocks.py`, `blocks_summary.txt`,
  `tests/`); code in `chem/p6.rs` (`energy_input_pass` Driven branch, `sun_target`), `chem/tests_p6.rs`; backups in
  `/workspace/gameladder_chem_findings/backups_sun_air/{pre,post}/`
- Whole-column plume (§5m): `/workspace/scale_measure/uplift_caps/` (`blocks_summary.txt`, `exp/soaks/`, `exp/probes/`
  incl. `ship_rocky_sky8_{100000,300000}.txt`, knob scratch tree `exp/bitblock` + `exp/sm`, `equiv/`, `tests/`); code in
  `chem/p6_rest.rs` (`PLUME_HEADROOM`, `PLUME_REPOSE`, `plume_pass`, `within_repose`, `col_top_near`), `chem/tests_p6.rs`;
  backups in `/workspace/gameladder_chem_findings/backups_uplift_caps/{pre,post}/`
