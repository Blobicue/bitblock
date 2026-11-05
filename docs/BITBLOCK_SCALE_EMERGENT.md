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
