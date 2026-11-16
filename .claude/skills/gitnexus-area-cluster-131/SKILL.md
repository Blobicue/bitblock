---
name: gitnexus-area-cluster-131
description: "Skill for the Cluster_131 area of bitblock. 41 symbols across 4 files."
---

# Cluster_131

41 symbols | 4 files | Cohesion: 80%

## When to Use

- Working with code in `crates/`
- Understanding how export_rejected_tail, export_rejected_tail, discard_unverified_tail work
- Modifying cluster_131-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/research/src/journal.rs` | required, read_exact_bytes, discard_unverified_tail, export_rejected_tail, verified_prefix_records (+27) |
| `crates/research/src/storage.rs` | encode, encoded_bytes, head, limits, records (+2) |
| `crates/research/src/display_store.rs` | export_rejected_tail |
| `crates/research/src/history_store.rs` | export_rejected_tail |

## Entry Points

Start here when exploring this area:

- **`export_rejected_tail`** (Function) — `crates/research/src/display_store.rs:1186`
- **`export_rejected_tail`** (Function) — `crates/research/src/history_store.rs:1421`
- **`discard_unverified_tail`** (Function) — `crates/research/src/journal.rs:645`
- **`export_rejected_tail`** (Function) — `crates/research/src/journal.rs:604`
- **`verified_prefix_records`** (Function) — `crates/research/src/journal.rs:599`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `export_rejected_tail` | Function | `crates/research/src/display_store.rs` | 1186 |
| `export_rejected_tail` | Function | `crates/research/src/history_store.rs` | 1421 |
| `discard_unverified_tail` | Function | `crates/research/src/journal.rs` | 645 |
| `export_rejected_tail` | Function | `crates/research/src/journal.rs` | 604 |
| `verified_prefix_records` | Function | `crates/research/src/journal.rs` | 599 |
| `inspect_bytes` | Function | `crates/research/src/journal.rs` | 776 |
| `encode` | Function | `crates/research/src/storage.rs` | 527 |
| `encoded_bytes` | Function | `crates/research/src/storage.rs` | 458 |
| `head` | Function | `crates/research/src/storage.rs` | 454 |
| `limits` | Function | `crates/research/src/storage.rs` | 446 |
| `records` | Function | `crates/research/src/storage.rs` | 450 |
| `max_records` | Function | `crates/research/src/storage.rs` | 96 |
| `max_stream_bytes` | Function | `crates/research/src/storage.rs` | 100 |
| `required` | Function | `crates/research/src/journal.rs` | 49 |
| `read_exact_bytes` | Function | `crates/research/src/journal.rs` | 1208 |
| `append_synced_with_io` | Function | `crates/research/src/journal.rs` | 392 |
| `classify_rejection` | Function | `crates/research/src/journal.rs` | 868 |
| `discard_tail_with_io` | Function | `crates/research/src/journal.rs` | 662 |
| `file_len` | Function | `crates/research/src/journal.rs` | 960 |
| `inspect` | Function | `crates/research/src/journal.rs` | 799 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Every_append_cut_has_one_exact_verified_prefix → Kind` | cross_community | 6 |
| `Every_append_cut_has_one_exact_verified_prefix → Head` | cross_community | 5 |
| `Every_append_cut_has_one_exact_verified_prefix → Parent` | cross_community | 5 |
| `Absent_capture_inventory_keeps_reference_only_and_embedded_roles_distinct → Head` | cross_community | 4 |
| `Read_only_inspection_replays_an_anchored_clean_chain_and_never_admits_a_tail → Head` | cross_community | 4 |
| `Exact_boundary_rollback_and_alternate_valid_branch_remain_locally_admissible → Head` | cross_community | 4 |
| `History_sample_chain → Head` | cross_community | 4 |
| `Every_append_cut_has_one_exact_verified_prefix → Name` | cross_community | 4 |
| `Recovery_handle_exposes_exactly_the_scanner_admitted_prefix → Name` | cross_community | 4 |
| `Torn_append_never_resynchronizes_to_later_valid_magic → Name` | cross_community | 4 |

## How to Explore

1. `context({name: "export_rejected_tail"})` — see callers and callees
2. `query({search_query: "cluster_131"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
