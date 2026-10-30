---
name: gitnexus-area-pages
description: "Skill for the Pages area of bitblock. 47 symbols across 1 files."
---

# Pages

47 symbols | 1 files | Cohesion: 77%

## When to Use

- Working with code in `crates/`
- Understanding how onPlayChemApply, playChemBudgetView, playChemEveryTicks work
- Modifying pages-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/wasm/pages/play_client.js` | onPlayChemApply, playChemBudgetView, playChemEveryTicks, playChemFillFromPreset, playChemLoadCustom (+42) |

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `onPlayChemApply` | Function | `crates/wasm/pages/play_client.js` | 445 |
| `playChemBudgetView` | Function | `crates/wasm/pages/play_client.js` | 263 |
| `playChemEveryTicks` | Function | `crates/wasm/pages/play_client.js` | 259 |
| `playChemFillFromPreset` | Function | `crates/wasm/pages/play_client.js` | 292 |
| `playChemLoadCustom` | Function | `crates/wasm/pages/play_client.js` | 311 |
| `playChemRadiusVal` | Function | `crates/wasm/pages/play_client.js` | 261 |
| `playChemReadLanes` | Function | `crates/wasm/pages/play_client.js` | 274 |
| `playChemSeedVal` | Function | `crates/wasm/pages/play_client.js` | 260 |
| `playChemUpdateInfo` | Function | `crates/wasm/pages/play_client.js` | 297 |
| `playChemWriteInputs` | Function | `crates/wasm/pages/play_client.js` | 287 |
| `playNum` | Function | `crates/wasm/pages/play_client.js` | 254 |
| `playResetLive` | Function | `crates/wasm/pages/play_client.js` | 378 |
| `playSelectedWorld` | Function | `crates/wasm/pages/play_client.js` | 374 |
| `playSphereCells` | Function | `crates/wasm/pages/play_client.js` | 267 |
| `playFitCanvas` | Function | `crates/wasm/pages/play_client.js` | 83 |
| `playLoop` | Function | `crates/wasm/pages/play_client.js` | 210 |
| `playPaint` | Function | `crates/wasm/pages/play_client.js` | 106 |
| `playRefreshChem` | Function | `crates/wasm/pages/play_client.js` | 324 |
| `playStepAndPaint` | Function | `crates/wasm/pages/play_client.js` | 177 |
| `playSyncCanvasSize` | Function | `crates/wasm/pages/play_client.js` | 93 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `OnPlayChemRebuild → PlayNum` | cross_community | 5 |
| `OnPlayKeyDown → PlaySelectedLayout` | cross_community | 5 |
| `PlayLoop → PlayFitCanvas` | intra_community | 5 |
| `OnPlayKeyDown → PlayNum` | cross_community | 5 |
| `OnPlayChemDynamicsMenu → PlayChemSelVal` | intra_community | 4 |
| `OnPlayChemRebuild → PlayChemBudgetView` | cross_community | 4 |
| `OnPlayChemRebuild → PlayChemReadLanes` | cross_community | 4 |
| `OnPlayKeyDown → EnsurePlayLoop` | cross_community | 4 |
| `OnPlayKeyDown → PlayChemCutMode` | cross_community | 4 |
| `OnPlayKeyDown → PlaySelectedChem` | cross_community | 4 |

## How to Explore

1. `context({name: "onPlayChemApply"})` — see callers and callees
2. `query({search_query: "pages"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
