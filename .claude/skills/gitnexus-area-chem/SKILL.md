---
name: gitnexus-area-chem
description: "Skill for the Chem area of bitblock. 335 symbols across 7 files."
---

# Chem

335 symbols | 7 files | Cohesion: 77%

## When to Use

- Working with code in `crates/`
- Understanding how coreward, outward_guard, set_mat work
- Modifying chem-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/core/src/chem/tests_p6.rs` | outward_step_is_the_nearest_lattice_direction, albedo_full_driven_sky_shell_books_exact_and_reflects_off_water, classic_ignores_gravity, mk, cloud_albedo_full_driven_sky_shell_books_exact_and_clouds_reflect (+167) |
| `crates/core/src/chem/p6.rs` | combine2_pass, first_nb6, is_rising, track_mask, cube (+45) |
| `crates/core/src/chem/p6_rest.rs` | col_top_near, collapse_pass, convect_pass, degas_pass, dissolve_pass (+32) |
| `crates/core/src/chem.rs` | coreward, outward_guard, set_mat, shell_blocked, energy_total (+31) |
| `crates/core/src/chem/net.rs` | mount, bb, find, g, is_atom (+19) |
| `crates/wasm/src/play_chem.rs` | run, set_dynamics, dynamics_default_classic_off_and_switch_keeps_books, full_ball_core_evap_keeps_books_and_differs_from_up_evap, full_ball_core_rain_sheds_locally_where_down_rain_spills (+3) |
| `crates/core/src/chem/vapor.rs` | vapor_sync, build, patch_count, spread, vapor_patch (+3) |

## Entry Points

Start here when exploring this area:

- **`coreward`** (Function) — `crates/core/src/chem.rs:1176`
- **`outward_guard`** (Function) — `crates/core/src/chem.rs:1169`
- **`set_mat`** (Function) — `crates/core/src/chem.rs:942`
- **`shell_blocked`** (Function) — `crates/core/src/chem.rs:1191`
- **`is_rising`** (Function) — `crates/core/src/chem/p6.rs:1979`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `coreward` | Function | `crates/core/src/chem.rs` | 1176 |
| `outward_guard` | Function | `crates/core/src/chem.rs` | 1169 |
| `set_mat` | Function | `crates/core/src/chem.rs` | 942 |
| `shell_blocked` | Function | `crates/core/src/chem.rs` | 1191 |
| `is_rising` | Function | `crates/core/src/chem/p6.rs` | 1979 |
| `track_mask` | Function | `crates/core/src/chem/p6.rs` | 145 |
| `cube` | Function | `crates/core/src/chem/p6.rs` | 984 |
| `nb6` | Function | `crates/core/src/chem/p6.rs` | 979 |
| `offset` | Function | `crates/core/src/chem/p6.rs` | 957 |
| `collapse_pass` | Function | `crates/core/src/chem/p6_rest.rs` | 1543 |
| `convect_pass` | Function | `crates/core/src/chem/p6_rest.rs` | 207 |
| `degas_pass` | Function | `crates/core/src/chem/p6_rest.rs` | 1445 |
| `dissolve_pass` | Function | `crates/core/src/chem/p6_rest.rs` | 1682 |
| `driven_dissolve_pass` | Function | `crates/core/src/chem/p6_rest.rs` | 729 |
| `driven_erode_pass` | Function | `crates/core/src/chem/p6_rest.rs` | 700 |
| `driven_water_pass` | Function | `crates/core/src/chem/p6_rest.rs` | 419 |
| `entrain_pass` | Function | `crates/core/src/chem/p6_rest.rs` | 779 |
| `erode_pass` | Function | `crates/core/src/chem/p6_rest.rs` | 1658 |
| `evap_frame` | Function | `crates/core/src/chem/p6_rest.rs` | 368 |
| `evap_open` | Function | `crates/core/src/chem/p6_rest.rs` | 380 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Combine2_pass → Bounds` | cross_community | 10 |
| `Collapse_pass → Bounds` | cross_community | 10 |
| `Convect_pass → Bounds` | cross_community | 10 |
| `Degas_pass → Bounds` | cross_community | 10 |
| `Dissolve_pass → Bounds` | cross_community | 10 |
| `Driven_dissolve_pass → Bounds` | cross_community | 10 |
| `Driven_water_pass → Bounds` | cross_community | 10 |
| `Entrain_pass → Bounds` | cross_community | 10 |
| `Settle_pass → Bounds` | cross_community | 10 |
| `Water_cycle_pass → Bounds` | cross_community | 10 |

## How to Explore

1. `context({name: "coreward"})` — see callers and callees
2. `query({search_query: "chem"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
