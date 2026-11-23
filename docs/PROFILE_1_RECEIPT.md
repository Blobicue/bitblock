# PROFILE-1@base: admitted activated-feature closure receipt v1

This receipt resolves `PS-084:PROFILE-1@base=BOUNDED`, unlocking
**`PS-089:RELEASE-1@base=READY`**, **`PS-090:RELEASE-1@admitted=READY`**, and
**`PS-083:UPGRADE-1@base=READY`** while keeping `CHAIN-LAUNCH-1`, `TESTNET-1`,
`SUCCESSOR-ACTIVATION-1`, and `PROD-OPS-1` strictly `LOCKED` on their remaining prerequisites.

## Provenance

- **Merged `origin/main` subject (PR #92):** commit `32fe68a196e8f239cbfd146669320d718cd095c3`
  (tree `8e445aae45ba90a4b010ee180452580612dd41c8`, parent `8037cc2c4d309f28dcd92f911bb187e3a5ebf7e9`,
  CI runs `45249475161` & `45249481578`, 10/10 jobs green across Linux, Windows, `wasm32` V8
  parity, Python vectors, generated index, and Lean 4 `--trust=0`).
- **Executable module:** [`crates/research/src/profile_1.rs`](../crates/research/src/profile_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`, zero new Cargo dependencies,
  zero edits to frozen `genesis_d1.rs`, `profile_d1.rs`, or `crates/core/**`).
- **Clean-room pure-Python verifier:**
  [`tools/profile_1_vectors.py`](../tools/profile_1_vectors.py) (`182/182` protocol schemas
  independently vectored; `0` without vectors).
- **CI & mutation evidence:**
  [`tests/wave28_profile_1.rs`](../tests/wave28_profile_1.rs) and
  `python3 tools/bbmutate.py profile_1` (`10/10` canonical big-endian sites KILLED, 100%).
- **Release probe:**
  [`examples/profile_1_admission_probe.rs`](../examples/profile_1_admission_probe.rs).

## Verified golden tags & normative invariants

Both Rust (`ProfileClosureEvidenceSetV1`, `ProfileAdmissionManifestV1`,
`PredicateEnvironmentV1`, `admit_candidate_profile`, `verify_role_aware_closure_acyclic`) and
clean-room pure Python (`tools/profile_1_vectors.py`) match bit-for-bit across both golden
artifacts for `bitblock.devnet.v1`:

- **`ProfileClosureEvidenceSetV1` (`bitblock/profile-1/evidence-closure/v1`, `2,201` bytes):**
  `#bb.policy.sha256.rdkfncmp3sn6smjeyud6xb3xac35i44ihsl5piicglx3p6pydwga` (maps all 10 active
  launch capabilities — 7 unconditional base prerequisites `BOUNDS-1`, `EXECUTION-ROOT-1`,
  `GENESIS-ROOT-PARITY-1`, `GOVERNANCE-1`, `PROFILE-D1`, `TRANSITION-SPEC-1`,
  `UPGRADE-FRAMEWORK-1`, plus target-context `CE-25` `ASSET-1`, `CE-27` `RESOURCE-1`, and `CE-28`
  `VALIDATOR-ECON-1` — to their exact merged `origin/main` 20-byte Git commits, golden receipt
  digests, assumptions digests, loss-condition digests, 5 resolved conditional edges, 8 domain
  slots, and 7 reconciled stage×gate subclaims).
- **`ProfileAdmissionManifestV1` (`bitblock/profile-admission-manifest/v1`, `353` bytes):**
  `#bb.receipt.sha256.nmcfo64s3rjesde2eumymbeemqggiqvlcsnvlcpx6ggxm4meynjq` (protocol-frozen
  `profile_admission_manifest_id` binding `admitted_profile_id`,
  `admitted_transition_spec_at_profile_id`, `admitted_profile_scope_digest`,
  `admitted_domain_registry_digest`, `admission_policy_id`, sorted unique
  `required_capability_ids`, `evidence_closure_digest`, and sole passing disposition
  `AdmittedBoundedV1`).
- **`InitialLaunch` vs `SuccessorCandidate` `CE-29`/`G19.upgrade` boundary:** Frozen `GENESIS-D1`
  and `PROFILE-D1` carry `in-place-upgrade` and `migration-profile` in
  `CANONICAL_LAUNCH_PREDICATES_V1` with `predecessor_profile_id = None`. On `InitialLaunch`,
  `PROFILE-1` explicitly sets `initial_launch_upgrade_framework_scoped = true`, binds `CE-29` to
  `UPGRADE-FRAMEWORK-1` readiness, keeps `G19.upgrade` explicitly `EvidencePending`, and forbids
  fabricated `UPGRADE-1` bindings. On every `SuccessorCandidate`, `PROFILE-1` strictly requires
  concrete `UPGRADE-1@old->new` migration evidence with verified polarity (`old != new`).
- **10/10 adversarial fail-closed negative controls (`tests/wave28_profile_1.rs`):** missing
  unconditional prerequisite, wrong-version/pending evidence, `CE-26` `program-active = true`
  missing `PROGRAM-CONSENSUS`, `CE-29` `SuccessorCandidate` missing `UPGRADE-1`, inverted
  `CE-29`/`CE-30`/`CE-31` polarity & fabricated `InitialLaunch` migration binding, `CE-42`
  `verkle-selected = true` with mismatched `CE-40` candidate ID, inactive-domain non-zero
  commitment-profile forgery, Kahn role-aware dependency cycle, incomplete `PE-01`..`PE-10`
  predicate entailment, and wire-tampered `Rejected` admission disposition all fail closed.
- **Merged-head release timings (`32fe68a`):** full 10-node closure admission `60.74 us`,
  canonical closure & manifest encode/decode/tag `13.31 us`, role-cycle & rejected-disposition
  fail-closed rejection `2.10 us`.
- **Visual verification:** `profile_1_admission_chart.png` rendered on merged `32fe68a` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: profile admission does not prove properties outside the activated closure,
sign an admitted release (`RELEASE-1`), authorize launch (`CHAIN-LAUNCH-1`), establish live
in-place migration (`UPGRADE-1` / `SUCCESSOR-ACTIVATION-1` remain separate gates), or prove
runtime or cryptographic security.
