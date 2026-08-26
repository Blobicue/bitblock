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
  vector tool must exit zero, and `tools/metric_registry.py audit` must report
  an exact manifest with zero unresolved references. The audit has been
  watched failing on a deliberately corrupted registry before being enforced.
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
today is that CI runs on every push to `main` and `timeline-2` and a red
run is loud and visible. Enabling hard protection is a one-command change
if the repository is later made public or the plan upgraded; that choice
belongs to the owner, not the timeline.

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
