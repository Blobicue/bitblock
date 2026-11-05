---
name: gitnexus-area-cluster-73
description: "Skill for the Cluster_73 area of bitblock. 22 symbols across 3 files."
---

# Cluster_73

22 symbols | 3 files | Cohesion: 67%

## When to Use

- Working with code in `crates/`
- Understanding how genesis_payload, create_new_with_replay_limits, history work
- Modifying cluster_73-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/research/src/history_store.rs` | create_new_with_replay_limits, history, world, replay_history_records, assert_repaired_carrier_rejected (+15) |
| `crates/core/src/history.rs` | genesis_payload |
| `crates/research/src/storage.rs` | append |

## Entry Points

Start here when exploring this area:

- **`genesis_payload`** (Function) — `crates/core/src/history.rs:423`
- **`create_new_with_replay_limits`** (Function) — `crates/research/src/history_store.rs:1118`
- **`history`** (Function) — `crates/research/src/history_store.rs:39`
- **`world`** (Function) — `crates/research/src/history_store.rs:35`
- **`replay_history_records`** (Function) — `crates/research/src/history_store.rs:863`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `genesis_payload` | Function | `crates/core/src/history.rs` | 423 |
| `create_new_with_replay_limits` | Function | `crates/research/src/history_store.rs` | 1118 |
| `history` | Function | `crates/research/src/history_store.rs` | 39 |
| `world` | Function | `crates/research/src/history_store.rs` | 35 |
| `replay_history_records` | Function | `crates/research/src/history_store.rs` | 863 |
| `append` | Function | `crates/research/src/storage.rs` | 462 |
| `assert_repaired_carrier_rejected` | Function | `crates/research/src/history_store.rs` | 1542 |
| `bounded_in_memory_verification_matches_persistent_usage_and_refuses_each_limit` | Function | `crates/research/src/history_store.rs` | 1584 |
| `caller_supplied_initial_world_is_part_of_the_replay_claim` | Function | `crates/research/src/history_store.rs` | 1899 |
| `exact_boundary_rollback_and_alternate_valid_branch_remain_locally_admissible` | Function | `crates/research/src/history_store.rs` | 2012 |
| `matched_chain` | Function | `crates/research/src/history_store.rs` | 1487 |
| `objects` | Function | `crates/research/src/history_store.rs` | 1520 |
| `public_replay_refuses_a_broken_physical_parent_slice` | Function | `crates/research/src/history_store.rs` | 1911 |
| `rebuild` | Function | `crates/research/src/history_store.rs` | 1534 |
| `receipt_offsets` | Function | `crates/research/src/history_store.rs` | 1573 |
| `repaired_carrier_hashes_cannot_substitute_for_coordinate_semantics` | Function | `crates/research/src/history_store.rs` | 1758 |
| `replay_policy_bounds_decode_state_and_cumulative_world_tag_work` | Function | `crates/research/src/history_store.rs` | 1927 |
| `semantic_replay_reconstructs_world_effects_and_provenance` | Function | `crates/research/src/history_store.rs` | 1684 |
| `new` | Function | `crates/research/src/history_store.rs` | 2059 |
| `carrier_valid_semantic_failure_is_never_exposed_as_writable_or_repairable` | Function | `crates/research/src/history_store.rs` | 2491 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Semantic_replay_reconstructs_world_effects_and_provenance → Bounds` | cross_community | 7 |
| `Absent_capture_inventory_keeps_reference_only_and_embedded_roles_distinct → Kind` | cross_community | 6 |
| `Absent_capture_inventory_keeps_reference_only_and_embedded_roles_distinct → Name` | cross_community | 6 |
| `Read_only_inspection_replays_an_anchored_clean_chain_and_never_admits_a_tail → Kind` | cross_community | 6 |
| `Semantic_replay_reconstructs_world_effects_and_provenance → Split_coord` | cross_community | 6 |
| `Read_only_inspection_replays_an_anchored_clean_chain_and_never_admits_a_tail → Name` | cross_community | 6 |
| `Exact_boundary_rollback_and_alternate_valid_branch_remain_locally_admissible → Kind` | cross_community | 6 |
| `Exact_boundary_rollback_and_alternate_valid_branch_remain_locally_admissible → Name` | cross_community | 6 |
| `Every_append_cut_has_one_exact_verified_prefix → Kind` | cross_community | 6 |
| `History_sample_chain → Name` | cross_community | 6 |

## How to Explore

1. `context({name: "genesis_payload"})` — see callers and callees
2. `query({search_query: "cluster_73"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
