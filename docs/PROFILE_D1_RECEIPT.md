# PROFILE-D1: ID-bound candidate activated-feature manifest receipt v1

This receipt resolves `PS-079:PROFILE-D1@base=BOUNDED` and unlocks both
**`PS-080:TRANSITION-SPEC-1@base=READY`** and **`PS-088:RELEASE-R1@candidate=READY`** (`PD-05`,
alongside previously bounded `PD-04` `RELEASE-R1@framework`).

## Provenance

- **Merged `origin/main` subject (PR #50):** commit `7a710fc26abf100c6e4e3f8ddb33058227127f84`
  (tree `dbabdefaa94519ad0fb1fb5483825ae5ec70569a`, parent `9f5f6415aa58581aef1aa86130931a6967f217e3`,
  CI runs `45098940698` & `45098944665`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/profile_d1.rs`](../crates/research/src/profile_d1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero changes to `crates/core/**`,
  default binary, `world_tag`, `crates/wasm/www/*`, or `Cargo.lock`).
- **Independent Python reference suite:** [`tools/profile_d1_vectors.py`](../tools/profile_d1_vectors.py)
  (`140/140` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave11_profile_d1.rs`](../tests/wave11_profile_d1.rs) and
  `python3 tools/bbmutate.py profile_d1` (`8/8` canonical big-endian sites KILLED, 100%).
- **Release probe & manifest JSON:** [`examples/profile_d1_manifest_probe.rs`](../examples/profile_d1_manifest_probe.rs)
  and [`profile-d1-manifest-v1.json`](profile-d1-manifest-v1.json).

## Verified golden tags & normative invariants

- **Initial launch candidate (`CandidateProfileManifestV1` / `BoundProfileManifestV1`):**
  - Pre-ID candidate tag (`bitblock/profile-d1/candidate-manifest/v1`):
    `#bb.policy.sha256.r4ldc6k33p7evc4zg67ksajcpr6dxeiwuazimy6pxhwgaepepaoq`
    (derived `profile_id` prefix `8f1631795bdb`, immutable `genesis_id` prefix `4bac9bfad840`).
  - Bound manifest tag (`bitblock/profile-d1/bound-profile-manifest/v1`):
    `#bb.receipt.sha256.24biozgdf4dc7n4z5llunavtkegk433r5qfh4plknnjgtvw4naoq`.
  - Freezes 7 launch predicates closed under `PE-01`..`PE-10` with `program-active` and
    `verkle-selected` explicitly **INACTIVE**, `predecessor_profile_id = None`,
    `current_scope_digest == genesis_scope_digest`, and 6 active / 2 inactive domain slots.
- **CE-43 successor candidate (`+program-active`, `+post-genesis-program`):**
  - Pre-ID candidate tag:
    `#bb.policy.sha256.q3sxi64vigpwbcm2yhh4jgtwm7dfxvsva7k4phdjsswbeqeje6ha`
    (derived `profile_id` prefix `86e5747b9541`, same immutable `genesis_id` prefix `4bac9bfad840`).
  - Bound manifest tag:
    `#bb.receipt.sha256.6sik5vhte62uajm53n43axcz3lylfpiindawhjvried475wxxxka`.
  - Chains `predecessor_profile_id = Some(launch.profile_id)` with updated `current_scope_digest`,
    9 active predicates, and 7 active / 1 inactive domain slots (`verkle-selected` still inactive).
- **Non-circularity & stage×gate subclaim matrix:** candidate bytes exclude their own derived
  `profile_id`; self-reference, predicate closure omissions, and lineage violations reject
  fail-closed. All 7 stage×gate subclaim rows (`G5.core`, `G5.asset`, `G15.resource`,
  `G15.economic`, `G19.program`, `G19.upgrade`, `G19.verkle`) are deterministically projected with
  explicit `Satisfied` vs `EvidencePending` reporting.
- **Visual verification:** `profile_d1_manifest_chart.png` rendered on merged `7a710fc` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: a frozen candidate manifest is not an admitted `PROFILE-1` profile and does
not implement, verify, or approve any feature or unresolved conditional it names.
