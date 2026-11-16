---
name: gitnexus-area-cluster-163
description: "Skill for the Cluster_163 area of bitblock. 36 symbols across 2 files."
---

# Cluster_163

36 symbols | 2 files | Cohesion: 92%

## When to Use

- Working with code in `crates/`
- Understanding how encode_canonical, not_applicable, reported_samples work
- Modifying cluster_163-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/research/src/physical_sensor.rs` | build, encode_canonical, feed_payload, not_applicable, payload_matches (+30) |
| `crates/research/src/presentation_output.rs` | tag |

## Entry Points

Start here when exploring this area:

- **`encode_canonical`** (Function) — `crates/research/src/physical_sensor.rs:781`
- **`not_applicable`** (Function) — `crates/research/src/physical_sensor.rs:668`
- **`reported_samples`** (Function) — `crates/research/src/physical_sensor.rs:699`
- **`unavailable`** (Function) — `crates/research/src/physical_sensor.rs:681`
- **`verify`** (Function) — `crates/research/src/physical_sensor.rs:923`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `encode_canonical` | Function | `crates/research/src/physical_sensor.rs` | 781 |
| `not_applicable` | Function | `crates/research/src/physical_sensor.rs` | 668 |
| `reported_samples` | Function | `crates/research/src/physical_sensor.rs` | 699 |
| `unavailable` | Function | `crates/research/src/physical_sensor.rs` | 681 |
| `verify` | Function | `crates/research/src/physical_sensor.rs` | 923 |
| `verify_against` | Function | `crates/research/src/physical_sensor.rs` | 944 |
| `physical_sensor_policy_tag` | Function | `crates/research/src/physical_sensor.rs` | 61 |
| `tag` | Function | `crates/research/src/presentation_output.rs` | 67 |
| `build` | Function | `crates/research/src/physical_sensor.rs` | 716 |
| `feed_payload` | Function | `crates/research/src/physical_sensor.rs` | 1194 |
| `payload_matches` | Function | `crates/research/src/physical_sensor.rs` | 1233 |
| `recompute_tag` | Function | `crates/research/src/physical_sensor.rs` | 1181 |
| `validate_and_len` | Function | `crates/research/src/physical_sensor.rs` | 1086 |
| `new` | Function | `crates/research/src/physical_sensor.rs` | 1350 |
| `code` | Function | `crates/research/src/physical_sensor.rs` | 94 |
| `checked_mul` | Function | `crates/research/src/physical_sensor.rs` | 1331 |
| `bracket` | Function | `crates/research/src/physical_sensor.rs` | 1634 |
| `canonical_decoder_rejects_prefixes_trailing_bytes_and_structural_mutations` | Function | `crates/research/src/physical_sensor.rs` | 1943 |
| `challenge` | Function | `crates/research/src/physical_sensor.rs` | 1756 |
| `exact_samples` | Function | `crates/research/src/physical_sensor.rs` | 1764 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Tolerance_boundary_is_inclusive_and_one_code_excess_refuses → Code` | cross_community | 10 |
| `Tolerance_boundary_is_inclusive_and_one_code_excess_refuses → Digest` | cross_community | 10 |
| `Canonical_decoder_rejects_prefixes_trailing_bytes_and_structural_mutations → Payload_sink` | cross_community | 9 |
| `Tolerance_boundary_is_inclusive_and_one_code_excess_refuses → Payload_sink` | cross_community | 9 |
| `Canonical_decoder_rejects_prefixes_trailing_bytes_and_structural_mutations → Name` | cross_community | 8 |
| `Tolerance_boundary_is_inclusive_and_one_code_excess_refuses → Name` | cross_community | 8 |
| `Reported_samples_need_explicit_exact_parent_binding_before_they_match → Code` | cross_community | 7 |
| `Reported_samples_need_explicit_exact_parent_binding_before_they_match → Code` | cross_community | 7 |
| `Tolerance_boundary_is_inclusive_and_one_code_excess_refuses → ContentTag` | cross_community | 7 |
| `Canonical_decoder_rejects_prefixes_trailing_bytes_and_structural_mutations → Canonical_len` | cross_community | 6 |

## How to Explore

1. `context({name: "encode_canonical"})` — see callers and callees
2. `query({search_query: "cluster_163"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
