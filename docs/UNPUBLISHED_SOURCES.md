# Unpublished sources and local paths

Some records in this repository cite material that is not published with it. They are kept as
written, because several are frozen audit records whose text is pinned by Git blob or commit,
and editing them would falsify what they record. This page says what those references are.

## `design.txt`

`design.txt` is the raw research conversation that
[`DESIGN_RESEARCH_DISTILLATION.md`](DESIGN_RESEARCH_DISTILLATION.md) compresses. It was never
committed and is not published. What the repository does publish is its identity:
[`design-source-receipt-v1.json`](design-source-receipt-v1.json) pins its SHA-256 and the
counting method used on it.

So an outside reader can check that the distillation's own claims are internally consistent
and that later audits cite the same source bytes. They cannot check that the distillation is a
faithful reading of `design.txt`. That claim rests on the author's local copy.

## Local paths

These files contain absolute paths from the author's Windows machine:

| File | Path | What it was |
|---|---|---|
| `docs/DESIGN_RESEARCH_DISTILLATION.md`, `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT.md`, `docs/TRANSCRIPT_AUDIT.md`, `docs/design-source-receipt-v1.json` | `C:\Users\Owen\Downloads\design.txt` | where `design.txt` lived when it was receipted |
| `docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP11.md` | `C:/Users/Owen/bitblock-t2` | the clean checkout that audit ran in |
| `docs/TIMELINES.md`, `tools/mirror_timeline1.py` | `C:/Users/Owen/bitblock`, `C:/Users/Owen/bitblock-t2` | the retired timeline-1 directory and the checkout it was mirrored into |

They describe where work happened. Nothing in the build, the tests or CI reads them.
`tools/mirror_timeline1.py` is the one tool that uses them, and CI skips it because timeline 1
is retired.

The generated `index/nodes.jsonl` repeats some of these strings because it indexes the files
above. Regenerating it will keep them for as long as the source files contain them.
