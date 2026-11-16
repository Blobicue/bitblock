---
name: gitnexus-area-cluster-0
description: "Skill for the Cluster_0 area of bitblock. 63 symbols across 4 files."
---

# Cluster_0

63 symbols | 4 files | Cohesion: 85%

## When to Use

- Working with code in `crates/`
- Understanding how apply_mouse_delta, basis, center_ray work
- Modifying cluster_0-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/core/src/camera.rs` | apply_mouse_delta, basis, center_ray, dolly_clamped, fit_bounds (+51) |
| `src/main.rs` | dolly_observer, maximum_depth_fills_every_pixel_after_camera_rotation, place_cannot_enclose_the_observer_at_spawn |
| `crates/wasm/src/lib.rs` | click, coord_for_canvas_pixel |
| `crates/wasm/src/play.rs` | dolly_observer, resolve_aim_edit |

## Entry Points

Start here when exploring this area:

- **`apply_mouse_delta`** (Function) — `crates/core/src/camera.rs:388`
- **`basis`** (Function) — `crates/core/src/camera.rs:362`
- **`center_ray`** (Function) — `crates/core/src/camera.rs:521`
- **`dolly_clamped`** (Function) — `crates/core/src/camera.rs:447`
- **`fit_bounds`** (Function) — `crates/core/src/camera.rs:591`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `apply_mouse_delta` | Function | `crates/core/src/camera.rs` | 388 |
| `basis` | Function | `crates/core/src/camera.rs` | 362 |
| `center_ray` | Function | `crates/core/src/camera.rs` | 521 |
| `dolly_clamped` | Function | `crates/core/src/camera.rs` | 447 |
| `fit_bounds` | Function | `crates/core/src/camera.rs` | 591 |
| `fit_distance_for_bounds` | Function | `crates/core/src/camera.rs` | 579 |
| `for_observer_spawn` | Function | `crates/core/src/camera.rs` | 317 |
| `for_single_pixel_voxel` | Function | `crates/core/src/camera.rs` | 326 |
| `forward` | Function | `crates/core/src/camera.rs` | 373 |
| `from_anchor` | Function | `crates/core/src/camera.rs` | 293 |
| `from_position` | Function | `crates/core/src/camera.rs` | 265 |
| `observation_anchor` | Function | `crates/core/src/camera.rs` | 342 |
| `observation_distance` | Function | `crates/core/src/camera.rs` | 346 |
| `project` | Function | `crates/core/src/camera.rs` | 560 |
| `projection_for_viewport` | Function | `crates/core/src/camera.rs` | 502 |
| `ray_for_pixel` | Function | `crates/core/src/camera.rs` | 529 |
| `right` | Function | `crates/core/src/camera.rs` | 377 |
| `rotate` | Function | `crates/core/src/camera.rs` | 398 |
| `set_observation_distance` | Function | `crates/core/src/camera.rs` | 471 |
| `up` | Function | `crates/core/src/camera.rs` | 381 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `F3_recomposes_sidebar_without_forging_a_changed_game_frame → New` | cross_community | 10 |
| `F4_and_f5_requests_are_mutually_exclusive_without_state_conversion → New` | cross_community | 10 |
| `F4_arms_only_observational_work_without_mutating_tick_authority → New` | cross_community | 10 |
| `F5_arms_the_next_presentation_without_forging_tick_or_render_authority → New` | cross_community | 10 |
| `Live_v2_streams_follow_independent_f3_endpoints_and_catch_up_paths → New` | cross_community | 10 |
| `Pending_streams_resolve_their_exact_independent_outcome_endpoints → New` | cross_community | 10 |
| `Physical_w_key_adapter_and_normalized_tape_map_to_the_same_action → New` | cross_community | 10 |
| `F3_recomposes_sidebar_without_forging_a_changed_game_frame → CameraBasis` | cross_community | 9 |
| `F3_recomposes_sidebar_without_forging_a_changed_game_frame → Is_finite` | cross_community | 9 |
| `F3_recomposes_sidebar_without_forging_a_changed_game_frame → ProjectedPoint` | cross_community | 9 |

## How to Explore

1. `context({name: "apply_mouse_delta"})` — see callers and callees
2. `query({search_query: "cluster_0"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
