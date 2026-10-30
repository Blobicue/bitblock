<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **bitblock** (9876 symbols, 35289 relationships, 882 execution flows).

> Index stale? Run `node .gitnexus/run.cjs analyze --index-only` from the project root — it auto-selects an available runner. No `.gitnexus/run.cjs` yet? Bootstrap with `npx`, `bunx`, or `pnpm dlx` — e.g. `bunx gitnexus@latest analyze` (npm 11 npx crash; #1939).

## Always Do

- **MUST run impact before editing.** Use `impact({target: "symbolName", direction: "upstream"})` or `node .gitnexus/run.cjs impact "symbolName" --direction upstream --repo .`; report callers, processes, and risk. Never substitute grep for graph analysis.
- **MUST analyze graph changes before committing.** Use `detect_changes({scope: "all"})` (MCP) or `node .gitnexus/run.cjs detect-changes --scope all --repo .` (CLI fallback). `partial: true` or `truncated: true` is not a clean check — a zero means unseen, not unaffected; re-run it. For regression review: `detect_changes({scope: "compare", base_ref: "main"})` or `node .gitnexus/run.cjs detect-changes --scope compare --base-ref "main" --repo .`.
- MUST warn on HIGH/CRITICAL `risk` pre-edit; never use `riskSharedAxes` to waive a HIGH/CRITICAL `risk` warning. Compare File/symbol: MCP File omits axes; Graph-RAG expands File.
- **MUST treat `risk: UNKNOWN` as unresolved, not as low.** An empty caller set is not evidence the symbol is unused — it can also mean the callers are not resolvable by the index (plain-object property access, dynamic dispatch, cross-language calls). `impact` pairs `UNKNOWN` with a `riskNote` saying so. Confirm with a text search before treating the symbol as safe to change or delete; do not proceed on the strength of a zero.
- **MUST use `query({search_query: "concept"})` for concepts/flows, `context({name: "symbolName"})` for a named symbol, or `impact` for blast radius, on read-only callers, dependencies, imports, or execution flow.** Graph first; text search only for empty/`UNKNOWN`/literals.
- For security review, `explain({target: "fileOrSymbol"})` lists taint findings (source→sink flows; needs `analyze --pdg`).

## Never Do

- NEVER edit a function, class, or method before MCP/CLI impact analysis.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis, and never read `UNKNOWN` as an all-clear — it means the walk could not answer, which is the one verdict that requires confirming by other means.
- NEVER rename symbols with find-and-replace — use `rename` which understands the call graph.
- NEVER commit before MCP/CLI graph change analysis.

## Resources

| Resource | Use for |
| --- | --- |
| `gitnexus://repo/bitblock/context` | Codebase overview, check index freshness |
| `gitnexus://repo/bitblock/clusters` | All functional areas |
| `gitnexus://repo/bitblock/processes` | All execution flows |
| `gitnexus://repo/bitblock/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
| --- | --- |
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus-cli/SKILL.md` |
| Work in the Tools area (697 symbols) | `.claude/skills/gitnexus-area-tools/SKILL.md` |
| Work in the Chem area (335 symbols) | `.claude/skills/gitnexus-area-chem/SKILL.md` |
| Work in the Examples area (246 symbols) | `.claude/skills/gitnexus-area-examples/SKILL.md` |
| Work in the Cluster_71 area (69 symbols) | `.claude/skills/gitnexus-area-cluster-71/SKILL.md` |
| Work in the Cluster_0 area (63 symbols) | `.claude/skills/gitnexus-area-cluster-0/SKILL.md` |
| Work in the Cluster_59 area (63 symbols) | `.claude/skills/gitnexus-area-cluster-59/SKILL.md` |
| Work in the Pages area (47 symbols) | `.claude/skills/gitnexus-area-pages/SKILL.md` |
| Work in the Cluster_55 area (45 symbols) | `.claude/skills/gitnexus-area-cluster-55/SKILL.md` |
| Work in the Cluster_131 area (41 symbols) | `.claude/skills/gitnexus-area-cluster-131/SKILL.md` |
| Work in the Cluster_56 area (37 symbols) | `.claude/skills/gitnexus-area-cluster-56/SKILL.md` |
| Work in the Cluster_163 area (36 symbols) | `.claude/skills/gitnexus-area-cluster-163/SKILL.md` |
| Work in the Cluster_107 area (35 symbols) | `.claude/skills/gitnexus-area-cluster-107/SKILL.md` |
| Work in the Cluster_74 area (25 symbols) | `.claude/skills/gitnexus-area-cluster-74/SKILL.md` |
| Work in the Cluster_209 area (25 symbols) | `.claude/skills/gitnexus-area-cluster-209/SKILL.md` |
| Work in the Cluster_64 area (24 symbols) | `.claude/skills/gitnexus-area-cluster-64/SKILL.md` |
| Work in the Cluster_244 area (23 symbols) | `.claude/skills/gitnexus-area-cluster-244/SKILL.md` |
| Work in the Cluster_73 area (22 symbols) | `.claude/skills/gitnexus-area-cluster-73/SKILL.md` |
| Work in the Cluster_237 area (22 symbols) | `.claude/skills/gitnexus-area-cluster-237/SKILL.md` |
| Work in the Cluster_69 area (21 symbols) | `.claude/skills/gitnexus-area-cluster-69/SKILL.md` |
| Work in the Cluster_146 area (21 symbols) | `.claude/skills/gitnexus-area-cluster-146/SKILL.md` |

<!-- gitnexus:end -->
