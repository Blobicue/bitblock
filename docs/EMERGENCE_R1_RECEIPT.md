# EMERGENCE-R1: bounded falsifiable emergence assay receipt v1

This receipt resolves `PS-049:EMERGENCE-R1@base=BOUNDED`, completing Wave 6 of the
pre-launch research track over `RULE-1` (`B5/S45`).

## Provenance

- **Merged `origin/main` subject (PR #36):** commit `f0c428e7fcd01aaa0a9701aeb0f2137228afc51c`
  (tree `23efdc31ec9860595ee502c374d7554975930490`, parent `677364fcbb1a38efdc479357ff7e7c0994852eaf`,
  CI runs `45038296661` & `45038301971`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/emergence_r1.rs`](../crates/research/src/emergence_r1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero changes to `bitblock-core`
  or `world_tag`).
- **Independent Python reference executor:** [`tools/emergence_r1_vectors.py`](../tools/emergence_r1_vectors.py)
  (independently reproduces all 15 trajectories and matches both golden content tags bit-for-bit;
  `120/120` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave6_emergence_r1.rs`](../tests/wave6_emergence_r1.rs) and
  `python3 tools/bbmutate.py emergence_r1` (`19/19` canonical big-endian sites KILLED, 100%).
- **3D visual probe & JSON receipt:** [`examples/emergence_r1_visual_probe.rs`](../examples/emergence_r1_visual_probe.rs)
  and [`emergence-r1-assay-v1.json`](emergence-r1-assay-v1.json).

## Verified golden tags & frozen corpus

- **`EmergenceCorpusSpecV1` (`bitblock/emergence-r1/corpus-spec/v1`):**
  `#bb.policy.sha256.5cby7wdtezi5y64vo6pcsjt5kdcurkmezxnjsq5oidbavlca4mza`
  (3 engineered `B5/S45` structures + 4 `SplitMix64` matched-density `20%` soups in `[-4, 3]^3`
  evaluated across 3 3D Moore rules for `20` ticks under a `10,000`-cell safety cap; permanent
  bedrock `(0, 0, 0) = 1` excluded from mutable counts).
- **`EmergenceAssayReportV1` (`bitblock/emergence-r1/assay-report/v1`):**
  `#bb.receipt.sha256.54o2gvb4dxly2gxewna3zzvw6ufdtmiiveyc6msducfjbstadzca`
  (exact bit-for-bit parity between Rust `run_emergence_assay` and independent Python
  `tools/emergence_r1_vectors.py`).

## Predeclared 4-metric assay results (`f0c428e`)

| Cohort | Rule | Mean survival ticks (/20) | Mean final live cells | Mean 1-bit Hamming divergence | Notable structure / transport metrics |
| --- | --- | ---: | ---: | ---: | --- |
| **Subcritical control** (4 soups) | `B6/S45` | `5.5` | `0.0` | `0.0` | All 4 seeds die out by `t <= 7` (rapid extinction) |
| **Candidate `RULE-1` soups** (4 soups) | `B5/S45` | `17.5` | `12.0` (`48` on Seed 1) | `16.8` (`61` on Seed 1, `6` on Seed 4) | Seed 1 survives 20 ticks at `48` cells (`10.96%` Morton ratio, `5.57`-voxel L1 centroid shift) |
| **Candidate `RULE-1` engineered** (3 structures) | `B5/S45` | `20.0` | `8.7` | `8.7` | Glider `10.00`-voxel L1 shift (`18.56%` Morton); oscillator `period = 4`; octahedron `period = 1` |
| **Supercritical control** (4 soups) | `B4/S45` | `20.0` | `1,398.8` | `2,249.8` | Runaway explosion across all 4 seeds (`1,166–1,609` live cells at `t = 20`, `8.71%` mean Morton ratio) |

- **Falsifiable phase separation:** Across identical initial soups (`0xB17B0001..0xB17B0004`, `90–120`
  initial cells), the four predeclared metrics strictly separate `B6/S45` (subcritical extinction) <
  `B5/S45` (edge-of-chaos bounded persistence and coherent glider/oscillator transport) < `B4/S45`
  (supercritical chaotic blow-up), with zero `10,000`-cell safety-cap hits.
- **Visual observations (labeled qualitative companions, not metric substitutes):** `801×801` 3D
  snapshots (`t = 0, 4, 12, 20`) exported on merged `origin/main` (`f0c428e`) for Seed `0xB17B0001`
  across all three rules (`emergence_r1_contact_sheet.png`) and the 4-panel metric chart
  (`emergence_r1_metrics_chart.png`) were visually inspected via `read_file`, confirming visual
  extinction (`B6/S45`), bounded surviving structure (`B5/S45`), and frame-filling blow-up (`B4/S45`).

## Non-claims

This bounded assay evaluates only 15 deterministic trajectories over a `20`-tick horizon in `[-4, 3]^3`
for `B5/S45`, `B6/S45`, and `B4/S45`. It does **not** prove universal computation, open-ended
evolution, spontaneous self-replication, intelligence, life, a fractal scaling law, or asymptotic
behavior outside the tested finite rule and corpus.
