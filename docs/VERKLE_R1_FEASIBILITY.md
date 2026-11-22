# VERKLE-R1@feasibility: vector-commitment feasibility and deferral record v1

This record resolves `PS-017:VERKLE-R1@feasibility` as **BOUNDED** while keeping
`PS-016:VERKLE-R1@base=READY`, `PS-018:VERKLE-R1@candidate-scope=LOCKED`, and
`PS-019:VERKLE-R1@prototype=LOCKED`. It evaluates vector-commitment authenticated maps strictly as
a separate feasibility lane—never as “Candidate C” in the already bounded two-candidate
[`COMMIT_R1_DECISION.md`](COMMIT_R1_DECISION.md) hash-tree decision—and pins the self-excluding
`VectorCommitmentProfileDescriptorV1`, injective `VerkleCoordinateKeyV1`, comparative structural
and whole-witness measurements, rejected alternatives, and migration design.

The raw local release probe output is retained in
[`verkle-r1-feasibility-v1.json`](verkle-r1-feasibility-v1.json).

## Provenance and verified artifacts

- **Merged `origin/main` subject:** commit `2ee1e0c874a6849b67d14997cb35bc23e651b93c`
  (tree `734c0fa3565c3d3cb070caee48ef3d10199126e8`, PR #20, CI runs `44996624134` & `44996627977`,
  10/10 jobs green across Windows/Linux Rust, `wasm32` `+simd128` V8 parity, Python vectors,
  generated index, and Lean 4 `--trust=0` proof capsules).
- **Authoritative implementation & independent parity surfaces:**
  - Portable Rust codecs & deterministic arity-256 analyzer: `crates/research/src/verkle_feasibility.rs`
    (`wasm32`-clean; `bbmutate.py verkle_feasibility`: `9/9` sites KILLED, 100%).
  - Independent stdlib Python vector reconstruction & negative controls:
    `tools/verkle_feasibility_vectors.py` (`92/92` protocol schemas independently vectored).
  - Integration test suite (`13^2 = 169` signed boundary pairs, modulus headroom, non-aliasing leaf
    slots, profile mutation rejection, stem-sharing & witness-growth controls):
    `tests/verkle_r1_feasibility.rs`.
  - Reproducible release assay binary: `examples/verkle_r1_feasibility_probe.rs`.

## 1. Candidate primitive, setup/custody model, and rejected alternatives

| Candidate | Primitive & arity | Setup / custody | Security assumptions | Compressed point / opening floor | Disposition |
| --- | --- | --- | --- | --- | --- |
| **VC-1 (Primary Feasibility Subject)** | Bandersnatch twisted-Edwards prime-order subgroup Pedersen + arity-256 IPA (`#bb.policy.sha256.p74jrvkyumadcmam5hrp5cum4smpcciyng2yxfmsmvqbla2lepfq`) | Transparent hash-to-curve generator basis (`trusted_setup_required_u8 = 0`, no toxic waste) | Classical DLOG + Random Oracle Model + hash-to-curve linear-independence across all 256 basis points; **NOT post-quantum** | `32 B` commitment; `576 B` IPA multiproof floor (`16*32 + 32 + 32`); `273`-point MSM per verify | **FEASIBILITY SUBJECT — EXPLICITLY DEFERRED** (does not beat `COMMIT-1` Patricia on single/small queries or TCB) |
| **VC-2 (Rejected)** | BLS12-381 KZG arity-256 (`#bb.policy.sha256.tr7fdslagw7eotvfuoerwjfekutud5zh7jhns6v6zwn3uteey2mq`) | Updatable universal powers-of-tau ceremony (`trusted_setup_required_u8 = 1`) | q-SDH / AGM + bilinear pairing TCB + honest-participant ceremony assumption; **NOT post-quantum** | `48 B` G1 commitment (`1.5×` larger than VC-1); `48 B` single opening floor | **REJECTED**: imports toxic-waste ceremony custody risk, full pairing TCB, and `1.5×` larger per-stem commitment bytes (`41.6 KB` vs `31.1 KB` at `k = 256`) |
| **VC-3 (Rejected)** | Catalano–Fiore RSA / class-group Verkle (Kuszmaul 2018) | Hidden-order RSA setup or class-group parameters | Strong RSA / adaptive root assumption; **NOT post-quantum** | `256–384 B` group elements per commitment | **REJECTED**: excessive element size and audit/constant-time immaturity |
| **VC-4 (Deferred)** | Post-quantum lattice / FRI vector commitments | Transparent (`trusted_setup_required_u8 = 0`) | Module-SIS / hash-only FRI (post-quantum) | `~10–50 KB+` opening proofs | **REJECTED FOR v1 / DEFERRED**: opening proofs exceed `COMMIT-1` Patricia's `38–509 B` proofs by `20×–100×` and lack standardized audited Rust crates |

**Transparent-setup boundary (per `DESIGN_RESEARCH_DISTILLATION.md`):** VC-1's transparent
hash-to-curve generator basis eliminates ceremony toxic waste, but it does **not** remove curve,
hash-to-curve linear-independence (ruling out any nonzero linear relation across all 256 basis
points), implementation, or constant-time side-channel assumptions.

## 2. Field/curve, subgroup rules, coordinate embedding, and self-excluding descriptor

- **Scalar field modulus (`BANDERSNATCH_SCALAR_MODULUS_BE`):**
  `r = 13108968793781547619861935127046491459309155893440570251786403306729687672801` (253-bit prime,
  `0x1cfb69d4ca675f520cce760202687600ff8f87007419047174fd06b52876e7e1`).
- **Mandatory subgroup & encoding policy (`SubgroupCheckPolicyV1::RejectNonCanonicalOffCurveAndSmallSubgroup`):**
  every compressed 32-byte Edwards point MUST check canonical field coordinate (`< p`), on-curve
  equation, and prime-order subgroup membership (cofactor-4 small-subgroup rejection and identity
  rejection where forbidden) before any scalar multiplication or batch aggregation.
- **Exact injective 32-byte coordinate embedding (`bitblock/verkle-r1/coord-key-32b/v1`):**
  ```text
  VerkleCoordinateKeyV1(c) = 0x0000000000000000 || CoordinateKeyV1(c)
  stem_31b = bytes[0..31]    (248-bit slice with top 8 bytes zero -> strictly < 2^184 << r, 69 bits headroom)
  suffix   = bytes[31]       (arity-256 leaf slot index 0..=255)
  ```
  Non-32-byte slices, non-zero leading 8-byte prefixes, hashed coordinates, truncations, and
  alternate paddings reject fail-closed (`VerkleCoordinateKeyV1::from_bytes`). Because the maximum
  possible stem (`Coord(i64::MAX, i64::MAX, i64::MAX)`) is `< 2^184 << r_Bandersnatch`, zero
  modular reduction or bit-truncation aliasing can occur.
- **Present-vs-absent leaf slot encoding (`VerkleLeafSlotEncodingV1`):**
  `ABSENT_SLOT = [0u8; 32]`; `present_slot(value_lo) = 2^128 | value_lo` (bit 128 set in byte 15,
  strictly `< 2^129 << r`). Even a present zero-valued payload (`present_slot(0)`) never aliases
  an absent slot (`ABSENT_SLOT`).
- **Self-excluding `VectorCommitmentProfileDescriptorV1` (`bitblock/verkle-r1/vector-commitment-profile/v1`, `247` bytes):**
  derives `VectorCommitmentProfileIdV1` via `TagKind::Policy` with zero dependence on itself,
  `genesis_id`, `candidate_id`, `profile_id`, or any produced root, ready for consumption by a
  later `DomainRegistryV1`. Golden Rust == Python tags:
  - VC-1 (`bandersnatch_ipa_256_v1`): `#bb.policy.sha256.p74jrvkyumadcmam5hrp5cum4smpcciyng2yxfmsmvqbla2lepfq`
  - VC-2 (`bls12_381_kzg_256_rejected_v1`): `#bb.policy.sha256.tr7fdslagw7eotvfuoerwjfekutud5zh7jhns6v6zwn3uteey2mq`

## 3. Comparative measurements across the 7 `COMMIT-R1` worlds (`verkle_r1_feasibility_probe`)

All 7 worlds match [`COMMIT_R1_DECISION.md`](COMMIT_R1_DECISION.md) byte-for-byte. Verkle whole-
witness byte sizes honestly include the fixed opening floor (`576 B` IPA / `48 B` KZG), **all**
deduplicated non-root internal commitments, stem + `C1`/`C2` commitments (`2 * comm_bytes` per
unique touched stem), opened stems + suffixes + values, and `16 B` witness framing. Even under the
most favorable path-compressed arity-256 model:

| Workload | Keys | Patricia logical / CAS nodes | `COMMIT-1` single proof (B) | `COMMIT-1` verify p50 (µs) | Verkle stems / int nodes | Verkle IPA 1-key whole (B) | Verkle KZG 1-key whole (B) | Patricia 8-disp MP (B) | Verkle IPA 8-disp whole (B) | Verkle 1-edit scalar muls |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `SingleOrigin` | 1 | 1 / 1 | 38 | 3.74 | 1 / 0 | 689 | 193 | 28 | 689 | 2 |
| `DenseCube16` | 4,096 | 8,464 / 16 | 509 | 14.58 | 256 / 17 | 721 | 241 | 552 | 1,624 | 4 |
| `HollowShell16` | 1,352 | 2,976 / 35 | 509 | 14.58 | 256 / 17 | 721 | 241 | 1,232 | 1,592 | 4 |
| `NegativeChunkCluster` | 9 | 24 / 12 | 214 | 8.80 | 9 / 8 | 785 | 337 | 297 | 1,592 | 6 |
| `SparseFarExtrema` | 9 | 24 / 12 | 121 | 6.00 | 9 / 8 | 721 | 241 | 297 | 1,592 | 4 |
| `Clustered1000` | 1,000 | 2,221 / 19 | 509 | 14.58 | 100 / 11 | 721 | 241 | 761 | 1,624 | 4 |
| `Dispersed1000` | 1,000 | 2,264 / 2,264 | 480 | 12.72 | 1,000 / 251 | 753 | 289 | 2,215 | 1,688 | 5 |

### Dispersed batch crossover on `Dispersed1000` (1,000 keys across 1,000 distinct chunks)

| Opened dispersed keys (`k`) | Patricia multiproof (B) | Verkle IPA-256 whole witness (B) | Verkle KZG-256 whole witness (B) | Smaller witness |
| ---: | ---: | ---: | ---: | --- |
| `1` | **280** | 753 | 289 | **Patricia (`2.69×` smaller than IPA)** |
| `8` | 2,215 | **1,688** | 1,576 | Verkle IPA (`1.31×` smaller) |
| `32` | 6,921 | **4,816** | 5,872 | Verkle IPA (`1.44×` smaller) |
| `64` | 10,910 | **8,944** | 11,536 | Verkle IPA (`1.22×` smaller) |
| `128` | **15,908** | 17,104 | 22,720 | **Patricia** (shared binary hashes beat per-stem `95 B` cost) |
| `256` | **20,603** | 31,088 | 41,584 | **Patricia** (refutes constant `1–2 KB` whole-witness claim!) |

### End-to-end cost, TCB, and implementation/audit maturity summary

1. **Single-point & small-query dominance of `COMMIT-1` Patricia:** Across all 7 workloads,
   `COMMIT-1` binary Patricia produces `38–509 B` single proofs verifying in `3.74–14.58 µs` using
   only `sha2` (`0` curve crates, `0` elliptic-curve side-channel surface, `0` subgroup bugs,
   post-quantum hash collision/preimage hardness). Verkle IPA-256 has a `689–785 B` minimum whole
   witness and requires a `273`-point Bandersnatch MSM (~`1–5 ms` native / ~`10–30 ms` WASM —
   **`100×–1,000×` slower** per verification!).
2. **Whole-witness scaling refutes the `1–2 KB` myth:** At `k = 256` dispersed stems, even
   path-compressed Verkle IPA whole witnesses reach **31.1 KB** because opened stems and `stem +
   C1/C2` commitments scale linearly (`~95 B/stem`). Verkle only wins materially on **large
   multi-million-key worlds** where binary Patricia depth approaches `20–30+` unshared SHA-256
   siblings (`~640–1,000 B/key`) across `32–1,000+` dispersed keys per stateless block.
3. **Storage & availability obligations:** Switching to Verkle does **not** eliminate full-state
   witness producers, archival availability, persistent node-store growth, or pin-epoch GC.

## 4. Migration, historical verification, and reversal design (for any future `verkle-selected` profile)

Before `VERKLE-R1@prototype` can ever unlock under CE-35–CE-42:
- **Typed root separation:** a Verkle world root MUST carry a distinct typed root schema
  (`bitblock/verkle-r1/world-root-candidate/v1`) and `VectorCommitmentProfileIdV1` inside
  `DomainRegistryV1`; it may never alias or reuse `COMMIT-1`'s `WorldRootV1` or legacy `world_tag`.
- **Deterministic conversion & overlay:** full-state migration under `UPGRADE-1` must specify
  bounded offline or per-block deterministic conversion (`CoordinateKeyV1 -> VerkleCoordinateKeyV1`),
  dual-root parity checks at the activation boundary, crash/reorg rollback to the pre-activation
  `WorldRootV1` head, and mandatory downgrade rejection once activated.
- **Historical verification & reversal:** historical pre-activation blocks retain `COMMIT-1`
  Patricia verification permanently; if Bandersnatch/DLOG or post-quantum assumptions become
  unacceptable, a successor `UPGRADE-1` profile migrates deterministically back to a hash-based
  `WorldRootV2` (or post-quantum commitment) from the canonical key/value state.

## 5. Feasibility decision and explicit non-claims

- **Decision:** **DEFER** prototype implementation for local and early profiles.
  `PS-017:VERKLE-R1@feasibility` moves to **BOUNDED**; `PS-018:VERKLE-R1@candidate-scope` and
  `PS-019:VERKLE-R1@prototype` remain **LOCKED** behind CE-35–CE-42 (`CRYPTO-D1`, `CODEC-1`,
  `THREAT-1`, `BOUNDS-1`, `UPGRADE-FRAMEWORK-1`, `PROFILE-D1`).
- **Non-claims:** This report does **not** implement elliptic-curve arithmetic, Bandersnatch
  Pedersen commitments, or IPA proof generation/verification; does **not** reopen the closed
  `COMMIT-R1` two-candidate hash-tree decision; and does **not** assert that Verkle is either
  cryptographically proven secure or globally inferior at multi-million-key stateless L1 scale.
