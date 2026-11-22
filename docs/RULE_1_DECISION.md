# RULE-1: autonomous transformation rule choice v1

This decision record resolves `PS-043:RULE-1@rule-choice=BOUNDED` and unlocks
`PS-042:RULE-1@base=READY` for implementation and visual verification in Wave 2.

## Selected opt-in rule: Synchronous 3D Moore-Neighborhood Life (`B5/S45`)

1. **Substrate purity:** operates strictly over Bitblock's 1-bit `{0, 1}` sparse `World` with zero
   hidden cell state, kept **opt-in only** and strictly separate from default live editing,
   `ASSET-D1` balances, and Layer 1 consensus authority.
2. **Neighborhood & transition law (Carter Bays' Life 4555 — `B5/S45`):**
   - For each coordinate `c = (x, y, z) ∈ i64³`, count live neighbors `n ∈ 0..=26` across the 26
     axis/edge/corner Moore neighbors `(x + dx, y + dy, z + dz)` where `dx, dy, dz ∈ {-1, 0, 1}`
     not all zero (checked arithmetic rejecting coordinates adjacent to `i64::MIN`/`i64::MAX`
     overflow cleanly via saturating/skip boundary rules).
   - **Birth:** an off coordinate becomes `1` iff `n == 5`.
   - **Survival:** a live mutable coordinate stays `1` iff `n == 4 || n == 5`; otherwise turns `0`.
   - **Bedrock invariant:** `(0, 0, 0)` is permanently clamped to `1` — it contributes to its 26
     neighbors' live counts but never dies or emits a burn.
3. **Zero order conflict & quiescence:** updates are **synchronous** (all candidate cells in the
   1-neighborhood of occupied bits are evaluated against pre-tick `World_t`, then canonical sorted
   diff edits commit atomically to produce `World_{t+1}`), making the rule invariant under all 48
   octahedral symmetries and giving O(1) quiescence detection when the diff set is empty.
4. **Finite resource bound:** enforces explicit `max_candidate_cells` and `max_mutations_per_tick`
   preflight caps, rejecting over-budget steps fail-closed before mutating `World`.

## Rejected alternatives

- **6-neighbor von Neumann threshold rule:** smaller frontier, but less established 3D glider and
  oscillator emergence than Bays' `B5/S45`.
- **2×2×2 Margolus block-cellular rule:** strictly bit-conserving, but tick-parity grid phasing
  breaks single-voxel translation invariance.
- **3D local falling-bit gravity rule:** anisotropic (`-y` axis) and less expressive for emergence.

## Acceptance gate for `PS-042:RULE-1@base` (Wave 2)

- Deterministic Rust implementation + independent Python reference stepper agreeing byte-for-byte.
- Base/step vectors, 48-way octahedral symmetry checks, Bays' 10-cell 3D glider translation check,
  bedrock immutability, over-cap fail-closed rejection, headless/live parity, and visually inspected
  multi-tick PNG snapshots + live Chrome 155 verification.
