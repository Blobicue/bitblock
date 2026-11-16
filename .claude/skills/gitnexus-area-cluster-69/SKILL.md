---
name: gitnexus-area-cluster-69
description: "Skill for the Cluster_69 area of bitblock. 21 symbols across 3 files."
---

# Cluster_69

21 symbols | 3 files | Cohesion: 53%

## When to Use

- Working with code in `src/`
- Understanding how height, observe, pixels work
- Modifying cluster_69-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/main.rs` | present, request_display_artifact_capture, request_presentation_output_capture, can_capture_new, pending_artifact (+11) |
| `crates/core/src/frame.rs` | height, observe, pixels, width |
| `crates/research/src/display_artifact.rs` | complete_facts |

## Entry Points

Start here when exploring this area:

- **`height`** (Function) — `crates/core/src/frame.rs:56`
- **`observe`** (Function) — `crates/core/src/frame.rs:116`
- **`pixels`** (Function) — `crates/core/src/frame.rs:60`
- **`width`** (Function) — `crates/core/src/frame.rs:52`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `height` | Function | `crates/core/src/frame.rs` | 56 |
| `observe` | Function | `crates/core/src/frame.rs` | 116 |
| `pixels` | Function | `crates/core/src/frame.rs` | 60 |
| `width` | Function | `crates/core/src/frame.rs` | 52 |
| `complete_facts` | Function | `crates/research/src/display_artifact.rs` | 820 |
| `present` | Function | `src/main.rs` | 2367 |
| `request_display_artifact_capture` | Function | `src/main.rs` | 2622 |
| `request_presentation_output_capture` | Function | `src/main.rs` | 2664 |
| `can_capture_new` | Function | `src/main.rs` | 680 |
| `pending_artifact` | Function | `src/main.rs` | 686 |
| `pending_presentation_output` | Function | `src/main.rs` | 692 |
| `drop` | Function | `src/main.rs` | 4053 |
| `report_display_artifact_result` | Function | `src/main.rs` | 946 |
| `report_presentation_output_result` | Function | `src/main.rs` | 1164 |
| `service_display_artifact_request` | Function | `src/main.rs` | 919 |
| `service_presentation_output_request_with` | Function | `src/main.rs` | 1079 |
| `submit_live_frame_pair` | Function | `src/main.rs` | 1431 |
| `live_v2_pair_submission_is_atomic_across_backend_refusal_and_retry` | Function | `src/main.rs` | 5596 |
| `refused_append_retains_one_exact_artifact_for_retry_without_recapture` | Function | `src/main.rs` | 5308 |
| `refused_f5_append_retains_the_exact_full_wrapper_for_storage_only_retry` | Function | `src/main.rs` | 5388 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Canonical_decoder_rejects_prefixes_trailing_bytes_and_structural_mutations → Payload_sink` | cross_community | 9 |
| `Tolerance_boundary_is_inclusive_and_one_code_excess_refuses → Payload_sink` | cross_community | 9 |
| `Canonical_decoder_rejects_prefixes_trailing_bytes_and_structural_mutations → Name` | cross_community | 8 |
| `Display_ledger_chains_matches_mismatches_and_failures_without_erasure → Payload_sink` | cross_community | 8 |
| `Tolerance_boundary_is_inclusive_and_one_code_excess_refuses → Name` | cross_community | 8 |
| `Display_receipts_reject_production_impossible_capture_transcripts → Payload_sink` | cross_community | 8 |
| `Tolerance_boundary_is_inclusive_and_one_code_excess_refuses → ContentTag` | cross_community | 7 |
| `Display_ledger_chains_matches_mismatches_and_failures_without_erasure → Name` | cross_community | 7 |
| `Display_receipts_reject_production_impossible_capture_transcripts → Name` | cross_community | 7 |
| `Refused_append_retains_one_exact_artifact_for_retry_without_recapture → New` | cross_community | 7 |

## How to Explore

1. `context({name: "height"})` — see callers and callees
2. `query({search_query: "cluster_69"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
