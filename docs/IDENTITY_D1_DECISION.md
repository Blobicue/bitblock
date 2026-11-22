# IDENTITY-D1: actor identity, authorization, and wire action decision v1

This decision record resolves `PS-026:IDENTITY-D1@base=BOUNDED`. It pins the pre-genesis actor
identifier derivation, role-separated authorization descriptor schema, delegation-cycle ban,
sequential nonce model, and explicit `On`/`Off` network transaction batch contract.

## Provenance

- **Merged `origin/main` subject (PR #26):** commit `ac503dce78ffb3951ea489ddb1d2cc35f4a1f178`
  (tree `950e59927c847833892b9b2debfe64b276227ae3`, CI runs `45009961375` & `45009964425`).
- **Executable contract:** [`crates/research/src/identity_d1.rs`](../crates/research/src/identity_d1.rs).
- **Independent Python vectors:** [`tools/pre_genesis_d1_vectors.py`](../tools/pre_genesis_d1_vectors.py).
- **CI integration & mutation evidence:** [`tests/pre_genesis_d1_contracts.rs`](../tests/pre_genesis_d1_contracts.rs)
  and `python3 tools/bbmutate.py identity_d1` (`16/16` sites KILLED).

## Selected design

1. **Stable pre-genesis `ActorIdV1`:** derived via SHA-256 over `bitblock/actor-identity/v1`,
   `chain_id`, bounded UTF-8 `identity_namespace`, and the initial canonical
   `AuthorizationDescriptorV1` (`#bb.policy.sha256.ewnhqdwxbhpuvwzeghndqdsssfsaz7u26xgwbxnporebo5poqd3q`,
   fixture `actor_id = c830de027d46f85cbf460a219bd3098e2452476e30b2e598a25743c9a94b30de`), needing
   zero `genesis_id` input and eliminating genesis identifier cycles.
2. **Role-separated `ThresholdPolicyV1` and `AuthorizationDescriptorV1`:** separates 7 explicit
   `CredentialRoleV1` roles (`ActorAction`, `ActorAdmin`, `ValidatorVote`, `ValidatorProposal`,
   `GovernanceSigner`, `EmergencyPauseSigner`, `ReleaseSigner`), enforces `1 <= threshold <= keys <= 32`
   with strictly sorted unique canonical keys, supports in-place admin-authorized rotation and
   terminal revocation while preserving `ActorIdV1`, and bans self-delegation and directed
   delegation cycles via 3-color DFS (`validate_actor_descriptor_set`).
3. **Bounded atomic explicit `On`/`Off` batches (`TransactionBodyV1`):** lowers `toggle` out of the
   network wire format (preventing race-condition accidental burns), enforces `1..=256` explicit
   `NetworkEditOpV1::{On, Off}` operations, sequential per-actor `ActorNonceV1`, height window
   `valid_after <= valid_until`, optional `expected_state_root` precondition, and `max_fee`
   (fixture tag `#bb.intent.sha256.foomjcxk5bnt4xzqhmt4zhwxa7hkdq37ykbkijliwasy4gwymela`).
4. **Local-only observer pose:** camera position, raycast aim, and reach remain strictly local
   presentation state and never enter consensus transactions.

## Rejected alternatives

- **Stateless public-key-hash addresses:** rejected because key rotation forces asset/permission
  migration across addresses rather than preserving stable identity.
- **Single-op-only transactions or retaining wire `Toggle`:** rejected due to multi-block edit
  non-atomicity and concurrent race-condition burn hazards.
- **Consensus-committed observer camera/reach:** rejected because client camera floats must never
  become consensus state authority.

## Falsifiers & non-claims

- **Smallest falsifiers:** accepting a wire `Toggle` discriminant (`2`), accepting a delegation
  cycle or cross-role key, accepting a non-sequential nonce, or allowing a revoked descriptor to
  rotate.
- **Non-claim:** a frozen schema does not authenticate live network traffic, prove human
  uniqueness, or implement `IDENTITY-1` / `AUTH-COMMIT-1` (both remain `LOCKED`).
