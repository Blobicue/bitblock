---
name: gitnexus-area-cluster-64
description: "Skill for the Cluster_64 area of bitblock. 24 symbols across 2 files."
---

# Cluster_64

24 symbols | 2 files | Cohesion: 64%

## When to Use

- Working with code in `crates/`
- Understanding how verify, new, encode_canonical work
- Modifying cluster_64-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/core/src/frame.rs` | recompute_tag, verify, new, recompute_tag, encode_canonical (+18) |
| `crates/research/src/optimization.rs` | validate_frame_input |

## Entry Points

Start here when exploring this area:

- **`verify`** (Function) — `crates/core/src/frame.rs:361`
- **`new`** (Function) — `crates/core/src/frame.rs:420`
- **`encode_canonical`** (Function) — `crates/core/src/frame.rs:84`
- **`observe_with`** (Function) — `crates/core/src/frame.rs:126`
- **`new`** (Function) — `crates/core/src/frame.rs:174`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `verify` | Function | `crates/core/src/frame.rs` | 361 |
| `new` | Function | `crates/core/src/frame.rs` | 420 |
| `encode_canonical` | Function | `crates/core/src/frame.rs` | 84 |
| `observe_with` | Function | `crates/core/src/frame.rs` | 126 |
| `new` | Function | `crates/core/src/frame.rs` | 174 |
| `verify` | Function | `crates/core/src/frame.rs` | 199 |
| `copy_exact` | Function | `crates/core/src/frame.rs` | 679 |
| `recompute_tag` | Function | `crates/core/src/frame.rs` | 371 |
| `recompute_tag` | Function | `crates/core/src/frame.rs` | 460 |
| `encode_with` | Function | `crates/core/src/frame.rs` | 88 |
| `recompute_tag` | Function | `crates/core/src/frame.rs` | 207 |
| `encode` | Function | `crates/core/src/frame.rs` | 819 |
| `frame_input_tag` | Function | `crates/core/src/frame.rs` | 383 |
| `frame_receipt_tag` | Function | `crates/core/src/frame.rs` | 465 |
| `overlay_input_tag` | Function | `crates/core/src/frame.rs` | 212 |
| `require_kind` | Function | `crates/core/src/frame.rs` | 608 |
| `callback_observation_is_row_major_and_keeps_the_standard_frame_result` | Function | `crates/core/src/frame.rs` | 775 |
| `canonical_bytes` | Function | `crates/core/src/frame.rs` | 739 |
| `canonical_two_by_two_vector_is_fixed_and_big_endian` | Function | `crates/core/src/frame.rs` | 794 |
| `exact_copy_rejects_wrong_shape_and_preserves_every_word` | Function | `crates/core/src/frame.rs` | 847 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Canonical_decoder_rejects_prefixes_trailing_bytes_and_structural_mutations → Payload_sink` | cross_community | 9 |
| `Tolerance_boundary_is_inclusive_and_one_code_excess_refuses → Payload_sink` | cross_community | 9 |
| `Canonical_decoder_rejects_prefixes_trailing_bytes_and_structural_mutations → Name` | cross_community | 8 |
| `Display_ledger_chains_matches_mismatches_and_failures_without_erasure → Payload_sink` | cross_community | 8 |
| `Tolerance_boundary_is_inclusive_and_one_code_excess_refuses → Name` | cross_community | 8 |
| `Display_receipts_reject_production_impossible_capture_transcripts → Payload_sink` | cross_community | 8 |
| `Frame_ledger_enforces_parent_tick_and_canonical_receipt → Payload_sink` | cross_community | 8 |
| `Cross_section_lights_cut_faces_and_keeps_bands_distinct → Payload_sink` | cross_community | 8 |
| `Tolerance_boundary_is_inclusive_and_one_code_excess_refuses → ContentTag` | cross_community | 7 |
| `Display_ledger_chains_matches_mismatches_and_failures_without_erasure → Name` | cross_community | 7 |

## How to Explore

1. `context({name: "verify"})` — see callers and callees
2. `query({search_query: "cluster_64"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
