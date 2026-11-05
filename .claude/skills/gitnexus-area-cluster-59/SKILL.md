---
name: gitnexus-area-cluster-59
description: "Skill for the Cluster_59 area of bitblock. 63 symbols across 6 files."
---

# Cluster_59

63 symbols | 6 files | Cohesion: 89%

## When to Use

- Working with code in `crates/`
- Understanding how verify, renderer_profile, admit_derived work
- Modifying cluster_59-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/core/src/frame_v2.rs` | admit_derived, admit_existing, head, new_structural_genesis, outcome_head (+41) |
| `crates/core/src/tick.rs` | frame, input, composite, game, input (+2) |
| `crates/research/src/optimization.rs` | evidence_ledger, evidence_outcome, evidence_outcome_payload, independent_python_vectors_pin_full_ledger_evidence, ladder (+2) |
| `crates/core/src/display.rs` | verify |
| `crates/core/src/frame.rs` | renderer_profile |
| `src/main.rs` | current_committed_composite_frame |

## Entry Points

Start here when exploring this area:

- **`verify`** (Function) — `crates/core/src/display.rs:519`
- **`renderer_profile`** (Function) — `crates/core/src/frame.rs:349`
- **`admit_derived`** (Function) — `crates/core/src/frame_v2.rs:520`
- **`admit_existing`** (Function) — `crates/core/src/frame_v2.rs:550`
- **`head`** (Function) — `crates/core/src/frame_v2.rs:506`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `verify` | Function | `crates/core/src/display.rs` | 519 |
| `renderer_profile` | Function | `crates/core/src/frame.rs` | 349 |
| `admit_derived` | Function | `crates/core/src/frame_v2.rs` | 520 |
| `admit_existing` | Function | `crates/core/src/frame_v2.rs` | 550 |
| `head` | Function | `crates/core/src/frame_v2.rs` | 506 |
| `new_structural_genesis` | Function | `crates/core/src/frame_v2.rs` | 460 |
| `outcome_head` | Function | `crates/core/src/frame_v2.rs` | 512 |
| `stream` | Function | `crates/core/src/frame_v2.rs` | 482 |
| `verify` | Function | `crates/core/src/frame_v2.rs` | 599 |
| `with_limits` | Function | `crates/core/src/frame_v2.rs` | 467 |
| `decode_canonical` | Function | `crates/core/src/frame_v2.rs` | 118 |
| `frame` | Function | `crates/core/src/frame_v2.rs` | 100 |
| `from_outcome` | Function | `crates/core/src/frame_v2.rs` | 174 |
| `new` | Function | `crates/core/src/frame_v2.rs` | 57 |
| `parent` | Function | `crates/core/src/frame_v2.rs` | 84 |
| `stream` | Function | `crates/core/src/frame_v2.rs` | 88 |
| `tag` | Function | `crates/core/src/frame_v2.rs` | 80 |
| `verify` | Function | `crates/core/src/frame_v2.rs` | 156 |
| `verify_selected_outcome` | Function | `crates/core/src/frame_v2.rs` | 199 |
| `admit_frame_pair_derived` | Function | `crates/core/src/frame_v2.rs` | 701 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Ledger_limits_and_stored_evidence_fail_closed → Name` | cross_community | 10 |
| `Ledger_limits_and_stored_evidence_fail_closed → Feed_sidebar_policy` | cross_community | 10 |
| `Strict_decoder_refuses_noncanonical_lengths_forms_frames_and_suffixes → Code` | cross_community | 10 |
| `Strict_decoder_refuses_noncanonical_lengths_forms_frames_and_suffixes → Digest` | cross_community | 10 |
| `Append_presentation_output_v2 → Frame_receipt_v2_tag` | cross_community | 10 |
| `Append_presentation_output_v2 → Kind` | cross_community | 10 |
| `Ledger_refuses_missing_reordered_and_forked_edges_without_mutation → Name` | cross_community | 10 |
| `Ledger_refuses_missing_reordered_and_forked_edges_without_mutation → Feed_sidebar_policy` | cross_community | 10 |
| `Tolerance_boundary_is_inclusive_and_one_code_excess_refuses → Code` | cross_community | 10 |
| `Color_parent_binding_and_nested_mutations_fail_closed → Frame_receipt_v2_tag` | cross_community | 10 |

## How to Explore

1. `context({name: "verify"})` — see callers and callees
2. `query({search_query: "cluster_59"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
