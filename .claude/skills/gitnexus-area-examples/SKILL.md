---
name: gitnexus-area-examples
description: "Skill for the Examples area of bitblock. 246 symbols across 27 files."
---

# Examples

246 symbols | 27 files | Cohesion: 73%

## When to Use

- Working with code in `crates/`
- Understanding how bounds_fit_one_pixel, position, set_observation_anchor work
- Modifying examples-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/core/src/render.rs` | inspect, changed_rows, clear_frame, draw_crosshair, frame_serial (+49) |
| `crates/core/src/history.rs` | circulating_supply, current_world, edits, active_mint, begin_tick (+35) |
| `crates/wasm/src/lib.rs` | bitblock_program_run, bitblock_program_symbol, count_non_background, gallery_camera, program_bytes_status (+31) |
| `crates/wasm/src/play.rs` | audit_game_palette, bench_chem_stage, bench_stage, convert_rgba, fit_world (+21) |
| `src/main.rs` | fit_world, fixed_tick, pointer_in_sidebar, resize, sidebar_snapshot (+16) |
| `crates/core/src/sidebar.rs` | compose_sidebar_only, compose_unbound, height, pixels, sidebar_width (+5) |
| `crates/core/src/world.rs` | bounds, is_empty, allocated_chunk_count, iter_on, len (+2) |
| `crates/core/src/camera.rs` | bounds_fit_one_pixel, position, set_observation_anchor, length, try_normalized (+1) |
| `crates/research/src/history_store.rs` | append_tick, discard_unverified_tail, initial_replay_usage, replay_history_records_with_limits, append_receipt (+1) |
| `crates/research/src/optimization.rs` | compare_histories, replay, compositor_snapshot, execute, execute_compositor_arm (+1) |

## Entry Points

Start here when exploring this area:

- **`bounds_fit_one_pixel`** (Function) — `crates/core/src/camera.rs:616`
- **`position`** (Function) — `crates/core/src/camera.rs:338`
- **`set_observation_anchor`** (Function) — `crates/core/src/camera.rs:484`
- **`length`** (Function) — `crates/core/src/camera.rs:70`
- **`try_normalized`** (Function) — `crates/core/src/camera.rs:74`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `bounds_fit_one_pixel` | Function | `crates/core/src/camera.rs` | 616 |
| `position` | Function | `crates/core/src/camera.rs` | 338 |
| `set_observation_anchor` | Function | `crates/core/src/camera.rs` | 484 |
| `length` | Function | `crates/core/src/camera.rs` | 70 |
| `try_normalized` | Function | `crates/core/src/camera.rs` | 74 |
| `reset_projection_context_build_count` | Function | `crates/core/src/camera.rs` | 197 |
| `len` | Function | `crates/core/src/frame.rs` | 509 |
| `circulating_supply` | Function | `crates/core/src/history.rs` | 449 |
| `current_world` | Function | `crates/core/src/history.rs` | 435 |
| `edits` | Function | `crates/core/src/history.rs` | 711 |
| `apply` | Function | `crates/core/src/program.rs` | 230 |
| `replay` | Function | `crates/core/src/program.rs` | 451 |
| `changed_rows` | Function | `crates/core/src/render.rs` | 476 |
| `draw_crosshair` | Function | `crates/core/src/render.rs` | 688 |
| `frame_serial` | Function | `crates/core/src/render.rs` | 468 |
| `height` | Function | `crates/core/src/render.rs` | 508 |
| `new` | Function | `crates/core/src/render.rs` | 389 |
| `observe_final_game_frame` | Function | `crates/core/src/render.rs` | 520 |
| `observe_final_game_frame_tinted` | Function | `crates/core/src/render.rs` | 530 |
| `pixels` | Function | `crates/core/src/render.rs` | 512 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Combine2_pass → Bounds` | cross_community | 10 |
| `F3_recomposes_sidebar_without_forging_a_changed_game_frame → New` | cross_community | 10 |
| `Collapse_pass → Bounds` | cross_community | 10 |
| `F4_and_f5_requests_are_mutually_exclusive_without_state_conversion → New` | cross_community | 10 |
| `Convect_pass → Bounds` | cross_community | 10 |
| `F4_arms_only_observational_work_without_mutating_tick_authority → New` | cross_community | 10 |
| `Degas_pass → Bounds` | cross_community | 10 |
| `F5_arms_the_next_presentation_without_forging_tick_or_render_authority → New` | cross_community | 10 |
| `Dissolve_pass → Bounds` | cross_community | 10 |
| `Live_v2_streams_follow_independent_f3_endpoints_and_catch_up_paths → New` | cross_community | 10 |

## How to Explore

1. `context({name: "bounds_fit_one_pixel"})` — see callers and callees
2. `query({search_query: "examples"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
