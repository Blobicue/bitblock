# LadderLang transition v1 capsule

This is a frozen two-source executable target for the one-bit transition
semantics used by Bitblock's `on`, `off`, and `toggle` operations. It carries
the exact source identities in `source-manifest.tsv` and uses the local Lean
4.33.0 toolchain.

Run from this directory:

```powershell
.\verify.ps1
```

The verifier first recomputes the two source SHA-256 values in
`source-manifest.tsv`, then builds the copied `Ladderlang.Core` and
`Ladderlang.Entries` libraries and runs the direct truth-table theorem file
with `--trust=0`. On the currently pinned Lean/LadderLang sources, each listed
theorem reports dependency on `propext`; this is not an axiom-free result.

What it covers: the six mutable operation/prestate rows, three valid immutable
origin rows, a wrong-operation control, and bounded evaluator controls for
fuel (`RAN`) and size (`GREW`).

What it does not cover: Rust implementation refinement, sparse-coordinate
world semantics, hashes, history/mint/burn provenance, rendering, iota leaf
cost of execution, proof-of-history, proof-of-stake, consensus, currency, or
Layer 1 properties. The source files are locally hash-pinned; no independent
remote source or custody assertion is made by this capsule.
