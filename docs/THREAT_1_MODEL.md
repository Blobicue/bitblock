# THREAT-1: quantitative adversary model and 11-class claim map v1

This document resolves `PS-067:THREAT-1@base=BOUNDED`, closing all 11 `OPEN (UNQUANTIFIED)`
parameter slots from [`THREAT_D0_ENVELOPE.md`](THREAT_D0_ENVELOPE.md) and unlocking
`PS-074:BOUNDS-1@base=READY`.

## Provenance

- **Merged `origin/main` subject (PR #32):** commit `8bb352dee5cc2c2db7e34634ea2e883cf7e806df`
  (tree `c76bc964ebba793b461d6f6510ec7b00f0769389`, parent `4a88e75b11a60a5140a97572b5da1de00faab963`,
  CI runs `45033334910` & `45033330670`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/threat_1.rs`](../crates/research/src/threat_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`).
- **Independent Python reference suite:** [`tools/threat_1_and_vm_d1_vectors.py`](../tools/threat_1_and_vm_d1_vectors.py)
  (`115/115` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave4_threat_1_and_vm_d1.rs`](../tests/wave4_threat_1_and_vm_d1.rs)
  and `python3 tools/bbmutate.py threat_1` (`14/14` canonical big-endian sites KILLED, 100%).

## Verified golden tags & conservative consumer-node BFT envelope

- **`ThreatModelV1` (`bitblock/threat-1/adversary-model/v1`):**
  `#bb.policy.sha256.qnfdid2kwcnzhyxinyitgiu62cimnl5a37ugwjye7wolmu7qurmq`
- **`ThreatClaimMapV1` (`bitblock/threat-1/property-claim-map/v1`):**
  `#bb.receipt.sha256.kf6z6jjjbalzphsawlx7b3q7qpy4geyxxpr2vhlpmnnr5k374apq`

| Class | Attacker power | Quantified v1 threshold | Loss condition | Detection / recovery / mitigation | Explicit v1 exclusions |
| ---: | --- | --- | --- | --- | --- |
| 1 | Static & adaptive validator corruption | `< 1/3` stake (`3333` bps), `>= 1` epoch corruption delay | Conflicting finalized roots or liveness stall | `> 2/3` strict Ed25519 quorum, equivocation slashing, deterministic safe-halt | None |
| 2 | Network scheduling, partition & eclipse | `2,000 ms` target block, `10,000 ms` GST, `>= 8` honest peers | Eclipsed verifier follows stale/invalid fork | Multi-peer header/proof check (`SYNC-1`), `CHAIN-LAUNCH-1` anchor | Async partition liveness |
| 3 | Key compromise (actor/validator/release/gov) | `>= 1` epoch delay, `21`-epoch exit timelock, `m-of-n` thresholds | Unauthorized edit, double-sign, forged release | Role-separated keys, `SIGNER-OPS-1`, safe-halt + new genesis on live-authority break | Host custody failure |
| 4 | Cryptographic primitive / hash break | `128`-bit classical Ed25519 + SHA-256 suite | Preimage/collision or signature forgery | Versioned suite IDs + preventive `UPGRADE-1`; terminal safe-halt on `ProfileIdHashV1` break | Unproven PQ hardness |
| 5 | Parser, codec & proof-verification DoS | `<= 2 MiB` block payload (`2,097,152` B), `<= 12 KiB` proof | OOM, CPU stall, or parser divergence | Zero-allocation streaming preflight (`BOUNDS-1`), dual-language `CODEC-1` | None |
| 6 | State-growth & history spam | `<= 4,096` state writes/block, bounded rolling retention | Disk exhaustion or unrecoverable restart | `LOCAL-BOUNDS-1` + `BOUNDS-1` + refundable per-block deposit (`ASSET-D1`/`RESOURCE-1`) | None |
| 7 | Censorship & ordering manipulation | `CONSENSUS-1`: `<= 3 * active_validators` blocks under GST | Indefinite exclusion of valid transactions | Weighted proposer rotation in `CONSENSUS-1`; receipt accountability | **`SEQUENCER-1` carries zero censorship-resistance guarantee** |
| 8 | Data availability & archival withholding | `28`-epoch rolling retention (`>= 21`-epoch unbonding) | Accepted root with unavailable body | Data-before-vote rule, embedded launch/successor bundles, weak-subjectivity sync | Unpinned deep archive nodes |
| 9 | Supply-chain & build substitution | `3-of-5` independent deterministic release builders | Backdoored binary diverges from reviewed source | Pinned `rust-toolchain.toml`, `=` Cargo pins, `Cargo.lock` SBOM (`RELEASE-R1`/`RELEASE-1`) | Compromised `>= 3` builders |
| 10 | Bootstrap, genesis & upgrade spoofing | `>= 8` honest peers + out-of-band launch trust anchor | Fresh node syncs attacker chain or downgrade | Non-self-referential `GENESIS-D1 -> PROTOCOL-ID-1`, self-contained closure bundles | Compromised out-of-band anchor |
| 11 | Host, local storage, display & bridges | `21`-epoch unbonding reference; `external_bridges_admitted = false` | Silent rollback, display spoofing, bridge drain | Exclusive lock + `sync_all` + `RequireAncestor` (`STORAGE-1`), strict non-claims | **Sudden power hardware, physical display truth, external bridges/oracles excluded in v1** |

## Non-claims

Quantifying attacker powers and loss conditions does not prove implementation correctness, verify
worst-case resource caps (`BOUNDS-1`), or license Layer 1 production security.
