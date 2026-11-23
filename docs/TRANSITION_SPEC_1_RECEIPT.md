# TRANSITION-SPEC-1: profile-bound total transition specification receipt v1

This receipt resolves `PS-080:TRANSITION-SPEC-1@base=BOUNDED` (keeping production execution
downstream in `EXECUTOR-1`) and unlocks **`PS-048:PROGRAM-STATE-1@base=READY`** (while keeping
`EXECUTOR-1`, `GOVERNANCE-1`, `ASSET-1`, and `VALIDATOR-SCHEDULE-1` honestly `LOCKED` on
`AUTH-COMMIT-1`).

## Provenance

- **Merged `origin/main` subject (PR #54):** commit `27ede9a7c2b71930a8434f0c8744270134139d7f`
  (tree `5970eb2678d2fe2e0e1017972ac87c50022e9f7e`, parent `71f3b745ca05d48165eb90dddeba9a040a1c9baf`,
  CI runs `45104161636` & `45104166785`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/transition_spec_1.rs`](../crates/research/src/transition_spec_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero changes to `crates/core/**`,
  default binary, `world_tag`, `crates/wasm/www/*`, or `Cargo.lock`).
- **Independent Python reference suite:** [`tools/transition_spec_1_vectors.py`](../tools/transition_spec_1_vectors.py)
  (`144/144` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave12c_transition_spec_1.rs`](../tests/wave12c_transition_spec_1.rs)
  and `python3 tools/bbmutate.py transition_spec_1` (`23/23` canonical big-endian sites KILLED, 100%).
- **Release probe:** [`examples/transition_spec_1_probe.rs`](../examples/transition_spec_1_probe.rs).

## Verified golden tags & normative invariants

- **Profile-bound resolved transition specifications (`ResolvedProfileTransitionSpecV1`,
  `bitblock/transition-spec-1/resolved-profile-spec/v1`):**
  - Initial launch profile (`profile_id` prefix `8f1631795bdb`, `program-active = INACTIVE`):
    `#bb.policy.sha256.xhy32kud2wdzknglo6p2n3gulbnlikjn3eqhmmquzl2jcofjg6wa`.
  - `CE-43`/`CE-11` successor profile (`profile_id` prefix `86e5747b9541`, `program-active = ACTIVE`):
    `#bb.policy.sha256.ny3mlc3tk7f7ebqexdcz3azhleksz554tgoh2rrqohjp3bx6ldta`.
- **Disjoint 256-tag bands & exclusive write-domain ownership:**
  - `0..=15` Base Substrate (bits `0..=2`), `16..=31` Auth/Nonce (bits `3..=4`), `32..=47`
    Asset/Resource (bits `5..=6`), `48..=63` Validator/Schedule (bits `7..=8`), `64..=79`
    Gov/Upgrade (bit `9`), `80..=95` Program/VM (bit `10`: **INACTIVE wall** on launch, **ACTIVE**
    on successor), `96..=255` Unassigned (rejected fail-closed with `RejectedUnknownOperationTag`).
- **Total byte-to-disposition spec evaluation (`ProfileTransitionEvalReceiptV1`,
  `bitblock/transition-spec-1/spec-eval-receipt/v1`):**
  - Initial launch 4-op batch (`88,144` gas, `Admitted`):
    `#bb.receipt.sha256.ekrbspma4yaxej5loltysz3bznmy3ksyvqi3aaavui2fjvvjr3tq`.
  - Successor Program/VM op `80` batch (`50,128` gas, `Admitted`):
    `#bb.receipt.sha256.ozw6nmwf2si4ukpj25l6ppiacte5xfiwd7npftacave272nzissq`.
- **7/7 powered semantic mutants caught fail-closed:** op-tag band collision
  (`OpTagBandCollision`), write-domain overlap (`WriteDomainOverlap`), omitted-effect receipt
  tamper (`OmittedEffectDetected`), partial-write rollback leak (`PartialWriteRollbackViolation`),
  wrong version/profile (`RejectedWrongVersionOrProfile`), uncharged work `gas == 0`
  (`UnchargedWorkFormula`), and inactive Program/VM op `80` on launch
  (`RejectedInactiveExtensionSlot`).
- **Visual verification:** `transition_spec_1_bands_chart.png` rendered on merged `27ede9a` was
  directly inspected via `read_file`.

## Non-claims

`BOUNDED` is mandatory: an exact transition specification is not an authoritative or independent
executor (`EXECUTOR-1`), a commitment/root, profile admission (`PROFILE-1`), networking,
consensus, or a proof of implementation correctness.
