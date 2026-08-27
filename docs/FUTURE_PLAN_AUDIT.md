# Audit of the Bitblock future plan

**One read-only sweep followed by one coordinated fix sweep — 2026-08-27**

This audit freezes exact input
`162406819133c1fd57d0fd26cbe5fa0b495766b8`. The worktree was clean at that commit. Three
independent read-only lenses examined authenticated-state security, capability/dependency logic,
and motive/provenance consistency; a fourth local pass reconciled their findings across the same
files. No file was edited until all findings below were frozen.

The result is identified as the first descendant commit that adds this file. A Git commit cannot
contain its own final identifier, so the input is embedded while the result is recovered from the
file's first-addition history. This audit becomes stale when a named plan/source contract changes;
it is not a proof that the inventory is complete.

## Evidence boundary

The sweep inspected `README.md`, `docs/PROJECT_INTENT.md`, `docs/CAPABILITY_TREE.md`,
`docs/AUTHENTICATED_STATE_PLAN.md`, `docs/TRANSCRIPT_AUDIT.md`, `docs/HANDOFF.md`,
`docs/LAYER1_DESIGN_PAPER.md`, `docs/LAYER1_DESIGN_AUDIT.md`, and the source anchors named by those
documents. Source/proof files were unchanged from
`6545d8e7d60e75607445fda564bbf11610a62658` through the frozen input. No Rust, Lean, network,
storage-fault, cryptographic, or physical-display suite was run.

“Patched” below means the plan now names a deterministic contract, dependency, gate, or explicit
open boundary. It never means the runtime implements it.

## Frozen finding set and one-sweep patch map

| ID | Priority | Gap at `1624068` | One-sweep correction |
| --- | --- | --- | --- |
| FP-01 | P0 | Coordinate bytes existed, but trie traversal, child direction, bitmap numbering, and sibling order did not. | Fixed one MSB-first depth formula and reversal-sensitive vectors in the authenticated-state plan. |
| FP-02 | P0 | Sparse-tree present/default/branch hashing had no normative recurrence; Patricia-only hash prose could be misapplied. | Added a common domain/profile/depth-bound node-hash contract and exact symbolic SMT recurrence. |
| FP-03 | P0 | “Publish all nodes and roots atomically” named no feasible commit unit and allowed mixed recovered heads. | Added `CommitmentSetV1`, stage/verify/manifest/head ordering, and previous-or-next complete-tuple crash invariant. |
| FP-04 | P0 | Per-map root parity did not prove that committed active-token/episode projections matched the same ledger head. | Added independent cross-map rebuild/refinement and same-manifest gates to the plan, `COMMIT-1`, and `EXECUTION-ROOT-1`. |
| FP-05 | P1 | GC protected pinned roots but could race a writer's unpinned staged nodes; logical episode pruning was conflated with physical GC. | Added writer/GC generation protection, the exact adversarial schedule, and a logical-pruning-versus-GC boundary. |
| FP-06 | P1 | Proof input allowed both “query key” and “canonical key”; commitment codecs and numeric limits were implicit. | Defined one typed/byte key input, caller-selected claims, per-domain `CODEC-1`, and frozen numeric commitment limits. |
| FP-07 | P0 | Pre-genesis profile, limits, suites, governance, validator authority, and derived identifiers formed a semantic bootstrap circle. | Added `PROFILE-SCOPE-D0` and `GENESIS-D1`; moved pre-ID consensus/governance schemas ahead of `PROTOCOL-ID-1`; derived IDs are excluded from their own preimages. |
| FP-08 | P0 | No node owned the full application/execution root, including parameter, governance, validator-schedule, authorization, and optional asset domains. | Added `EXECUTION-ROOT-1` with per-domain evidence, typed composition, manifest publication, and header/root gates. |
| FP-09 | P1 | Local storage required unnamed bounds while the global bounds node depended on a future authority profile. | Added `LOCAL-BOUNDS-1` and made it an explicit prerequisite of `STORAGE-1`. |
| FP-10 | P1 | VM implementation supplied the operations that `BOUNDS-1` needed before allowing that implementation, and refinement pointed in the wrong direction. | Added `VM-D1`; bounds consume its frozen semantics, `VM-1` implements them, and `REFINE-1@program` follows implementation. |
| FP-11 | P1 | Independent executors had no named authoritative transition specification and could pass by agreeing with each other. | Added `TRANSITION-SPEC-D1`; each executor must pass its vectors/mutants before differential comparison. |
| FP-12 | P1 | Upgrade dependencies used two candidates rather than admitted-old/candidate-new polarity and required a fabricated migration at genesis. | Added `UPGRADE-FRAMEWORK-1`; concrete `UPGRADE-1@old->new` consumes an admitted old profile, while genesis uses the lifecycle framework. |
| FP-13 | P1 | `TESTNET-1` said every “relevant” G0–G19 gate must pass, allowing circular G18 or silent optional-gate exclusion. | Added a machine-readable stage-gate matrix with required/conditional/excluded-with-reason states and explicit stage ownership. |
| FP-14 | P1 | Program resource charging and validator incentives omitted named resource/asset/root prerequisites. | Made `RESOURCE-1` support non-economic quotas, required it for authoritative programs, and bound incentive economics to selected/authenticated asset state. |
| FP-15 | P2 | READY/OPEN labels obscured missing prerequisites, profile-instance status, and release/threat framework phases. | Split `THREAT-D0` from locked `THREAT-1@profile`, qualified decision nodes, and separated `RELEASE-1@framework` from profile instances. |
| FP-16 | P0 | Bedrock was economically open in the motive/tree/commitment plan but silently “not an initial coin” in the Layer 1 paper. | Made the paper's no-occupancy-token treatment a conditional first-profile sketch and retained the separate locked-genesis-allocation choice for `ASSET-D1`. |
| FP-17 | P2 | Audit finding F-12 pointed release integrity at G14, which is the client-status gate; the correct gate is G16. | Corrected the table and appended a transparent post-checkpoint erratum. |
| FP-18 | P2 | Dependency prose, compact diagrams, source claims, schedule summaries, and historical validation reports lacked one reproducible authority/receipt boundary. | Made detailed dependency fields authoritative, added a retained checker with negative controls, pinned source/input scope, subordinated conceptual schedules/summaries, and replaced inherited validation prose with exact commands. |

## Areas checked with no material gap in this sweep

- Every known user motive was present: binary coordinates, observer-first minimalism, deterministic
  ticks, cheap Rust, scale perception, origin/collision/crosshair/metrics, human/agent replay,
  emergence and visual language, ternary-hash/LadderLang research, occupancy episodes, multiplayer
  identity, authenticated maps, checkpoint workflow, and an eventual scoped Layer 1.
- The sign-biased `CoordinateKeyV1` formula is bijective and remains distinct from legacy
  `world_tag`; the missing contract began after byte encoding and is patched by FP-01.
- Patricia deletion collapse, extension merging, tombstone rejection, and state-restoration intent
  were coherent; the patch adds the missing insertion/leaf-length constraints without selecting it.
- World-state repetition versus episode-history distinction, caller-selected root context, typed
  domains, finality/availability non-claims, and the runtime chunked-world hot path remain sound.
- Static spatial grammar research and autonomous-rule research remain independent early branches.
- No current trie, transferable asset, network identity, consensus, cryptocurrency, or Layer 1
  authority is claimed. The intended ternary hash remains unidentified.

## Validation receipt

The coordinated patch was admitted only after these commands exited zero:

```powershell
python -B tools/check_planning_docs.py --self-test
git diff --check
```

The planning checker reads the actual capability document, including foundation-table edges; checks
all detailed fields, undefined references, and the conservative dependency DAG; checks links,
fences, duplicate headings, and table shape in the planning set; and proves its own main failure
paths with in-memory subject mutations. A passing documentation checker says nothing about the Rust,
Lean, storage, network, cryptographic, or physical-display implementations.

## Residual boundary

This sweep closes the listed specification contradictions. Every non-document capability remains at
its stated READY/LOCKED/OPEN/BOUNDED status. The preferred semantic implementation frontier remains
`ACCOUNT-1`; `COMMIT-R1`, `COST-1`, `LANGUAGE-R1`, `SCALE-1`, `REFINE-1`, `THREAT-D0`, and source
discovery can advance only as separately scoped research. A later audit must pin a new input and may
find new gaps; it must not silently append them to this frozen set.
