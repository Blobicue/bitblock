# RULE-1: synchronous 3D Moore-neighborhood Life (`B5/S45`) implementation receipt v1

This implementation receipt resolves `PS-042:RULE-1@base=IMPLEMENTED` (alongside
`PS-043:RULE-1@rule-choice=BOUNDED`) and unlocks `PS-049:EMERGENCE-R1@base=READY`.

## Provenance

- **Merged `origin/main` subject (PR #28):** commit `9200f7177111451f686ab9464dcdd63c9b3ba1d6`
  (tree `0362131a7257b1b03b4fe6228f8f21573a328b25`, parent `56c80bc70f523a5b6d83971ae8fe93c407530330`,
  CI runs `45027814143` & `45027819964`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/rule_1.rs`](../crates/research/src/rule_1.rs)
  (`wasm32`-clean, strictly opt-in behind `--features protocol-research`; default live editing,
  `world_tag`, assets, and consensus remain untouched).
- **Independent Python reference stepper:** [`tools/rule_1_vectors.py`](../tools/rule_1_vectors.py)
  (stdlib-only, zero Rust imports; `106/106` protocol schemas independently vectored).
- **CI integration & mutation evidence:** [`tests/rule_1_b5s45.rs`](../tests/rule_1_b5s45.rs) and
  `python3 tools/bbmutate.py rule_1` (`14/14` canonical big-endian sites KILLED, 100%).
- **Visual benchmark & PNG snapshot exporter:** [`examples/rule_1_visual_probe.rs`](../examples/rule_1_visual_probe.rs)
  (`cargo run --release --quiet --features protocol-research --example rule_1_visual_probe -- --json --dump-snapshots <dir>`).

## Verified golden tags & invariants

| Check | Pinned tag / metric | Verification |
| --- | --- | --- |
| `Rule1DescriptorV1::bays_4555_default()` | `#bb.policy.sha256.jckp2uhbjj2fa4evhth5sqyzde45yy7bnpl4htsbuezxw55itdqq` | Exact Rust + Python parity |
| Bays 10-cell 3D glider at `(20,20,20)` tick 1 | `#bb.receipt.sha256.iixge4c2kypetpd2futpdd3cdiyr4e5pm2kpuf3zcx5oknbii57a` | Exact Rust + Python + `History` parity |
| Bays 10-cell 3D glider tick 4 (`shift = (0, +1, +1)`) | `#bb.receipt.sha256.onankhpta54nmiufuyfkxpgqohp3nxjghr3dvvhlewqpyc64jfyq` | Exact Rust + Python + `History` parity |
| 6-cell 3D octahedron still life at `(0,0,6)` tick 1 (`quiescent = true`) | `#bb.receipt.sha256.pp2w2naumy5a7v3dwwr72lmjd2qtkklbrwhbamb52xbabsjr74pa` | Exact Rust + Python parity |
| 48 octahedral cube symmetries (`octahedral_transform`) | All `0..48` rotations/reflections of Bays' 10-cell glider | Exact 4-tick transformed shift |
| Period-4 3D oscillator | `10 -> 8 -> 10 -> 8 -> 10` exact `world_tag` return at tick 4 | CI verified + visually inspected |
| Bedrock `(0,0,0)` survival & `i64::MIN`/`MAX` safety | 7 mutable cells die in 2×2×2 cluster; corner dies cleanly with zero overflow | CI verified |
| Fail-closed resource caps (`max_candidate_cells`, `max_mutations_per_tick`) | Refuses before mutation, leaving `world_tag` byte-identical | CI verified |
| Powered rule-mutant control (`B6/S45` vs `B5/S45`) | Diverges on tick 1 | Verified in Python vector suite |

## Merged-head release visual showcase measurements (`9200f71`, ARM64 Linux)

- **Scene (27 live voxels across 20 ticks):** permanent orange bedrock `(0,0,0)` + 6-cell 3D
  octahedron still life at `(0,0,6)` + 10-cell period-4 oscillator at `(-8,0,0)` + Carter Bays'
  10-cell glider launching at `(5,-2,-2)` and displacing `(0, +5, +5)` to `(5, 3, 3)` by tick 20.
- **Step latency:** `p50 = 57.82 µs`, `max = 79.02 µs` (`296–306` Moore candidates/tick).
- **801×801 production `Renderer` latency:** `p50 = 0.467 ms`, `max = 0.645 ms`.
- **Tick 0 -> Tick 20 `world_tag`:** `#bb.world.sha256.qddd5tjp3aazxbkiarnij2eqm7qdqgvkkcowt2gn2cdrdl2viyya`
  -> `#bb.world.sha256.5wjdff2xpdg2ip5pkxakdeabyyxrbr767uvmvyrmieds64fudnyq` (`history_parity = true`).
- **Visual verification:** all 8 rendered `801×801` PNG frames (`t = 0, 1, 2, 4, 8, 12, 16, 20`)
  were directly inspected confirming stationary bedrock/octahedron, pulsing oscillator, and
  clean diagonal 3D glider flight.

## Non-claims

One finite opt-in cellular rule does not prove universal computation, useful emergence
(`EMERGENCE-R1` is now `READY` for formal null-model assays), a visual programming language
(`LANGUAGE-R1` / `VM-1`), or Layer 1 consensus authority.
