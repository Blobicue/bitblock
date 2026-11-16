---
name: gitnexus-area-cluster-55
description: "Skill for the Cluster_55 area of bitblock. 45 symbols across 10 files."
---

# Cluster_55

45 symbols | 10 files | Cohesion: 74%

## When to Use

- Working with code in `crates/`
- Understanding how expected_frame, parent, tag work
- Modifying cluster_55-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/research/src/display_store.rs` | append_artifact, append_capture, append_presentation_output, append_presentation_output_v2, carrier_head (+8) |
| `src/main.rs` | append_pending, append_pending_presentation_output, new, prepare_and_append, prepare_and_append_presentation_output (+3) |
| `crates/research/src/journal.rs` | append_synced, head, is_poisoned, records, valid_bytes |
| `crates/research/src/presentation_output.rs` | display, post_topology, timing, verify, exact_nested_layout_round_trips_without_extra_pixel_or_tag_lanes |
| `crates/core/src/display.rs` | expected_frame, parent, tag |
| `crates/research/src/display_artifact.rs` | captured_form, receipt, tag |
| `crates/research/src/history_store.rs` | carrier_head, carrier_is_poisoned, carrier_valid_bytes |
| `crates/research/src/presentation.rs` | receipt, tag |
| `crates/research/src/windows_topology.rs` | color, topology |
| `crates/research/src/presentation_output_v2.rs` | tag |

## Entry Points

Start here when exploring this area:

- **`expected_frame`** (Function) — `crates/core/src/display.rs:443`
- **`parent`** (Function) — `crates/core/src/display.rs:435`
- **`tag`** (Function) — `crates/core/src/display.rs:431`
- **`captured_form`** (Function) — `crates/research/src/display_artifact.rs:198`
- **`receipt`** (Function) — `crates/research/src/display_artifact.rs:182`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `expected_frame` | Function | `crates/core/src/display.rs` | 443 |
| `parent` | Function | `crates/core/src/display.rs` | 435 |
| `tag` | Function | `crates/core/src/display.rs` | 431 |
| `captured_form` | Function | `crates/research/src/display_artifact.rs` | 198 |
| `receipt` | Function | `crates/research/src/display_artifact.rs` | 182 |
| `tag` | Function | `crates/research/src/display_artifact.rs` | 178 |
| `append_artifact` | Function | `crates/research/src/display_store.rs` | 709 |
| `append_capture` | Function | `crates/research/src/display_store.rs` | 696 |
| `append_presentation_output` | Function | `crates/research/src/display_store.rs` | 836 |
| `append_presentation_output_v2` | Function | `crates/research/src/display_store.rs` | 993 |
| `carrier_head` | Function | `crates/research/src/display_store.rs` | 632 |
| `carrier_valid_bytes` | Function | `crates/research/src/display_store.rs` | 636 |
| `is_poisoned` | Function | `crates/research/src/display_store.rs` | 640 |
| `prepare_capture_artifact` | Function | `crates/research/src/display_store.rs` | 672 |
| `prepare_next_receipt` | Function | `crates/research/src/display_store.rs` | 648 |
| `display_receipt_head` | Function | `crates/research/src/display_store.rs` | 116 |
| `len` | Function | `crates/research/src/display_store.rs` | 100 |
| `storage_head` | Function | `crates/research/src/display_store.rs` | 120 |
| `carrier_head` | Function | `crates/research/src/history_store.rs` | 1241 |
| `carrier_is_poisoned` | Function | `crates/research/src/history_store.rs` | 1249 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Append_presentation_output_v2 → Frame_receipt_v2_tag` | cross_community | 10 |
| `Append_presentation_output_v2 → Kind` | cross_community | 10 |
| `Append_presentation_output_v2 → Encode_canonical` | cross_community | 10 |
| `Append_presentation_output_v2 → Checked_frame_len` | cross_community | 10 |
| `Color_parent_binding_and_nested_mutations_fail_closed → Encode_optional_tag` | cross_community | 10 |
| `Append_presentation_output_v2 → Digest` | cross_community | 10 |
| `Color_parent_binding_and_nested_mutations_fail_closed → Frame_receipt_v2_tag` | cross_community | 10 |
| `Color_parent_binding_and_nested_mutations_fail_closed → Kind` | cross_community | 10 |
| `Color_parent_binding_and_nested_mutations_fail_closed → Encode_canonical` | cross_community | 10 |
| `Color_parent_binding_and_nested_mutations_fail_closed → Checked_frame_len` | cross_community | 10 |

## How to Explore

1. `context({name: "expected_frame"})` — see callers and callees
2. `query({search_query: "cluster_55"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
