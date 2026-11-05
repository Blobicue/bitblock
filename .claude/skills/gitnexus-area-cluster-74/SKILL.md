---
name: gitnexus-area-cluster-74
description: "Skill for the Cluster_74 area of bitblock. 25 symbols across 4 files."
---

# Cluster_74

25 symbols | 4 files | Cohesion: 58%

## When to Use

- Working with code in `crates/`
- Understanding how content_tag, encode_canonical_preimage, hash_canonical work
- Modifying cluster_74-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/core/src/protocol.rs` | canonical_header_len, encode_canonical_preimage, hash_canonical, display_is_a_full_lowercase_unpadded_tag, feed_chunking_does_not_change_a_tag (+6) |
| `crates/core/src/sidebar.rs` | composite_overlay_input, composite_policy_tag, feed_sidebar_policy, sidebar_policy_payload_len, sidebar_policy_payload_reference (+2) |
| `crates/core/src/render.rs` | crosshair_policy_tag, renderer_profile_tag, analytic_corpus_tag, fixture_camera |
| `crates/core/src/history.rs` | content_tag, genesis_tag, world_tag_before_preallocation |

## Entry Points

Start here when exploring this area:

- **`content_tag`** (Function) — `crates/core/src/history.rs:270`
- **`encode_canonical_preimage`** (Function) — `crates/core/src/protocol.rs:344`
- **`hash_canonical`** (Function) — `crates/core/src/protocol.rs:321`
- **`crosshair_policy_tag`** (Function) — `crates/core/src/render.rs:117`
- **`renderer_profile_tag`** (Function) — `crates/core/src/render.rs:78`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `content_tag` | Function | `crates/core/src/history.rs` | 270 |
| `encode_canonical_preimage` | Function | `crates/core/src/protocol.rs` | 344 |
| `hash_canonical` | Function | `crates/core/src/protocol.rs` | 321 |
| `crosshair_policy_tag` | Function | `crates/core/src/render.rs` | 117 |
| `renderer_profile_tag` | Function | `crates/core/src/render.rs` | 78 |
| `composite_overlay_input` | Function | `crates/core/src/sidebar.rs` | 438 |
| `composite_policy_tag` | Function | `crates/core/src/sidebar.rs` | 509 |
| `sidebar_policy_tag` | Function | `crates/core/src/sidebar.rs` | 443 |
| `genesis_tag` | Function | `crates/core/src/history.rs` | 804 |
| `world_tag_before_preallocation` | Function | `crates/core/src/history.rs` | 784 |
| `canonical_header_len` | Function | `crates/core/src/protocol.rs` | 380 |
| `display_is_a_full_lowercase_unpadded_tag` | Function | `crates/core/src/protocol.rs` | 623 |
| `feed_chunking_does_not_change_a_tag` | Function | `crates/core/src/protocol.rs` | 510 |
| `full_tag_parser_round_trips_and_refuses_noncanonical_forms` | Function | `crates/core/src/protocol.rs` | 634 |
| `kind_schema_and_payload_are_domain_separated` | Function | `crates/core/src/protocol.rs` | 522 |
| `materialized_preimage_is_exactly_what_hash_canonical_hashes` | Function | `crates/core/src/protocol.rs` | 533 |
| `tag_wrapper_matches_independent_python_sha256_vector` | Function | `crates/core/src/protocol.rs` | 606 |
| `write_canonical_preimage` | Function | `crates/core/src/protocol.rs` | 385 |
| `payload_sink` | Function | `crates/core/src/protocol.rs` | 411 |
| `analytic_corpus_tag` | Function | `crates/core/src/render.rs` | 2365 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Ledger_limits_and_stored_evidence_fail_closed → Name` | cross_community | 10 |
| `Ledger_limits_and_stored_evidence_fail_closed → Feed_sidebar_policy` | cross_community | 10 |
| `Ledger_refuses_missing_reordered_and_forked_edges_without_mutation → Name` | cross_community | 10 |
| `Ledger_refuses_missing_reordered_and_forked_edges_without_mutation → Feed_sidebar_policy` | cross_community | 10 |
| `Display_receipt_v1_outcomes_match_independent_python_vectors → Payload_sink` | cross_community | 9 |
| `Ledger_limits_and_stored_evidence_fail_closed → ContentTag` | cross_community | 9 |
| `Control_actions_are_visible_in_the_outcome_tag → Payload_sink` | cross_community | 9 |
| `Ledger_refuses_missing_reordered_and_forked_edges_without_mutation → ContentTag` | cross_community | 9 |
| `Canonical_decoder_rejects_prefixes_trailing_bytes_and_structural_mutations → Payload_sink` | cross_community | 9 |
| `Tolerance_boundary_is_inclusive_and_one_code_excess_refuses → Payload_sink` | cross_community | 9 |

## How to Explore

1. `context({name: "content_tag"})` — see callers and callees
2. `query({search_query: "cluster_74"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
