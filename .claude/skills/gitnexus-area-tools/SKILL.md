---
name: gitnexus-area-tools
description: "Skill for the Tools area of bitblock. 697 symbols across 42 files."
---

# Tools

697 symbols | 42 files | Cohesion: 81%

## When to Use

- Working with code in `tools/`
- Understanding how build_match_reuse, build_mismatch_inline, build_receipt work
- Modifying tools-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `tools/check_planning_docs.py` | _check_audit_addition_identity, _git_rev_parse, check_audited_distillation_receipt, check_design_source_receipt, check_eighth_audit_addition_identity (+100) |
| `tools/output_topology_vectors.py` | acquisition, assert_golden, canonical_preimage, content_tag, encode_acquisition (+50) |
| `tools/display_capture_artifact_vectors.py` | build_match_reuse, build_mismatch_inline, build_receipt, build_unavailable_absent, canonical_preimage (+36) |
| `tools/bbindex.py` | dead_surface, duplication, line_id, extract_docs, extract_examples (+26) |
| `tools/bbgraph.py` | cmd_dag, cmd_node, cmd_schema, cmd_sub, describe (+21) |
| `tools/reference_renderer_vectors.py` | canonical_preimage, depth_payload, digest, emit_rust, f64 (+19) |
| `tools/presentation_output_vectors.py` | build_nested_parts, build_timing_payload, assert_nested_goldens, canonical_preimage, content_tag (+18) |
| `tools/tick_outcome_vectors.py` | control_lanes_only, edit, f64_bits, genesis_idle, i64 (+18) |
| `tools/headless_replay_policy_vectors.py` | admission_policy_payload, composite_policy_digest, content_digest, crosshair_policy_digest, displayed_metric_bindings (+17) |
| `tools/interaction_tape_vectors.py` | decode, decode_action, byte, remaining, take (+17) |

## Entry Points

Start here when exploring this area:

- **`build_match_reuse`** (Function) — `tools/display_capture_artifact_vectors.py:691`
- **`build_mismatch_inline`** (Function) — `tools/display_capture_artifact_vectors.py:701`
- **`build_receipt`** (Function) — `tools/display_capture_artifact_vectors.py:669`
- **`build_unavailable_absent`** (Function) — `tools/display_capture_artifact_vectors.py:711`
- **`canonical_preimage`** (Function) — `tools/display_capture_artifact_vectors.py:59`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `build_match_reuse` | Function | `tools/display_capture_artifact_vectors.py` | 691 |
| `build_mismatch_inline` | Function | `tools/display_capture_artifact_vectors.py` | 701 |
| `build_receipt` | Function | `tools/display_capture_artifact_vectors.py` | 669 |
| `build_unavailable_absent` | Function | `tools/display_capture_artifact_vectors.py` | 711 |
| `canonical_preimage` | Function | `tools/display_capture_artifact_vectors.py` | 59 |
| `complete_facts` | Function | `tools/display_capture_artifact_vectors.py` | 623 |
| `content_tag` | Function | `tools/display_capture_artifact_vectors.py` | 79 |
| `decode_artifact` | Function | `tools/display_capture_artifact_vectors.py` | 517 |
| `decode_bool` | Function | `tools/display_capture_artifact_vectors.py` | 251 |
| `decode_display_facts` | Function | `tools/display_capture_artifact_vectors.py` | 257 |
| `decode_display_receipt` | Function | `tools/display_capture_artifact_vectors.py` | 471 |
| `decode_frame` | Function | `tools/display_capture_artifact_vectors.py` | 111 |
| `decode_frame_facts` | Function | `tools/display_capture_artifact_vectors.py` | 165 |
| `decode_frame_receipt` | Function | `tools/display_capture_artifact_vectors.py` | 187 |
| `decode_optional` | Function | `tools/display_capture_artifact_vectors.py` | 87 |
| `digest` | Function | `tools/display_capture_artifact_vectors.py` | 70 |
| `display_policy` | Function | `tools/display_capture_artifact_vectors.py` | 612 |
| `encode_artifact` | Function | `tools/display_capture_artifact_vectors.py` | 495 |
| `encode_display_facts` | Function | `tools/display_capture_artifact_vectors.py` | 218 |
| `encode_display_receipt` | Function | `tools/display_capture_artifact_vectors.py` | 457 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Main → Framed` | cross_community | 6 |
| `Main → Optional_digest` | cross_community | 6 |
| `Main → Framed` | cross_community | 6 |
| `Main → U64` | cross_community | 6 |
| `Main → Checked_i64` | cross_community | 6 |
| `Main → _error` | cross_community | 6 |
| `Main → Field` | intra_community | 5 |
| `Main → Field` | intra_community | 5 |
| `Main → U16` | cross_community | 5 |
| `Main → Remaining` | cross_community | 5 |

## How to Explore

1. `context({name: "build_match_reuse"})` — see callers and callees
2. `query({search_query: "tools"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
