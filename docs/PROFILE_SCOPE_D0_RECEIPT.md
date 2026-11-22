# PROFILE-SCOPE-D0: pre-ID scope & non-self-referential bootstrap contracts receipt v1

This receipt resolves `PS-077:PROFILE-SCOPE-D0@base=BOUNDED` and unlocks
`PS-067:THREAT-1@base=READY`. Pre-verified `GENESIS-D1` (`PS-078`) and `PROTOCOL-ID-1`
(`PS-027`) wire codecs are included as early research artifacts and remain honestly `LOCKED`
until `THREAT-1` and `BOUNDS-1` pass.

## Provenance

- **Merged `origin/main` subject (PR #30):** commit `9e6780c4ddb0c2cf47440412634afad0690d66ab`
  (tree `6e1e243a57c6cb00ad89fa3e783d9c3255bb199d`, parent `7aef48be0f6c0990edfff2218fc1b87d583b1a50`,
  CI runs `45030235227` & `45030229935`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/profile_bootstrap.rs`](../crates/research/src/profile_bootstrap.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; default binary and `world_tag`
  untouched).
- **Independent Python reference suite:** [`tools/profile_bootstrap_vectors.py`](../tools/profile_bootstrap_vectors.py)
  (stdlib-only, zero Rust imports; `111/111` protocol schemas independently vectored).
- **CI integration & mutation evidence:** [`tests/wave3_bootstrap_and_language_r1.rs`](../tests/wave3_bootstrap_and_language_r1.rs)
  and `python3 tools/bbmutate.py profile_bootstrap` (`17/17` canonical big-endian sites KILLED, 100%).

## Verified golden tags & invariants

| Check | Pinned tag / behavior | Verification |
| --- | --- | --- |
| `ProfileScopeV1` (`bitblock/profile-scope-d0/scope/v1`) | `#bb.policy.sha256.yzu3porxqsiiivkwgepymeiuphc7er4wdl226g6go4kfaingn2eq` | Exact Rust + Python parity |
| `ChainGenesisPayloadV1` (`bitblock/genesis-d1/chain-genesis-payload/v1`, pre-verified contract; `GENESIS-D1` stays `LOCKED`) | `#bb.policy.sha256.q7pz74ykis4uuqina62xd6immkzczt4c77y6g4elrum5z37xdhcq` | Exact Rust + Python parity |
| `AcceptedProposalCoreV1` (`bitblock/protocol-id-1/accepted-proposal-core/v1`, pre-verified contract; `PROTOCOL-ID-1` stays `LOCKED`) | `#bb.receipt.sha256.7a5oxjjdyl5sfkec3va6ziw4q5mxomxmqkwhsfrkwuay6mmfr65q` | Exact Rust + Python parity |
| Canonical predicate registry & `PE-01..PE-10` entailment closure | Rejects reserved `always`, unregistered tokens, unsorted/duplicate lists, and missing entailed predicates (`stake-consensus` alone fails closed) | CI verified |
| Non-self-referential pre-ID graph (`verify_protocol_id_graph_acyclic`) | Accepts acyclic derivation DAG (`profile_scope_digest -> genesis_id -> profile_id -> proposal_core_id`); rejects self-reference or cycles with `IdentifierCycleDetected` | CI verified |
| Collision disposition (`classify_parent_collision`) | Equal core yields `DuplicateCredentialNormalized`; distinct core yields `DistinctCoreSafeHalt` | CI verified |
| Domain-bound signed transaction envelope (`verify_signed_transaction_envelope`) | Verifies strict RFC 8032 Ed25519 signature over `(chain_id, genesis_id, launch_manifest_id, profile_id, body_digest)`; rejects cross-genesis replay fail-closed | CI verified |

## Non-claims

A bounded pre-ID scope digest does not quantify attacker powers (`THREAT-1`), prove resource or
state-growth bounds (`BOUNDS-1`), unlock `GENESIS-D1` or `PROTOCOL-ID-1`, implement network
execution, or admit a network profile (`PROFILE-1`).
