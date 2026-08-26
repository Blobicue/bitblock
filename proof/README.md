# Bitblock proof capsule

This directory is a standalone Lean 4 model of Bitblock's smallest world core.
It does not import the Rust implementation and makes no claim about rendering,
camera arithmetic, hashing, or wall-clock scheduling.

`BitblockProof.World` gives every coordinate an unbounded integer address,
starts with only immutable origin bedrock on, stores only the finite
coordinates whose bits are on, and treats an ordered list of `on`, `off`, and
`toggle` edits as a program. Lean checks the initial world, bedrock preservation
for every edit and replay, membership laws, semantic idempotence, toggle
involution, finite support, and replay composition.

`BitblockProof.MintBurn` adds a separate duplicate-free `List` model for
literal supply accounting. Mutable supply excludes origin; mint increments it
exactly for an absent non-origin coordinate; burn decrements it exactly for a
present non-origin coordinate; protected and no-op edits preserve it; and
replay preserves origin. The separate representation is deliberate: raw list
length in `World` is not supply because that model permits duplicate storage
while treating duplicates as observationally irrelevant.

`BitblockProof.QuotedByteCost` is a separate static representation-cost proof
for the content-neutral quoted-byte codec. It proves equal cost for the two
distinct bit terms, the exact S/K/I census and atom/iota cost for every
fixed-width byte, and closed forms for every finite byte-list right spine.
Powered theorems separate left-associated PAIR, reversed-bit, and left-list
mutants even though their costs can coincide. Its SK syntax has no evaluator:
the results do not establish executable edit semantics.

`LadderLangQuotedByteParity.lean` is deliberately outside the standalone
Bitblock target. Run it from the separately pinned LadderLang checkout at commit
`59042265167b8f40700a891d890fde8015bcade9`; it asks LadderLang's own
`Lam.costChecked` to reproduce the bit, `PAIR`, byte, and two-byte-spine costs,
and verifies that an open term is refused. This is a dependency/parity assay,
not a claim that the two repositories share a verified compiler.

These are abstract mathematical models. They do not prove that the Rust world,
renderer, hash/receipt encoding, network, or consensus implementation refines
either model.

Build from this directory with:

```powershell
lake build
```
