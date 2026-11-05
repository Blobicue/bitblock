---
name: gitnexus-area-cluster-71
description: "Skill for the Cluster_71 area of bitblock. 69 symbols across 4 files."
---

# Cluster_71

69 symbols | 4 files | Cohesion: 70%

## When to Use

- Working with code in `src/`
- Understanding how limits, tick, entries work
- Modifying cluster_71-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/main.rs` | about_to_wait, advance_ticks, apply_infallible_accepted_action, capture_mouse, key (+61) |
| `crates/core/src/frame_v2.rs` | limits |
| `crates/core/src/tick.rs` | tick |
| `crates/research/src/display_store.rs` | entries |

## Entry Points

Start here when exploring this area:

- **`limits`** (Function) — `crates/core/src/frame_v2.rs:486`
- **`tick`** (Function) — `crates/core/src/tick.rs:771`
- **`entries`** (Function) — `crates/research/src/display_store.rs:96`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `limits` | Function | `crates/core/src/frame_v2.rs` | 486 |
| `tick` | Function | `crates/core/src/tick.rs` | 771 |
| `entries` | Function | `crates/research/src/display_store.rs` | 96 |
| `about_to_wait` | Function | `src/main.rs` | 3901 |
| `advance_ticks` | Function | `src/main.rs` | 2348 |
| `apply_infallible_accepted_action` | Function | `src/main.rs` | 1827 |
| `capture_mouse` | Function | `src/main.rs` | 1832 |
| `key` | Function | `src/main.rs` | 2712 |
| `new` | Function | `src/main.rs` | 1679 |
| `queue_wheel` | Function | `src/main.rs` | 1862 |
| `resumed` | Function | `src/main.rs` | 3755 |
| `window_event` | Function | `src/main.rs` | 3829 |
| `with_program_mode` | Function | `src/main.rs` | 1691 |
| `apply_aim_edit` | Function | `src/main.rs` | 3980 |
| `checked_headless_charge` | Function | `src/main.rs` | 2775 |
| `configure_display_artifact_journal` | Function | `src/main.rs` | 190 |
| `display_audit_enabled` | Function | `src/main.rs` | 3692 |
| `fingerprint_running_executable` | Function | `src/main.rs` | 3364 |
| `headless_chunk_key` | Function | `src/main.rs` | 2816 |
| `headless_chunk_word_units` | Function | `src/main.rs` | 2880 |

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

1. `context({name: "limits"})` — see callers and callees
2. `query({search_query: "cluster_71"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
