---
name: gitnexus-area-cluster-146
description: "Skill for the Cluster_146 area of bitblock. 21 symbols across 2 files."
---

# Cluster_146

21 symbols | 2 files | Cohesion: 80%

## When to Use

- Working with code in `crates/`
- Understanding how encode_canonical, new_available, new_unavailable work
- Modifying cluster_146-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/research/src/output_color.rs` | encode, encode, encoded_len, validate, encode_canonical (+15) |
| `crates/research/src/output_topology.rs` | tag |

## Entry Points

Start here when exploring this area:

- **`encode_canonical`** (Function) — `crates/research/src/output_color.rs:254`
- **`new_available`** (Function) — `crates/research/src/output_color.rs:180`
- **`new_unavailable`** (Function) — `crates/research/src/output_color.rs:203`
- **`verify`** (Function) — `crates/research/src/output_color.rs:326`
- **`verify_against`** (Function) — `crates/research/src/output_color.rs:345`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `encode_canonical` | Function | `crates/research/src/output_color.rs` | 254 |
| `new_available` | Function | `crates/research/src/output_color.rs` | 180 |
| `new_unavailable` | Function | `crates/research/src/output_color.rs` | 203 |
| `verify` | Function | `crates/research/src/output_color.rs` | 326 |
| `verify_against` | Function | `crates/research/src/output_color.rs` | 345 |
| `tag` | Function | `crates/research/src/output_topology.rs` | 938 |
| `encode` | Function | `crates/research/src/output_color.rs` | 52 |
| `encode` | Function | `crates/research/src/output_color.rs` | 144 |
| `encoded_len` | Function | `crates/research/src/output_color.rs` | 118 |
| `validate` | Function | `crates/research/src/output_color.rs` | 102 |
| `from_facts` | Function | `crates/research/src/output_color.rs` | 220 |
| `recompute_tag` | Function | `crates/research/src/output_color.rs` | 373 |
| `new` | Function | `crates/research/src/output_color.rs` | 515 |
| `acquisition` | Function | `crates/research/src/output_color.rs` | 580 |
| `available_and_unavailable_round_trip_with_raw_words` | Function | `crates/research/src/output_color.rs` | 666 |
| `available_facts_require_matching_available_parent_routes` | Function | `crates/research/src/output_color.rs` | 700 |
| `available_parent` | Function | `crates/research/src/output_color.rs` | 656 |
| `malformed_and_noncanonical_bytes_are_refused` | Function | `crates/research/src/output_color.rs` | 752 |
| `parent_and_route_binding_fail_closed` | Function | `crates/research/src/output_color.rs` | 725 |
| `route` | Function | `crates/research/src/output_color.rs` | 615 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Available_and_unavailable_round_trip_with_raw_words → Encode` | intra_community | 4 |
| `Available_and_unavailable_round_trip_with_raw_words → Validate` | intra_community | 4 |
| `Available_and_unavailable_round_trip_with_raw_words → Kind` | cross_community | 4 |
| `Available_and_unavailable_round_trip_with_raw_words → Encoded_len` | intra_community | 3 |
| `Available_and_unavailable_round_trip_with_raw_words → New` | intra_community | 3 |

## How to Explore

1. `context({name: "encode_canonical"})` — see callers and callees
2. `query({search_query: "cluster_146"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
