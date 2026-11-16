---
name: gitnexus-area-cluster-209
description: "Skill for the Cluster_209 area of bitblock. 25 symbols across 1 files."
---

# Cluster_209

25 symbols | 1 files | Cohesion: 88%

## When to Use

- Working with code in `crates/`
- Understanding how observe_active_output work
- Modifying cluster_209-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/research/src/windows_topology.rs` | observe_active_output, observe_active_output_with, base_api, clone_source_retains_every_target_route, display_name (+20) |

## Entry Points

Start here when exploring this area:

- **`observe_active_output`** (Function) — `crates/research/src/windows_topology.rs:425`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `observe_active_output` | Function | `crates/research/src/windows_topology.rs` | 425 |
| `observe_active_output_with` | Function | `crates/research/src/windows_topology.rs` | 448 |
| `base_api` | Function | `crates/research/src/windows_topology.rs` | 1440 |
| `clone_source_retains_every_target_route` | Function | `crates/research/src/windows_topology.rs` | 1743 |
| `display_name` | Function | `crates/research/src/windows_topology.rs` | 1345 |
| `duplicate_source_name_match_is_ambiguous_not_first_wins` | Function | `crates/research/src/windows_topology.rs` | 1868 |
| `fallback_profile_invalid_parameter_is_terminal` | Function | `crates/research/src/windows_topology.rs` | 1803 |
| `fixed` | Function | `crates/research/src/windows_topology.rs` | 1338 |
| `friendly_annotations_cannot_distinguish_materially_duplicate_routes` | Function | `crates/research/src/windows_topology.rs` | 1758 |
| `insufficient_buffer_repeats_size_query_but_other_errors_do_not` | Function | `crates/research/src/windows_topology.rs` | 1685 |
| `invalid_parameter_falls_back_once_to_0x12` | Function | `crates/research/src/windows_topology.rs` | 1662 |
| `malformed_successful_target_name_becomes_explicit_identity_failure` | Function | `crates/research/src/windows_topology.rs` | 1936 |
| `path` | Function | `crates/research/src/windows_topology.rs` | 1408 |
| `path_or_mode_cap_refuses_before_query_allocation` | Function | `crates/research/src/windows_topology.rs` | 1727 |
| `primary_profile_retains_one_route_and_exact_call_order` | Function | `crates/research/src/windows_topology.rs` | 1544 |
| `retry_budget_is_four_total_cycles` | Function | `crates/research/src/windows_topology.rs` | 1953 |
| `source_mode` | Function | `crates/research/src/windows_topology.rs` | 1363 |
| `successful_query_cannot_report_more_than_allocated` | Function | `crates/research/src/windows_topology.rs` | 1832 |
| `target_identity_failure_is_explicit_but_topology_remains_available` | Function | `crates/research/src/windows_topology.rs` | 1897 |
| `target_mode` | Function | `crates/research/src/windows_topology.rs` | 1379 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Duplicate_source_name_match_is_ambiguous_not_first_wins → Into_luid` | cross_community | 4 |
| `Duplicate_source_name_match_is_ambiguous_not_first_wins → Fixed` | intra_community | 4 |
| `Duplicate_source_name_match_is_ambiguous_not_first_wins → TargetKey` | intra_community | 3 |

## How to Explore

1. `context({name: "observe_active_output"})` — see callers and callees
2. `query({search_query: "cluster_209"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
