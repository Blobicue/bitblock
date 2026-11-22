# TRANSITION-SPEC-D1: base occupancy/episode transition contract v1

This specification resolves `PS-014:TRANSITION-SPEC-D1@base` as **BOUNDED** and unlocks
`PS-020:COMMIT-1@base` as **READY**. It freezes the base occupancy/episode state transition
contract and the typed extension interface consumed by later `PROFILE-SCOPE-D0` and
`TRANSITION-SPEC-1` phases, while explicitly leaving nonce, fee, governance, validator, asset, and
visual-program policy slots unfilled.

## Provenance and executable artifacts

- **Merged `origin/main` subject:** commit `58cefcb9e9b4ae24cb90861571489560266d6b23`
  (tree `2166e50bcce90d14ea80abec8b2190dd9ba6b93c`, parent
  `77e3bb35e93ef4418c63266d92994fbee0ed7eb3`, PR #14).
- **Exact-head CI evidence:** GitHub Actions runs `44978821303` and `44978824816` (10/10 jobs green
  across Windows/Linux Rust, `wasm32` `+simd128` V8 parity, Python vector tools, generated index,
  and Lean 4 `--trust=0`).
- **Authoritative Rust module:** `crates/research/src/transition_spec.rs` (portable across native
  and `wasm32`; `bbmutate.py transition_spec`: 19/19 canonical byte-order sites KILLED).
- **Independent stdlib Python verifier:** `tools/transition_spec_d1_vectors.py`.
- **Integration test suite:** `tests/transition_spec_d1.rs`.

## Canonical schemas and pinned tags

| Schema | Type | Wire shape | Pinned canonical tag |
| --- | --- | --- | --- |
| `bitblock/transition-spec-d1/base-contract/v1` | `BaseTransitionSpecD1` | `44 + 32 * slot_count` bytes big-endian (`spec_version=1`, `max_batch_ops=65536`, `base_work_per_edit=1`, `bedrock=(0,0,0)`, `base_domains_mask=0x7`, `slot_count`, sorted slots) | `#bb.policy.sha256.pesnqo27cmnpc4q2jpudlfm5mmnhysue2tmstbyc7f77dkvus5na` (base-only) |
| `bitblock/transition-spec-d1/extension-slot/v1` | `ExtensionSlotDescriptorV1` | Fixed 32 bytes (`slot_id`, `op_tag_min..=max`, `disposition_tag_min..=max`, `owned_domain_bit`, `conflict_policy`, `read_domains_mask`, `write_domains_mask`, `base_work_units`, `per_item_work_units`) | `#bb.policy.sha256.kxlgpvzyz3bcvdbpzvlda4yiehaph2rae2qsgrqx7dpibzz4y6ya` (reference slot 1) |
| `bitblock/transition-spec-d1/base-receipt/v1` | `BaseTransitionReceiptV1` | `180 + disposition_count` bytes (`spec_tag`, `tick`, `pre_world`, `pre_history`, `post_world`, `post_history`, `work_units`, `disposition_count`, ordered `BaseDispositionV1` bytes) | Verified per batch via Rust + Python reconstruction |

## Base substrate semantics

1. **Genesis & immutable bedrock:** `World::new()` contains exactly origin bedrock `(0, 0, 0)`.
   Bedrock is permanently occupied and excluded from the mutable active-episode map (`LEDGER-1`).
2. **Base operations (`0..=15` reserved):** `0 = On`, `1 = Off`, `2 = Toggle`; tags `3..=15` are
   reserved for future base substrate revisions and rejected for extensions.
3. **Complete base dispositions (`0..=15` reserved):** Every submitted edit in an accepted batch
   produces exactly one ordered `BaseDispositionV1`: `Mint (0)`, `Burn (1)`, `NoOpBedrock (2)`,
   `NoOpAlreadyOn (3)`, `NoOpAlreadyOff (4)`, `RejectedOverLimit (5)`, or `RejectedMalformed (6)`.
   Omitting a disposition lane fails closed and changes the receipt tag.
4. **Atomicity & zero partial writes:** `execute_base_transition_bytes` validates `spec_version`,
   checks `count <= max_batch_ops` before allocation, decodes every edit, stages mutations onto
   isolated working copies, verifies `OccupancyEpisodeLedger::verify_world_bijection` and
   `verify_consistency`, and commits into caller state only after all invariants pass. Any wrong
   version, malformed edit, or over-limit batch leaves `(world, history, episodes)` byte-identical.
5. **Base state-domain ownership:** Bits `0` (`world-occupancy`), `1` (`active-episode`), and `2`
   (`episode-history`) are owned exclusively by the base substrate (`BASE_STATE_DOMAINS_MASK = 0x7`).

## Typed extension interface rules

Every `ExtensionSlotDescriptorV1` admitted into `ExtensionInterfaceRegistryV1` must satisfy:
- unique non-zero `slot_id` in strictly ascending canonical order;
- disjoint `op_tag_min..=op_tag_max` strictly `>= 16`;
- disjoint `disposition_tag_min..=disposition_tag_max` strictly `>= 16`;
- unique `owned_domain_bit` in `3..=31`;
- `write_domains_mask` strictly a subset of `1 << owned_domain_bit` (forbids writing base domains
  `0..=2` or any domain bit owned by another slot — undeclared state access fails closed);
- non-zero `base_work_units` and non-overflowing `base_work_units + per_item_work_units`;
- explicit `ConflictPolicy` (`RejectConcurrentWrite = 0` or `DeterministicOrder = 1`).

## Non-claims

This base contract does **not** fill nonce, fee, governance, validator-schedule, asset, or VM
policy slots (those remain locked until `TRANSITION-SPEC-1@profile`), does not wire authenticated
trees into live `World` (`COMMIT-1`), does not prove Lean 4 refinement of Rust code, and does not
establish consensus or network execution authority.
