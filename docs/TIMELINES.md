# Timelines and enforcement

This repository is **timeline 2**: a fork of the bitblock working tree as it
stood on 2026-08-26, before the project had any version control. The fork
point is the `inherited-state` tag — genesis commit `992996d`, the tree copied
verbatim from `C:/Users/Owen/bitblock` (build artifacts aside). The original
directory continues as timeline 1, unversioned, and is not touched by this
repository's work.

## What enforcement exists

Three CI jobs gate every push and pull request:

- **rust (windows-latest)** — `cargo fmt --check`, `cargo clippy --all-targets
  -D warnings`, `cargo test`. The pinned `1.96.0` toolchain comes from
  `rust-toolchain.toml`.
- **python vector tools (ubuntu-latest)** — every standalone `tools/*.py`
  vector tool must exit zero; `tools/check_planning_docs.py --self-test` runs
  the powered planning/audit/receipt/link controls, exact planning-authority inventory, and frozen
  text EOL-policy checks from a full-history checkout;
  `tools/simulate_sequence.py examples/observer_walk_sequence.json --expect`
  checks one explicit scenario; and `tools/metric_registry.py audit` must report
  an exact manifest with zero unresolved references. Argument-taking tools are
  excluded from the bare-tool loop and invoked explicitly. The registry audit
  has been watched failing on a deliberately corrupted registry before being
  enforced.
- **lean proof capsule (ubuntu-latest)** — `lake build` in `proof/` must
  succeed. `LadderLangQuotedByteParity.lean` is deliberately outside this
  target (it requires the separately pinned LadderLang checkout) and is not
  CI'd.

`main` was intended to additionally carry branch protection (required
checks, no force pushes, linear history). GitHub declined: **branch
protection is a paid feature for private repositories** on the free tier
(HTTP 403, "Upgrade to GitHub Pro or make this repository public"). The
repository stays private by the same deliberate policy as its sibling
project, so hard protection is **not currently active** — what enforces
today is that CI runs on every branch push and pull request and a red run is
loud and visible. Enabling hard protection is a one-command change
if the repository is later made public or the plan upgraded; that choice
belongs to the owner, not the timeline.

## The mirror — retired 2026-08-27, the timelines have converged

The lines rejoined before either reached the question below. Timeline 1's
content — including the generated `index/` and its query surface — is merged
into `main`, `timeline-2` is deleted with its history preserved in `main`, and
`main` is the repository's default branch and only line of development.

`codex/bitblock-progress-2026-08-26` was deleted at that first convergence and
then recreated by continuing work. It carried 23 further commits — planning
documents and `tools/check_planning_docs.py`, no Rust — and those are merged
into `main` on 2026-08-27. This time the branch is left in place rather than
deleted: an agent is still writing to that checkout, and removing the branch
would strand its push target. Convergence is the merge, not the deletion.

`mirror/timeline-1` is kept as the record of the divergence, its final
observation being `1ddbfa2` (2026-08-27 06:08 UTC). `tools/mirror_timeline1.py`
now refuses to run: observing that folder again would commit noise about a
directory nobody develops in. The original folder is left untouched as a
historical artifact, exactly as this document always said it would be.

The timeline-1 checkout carries four later commits of its own -- the "outside
witness" series, which gave seven core schemas independent Python vectors. They
are not merged, and not because they were rejected: that checkout shares no
commit with `main`, so there is nothing to merge along. Grafting an unrooted
history to import work `main` already has would buy nothing.

`main` covers all seven, and 51 more: 58 of 58 protocol objects have
independent vectors. Every witness suite has a same-named counterpart here
except `composite_input_vectors.py`, which is this tree's
`composite_frame_input_vectors.py` under its earlier name. The two use
different fixtures and so derive different tags, and each asserts its own tag
in its own Rust -- they do not disagree, they describe different inputs.

The line is superseded, and superseded is recorded, not deleted. The folder
stays exactly where it is.

The key-profile line shared most of its history with the planning line and is
in `main` by that route, plus one cherry-picked commit, `d721e70`, adding
`tools/key_profile_vectors.py`. Nothing of it is outstanding.

A note for whoever adds the next frozen audit. `check_planning_docs.py` hashes
those documents as they sit in the working tree, not as they sit in git, so a
frozen document's `text eol=lf` line in `.gitattributes` must land in the same
commit as the document. Arriving one commit later, a Windows checkout writes
CRLF, the freeze records those bytes, and the gate then fails against its own
correct blob. That happened here to the loop-8 audit and is the same
line-ending class as the LadderLang `verify.ps1` report.

What the mirror was, in the past tense.

`mirror/timeline-1` recorded the observed states of the original
directory, which had no version control of its own and was being actively
worked by another agent toward a Layer-1 cryptocurrency design (blocks as
mint, deletion as sink). `tools/mirror_timeline1.py` copies that tree
read-only — same exclusions as the fork: `target/`, `__pycache__/`,
`.lake/` — into a worktree and commits each observed state with a
timestamp. The branch is rooted at `inherited-state`, so each mirror
commit's diff is the other line's progress since the fork, and
`git diff mirror/timeline-1 main` is the divergence between the
lines as it stood at convergence. An automation re-mirrored every 30 minutes
and pushed; a state identical to the last observation committed nothing.

One observation recorded at the mirror's creation, for whoever reads the
divergence later: the other line's stated destination — creation as mint,
deletion as sink — is the same economic primitive the sibling project's
metabolic fold tested under the name of its death rule, where a measured
discriminator showed the sink question is load-bearing (whether a death
burns its wallet or drops it as a pickup changed the ecology's fate
outright). The two lines will meet at that question from opposite ends:
this one designing the sink, that one having measured one.

They merged before meeting it. The question is still open, and it is now one
line's to answer.

## Commit honesty

Every commit message states what was verified and how (command and outcome),
and names anything it did not verify. A check that has not been watched
failing is not yet enforcement — before a new gate is added to CI, break the
subject locally, watch the check fail, restore, and say so in the commit that
adds it.

## Not claimed

This structure enforces build health, formatting, lint cleanliness, vector
agreement, registry exactness, and proof compilation. It does not verify
semantic replay across processes, prove that the Rust implementation refines
the Lean model, establish any custody or consensus property, or judge whether
the interaction model is good. Those boundaries belong to
[PROTOCOL.md](PROTOCOL.md) and the README.
