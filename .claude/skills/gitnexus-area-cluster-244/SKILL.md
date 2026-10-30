---
name: gitnexus-area-cluster-244
description: "Skill for the Cluster_244 area of bitblock. 23 symbols across 1 files."
---

# Cluster_244

23 symbols | 1 files | Cohesion: 74%

## When to Use

- Working with code in `crates/`
- Understanding how bench_chem, chem_queue, chem_set_auto work
- Modifying cluster_244-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/wasm/src/play.rs` | bench_chem, chem_queue, chem_set_auto, chem_status, key (+18) |

## Entry Points

Start here when exploring this area:

- **`bench_chem`** (Function) — `crates/wasm/src/play.rs:1320`
- **`chem_queue`** (Function) — `crates/wasm/src/play.rs:347`
- **`chem_set_auto`** (Function) — `crates/wasm/src/play.rs:276`
- **`chem_status`** (Function) — `crates/wasm/src/play.rs:271`
- **`key`** (Function) — `crates/wasm/src/play.rs:552`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `bench_chem` | Function | `crates/wasm/src/play.rs` | 1320 |
| `chem_queue` | Function | `crates/wasm/src/play.rs` | 347 |
| `chem_set_auto` | Function | `crates/wasm/src/play.rs` | 276 |
| `chem_status` | Function | `crates/wasm/src/play.rs` | 271 |
| `key` | Function | `crates/wasm/src/play.rs` | 552 |
| `pointer_button` | Function | `crates/wasm/src/play.rs` | 638 |
| `set_capture` | Function | `crates/wasm/src/play.rs` | 541 |
| `step` | Function | `crates/wasm/src/play.rs` | 686 |
| `with_chem` | Function | `crates/wasm/src/play.rs` | 238 |
| `with_chem_budget` | Function | `crates/wasm/src/play.rs` | 245 |
| `assert_mirrors_chem` | Function | `crates/wasm/src/play.rs` | 1887 |
| `chem_auto_steps_advance_tick_and_keep_books` | Function | `crates/wasm/src/play.rs` | 1927 |
| `chem_startup` | Function | `crates/wasm/src/play.rs` | 1880 |
| `chem_status_reports_lanes_drift_and_bag` | Function | `crates/wasm/src/play.rs` | 2288 |
| `cross_section_dig_and_place_keep_books_and_chem_ticks` | Function | `crates/wasm/src/play.rs` | 2139 |
| `cross_section_reveals_core_bands_on_rocky_and_ocean` | Function | `crates/wasm/src/play.rs` | 2083 |
| `custom_budget_matches_preset_and_spends_entered_lanes` | Function | `crates/wasm/src/play.rs` | 2221 |
| `custom_budget_rejects_negative_and_oversized_lanes` | Function | `crates/wasm/src/play.rs` | 2264 |
| `dig_and_place_go_through_chem` | Function | `crates/wasm/src/play.rs` | 1955 |
| `every_preset_genesis_is_visible_and_balanced` | Function | `crates/wasm/src/play.rs` | 1901 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Cross_section_dig_and_place_keep_books_and_chem_ticks → New` | cross_community | 9 |
| `Dig_and_place_go_through_chem → New` | cross_community | 9 |
| `Cross_section_dig_and_place_keep_books_and_chem_ticks → CameraBasis` | cross_community | 8 |
| `Cross_section_dig_and_place_keep_books_and_chem_ticks → Is_finite` | cross_community | 8 |
| `Cross_section_dig_and_place_keep_books_and_chem_ticks → ProjectedPoint` | cross_community | 8 |
| `Dig_and_place_go_through_chem → CameraBasis` | cross_community | 8 |
| `Dig_and_place_go_through_chem → Is_finite` | cross_community | 8 |
| `Dig_and_place_go_through_chem → ProjectedPoint` | cross_community | 8 |
| `Cross_section_dig_and_place_keep_books_and_chem_ticks → Viewport_focal_length` | cross_community | 7 |
| `Cross_section_dig_and_place_keep_books_and_chem_ticks → CameraProjection` | cross_community | 7 |

## How to Explore

1. `context({name: "bench_chem"})` — see callers and callees
2. `query({search_query: "cluster_244"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
