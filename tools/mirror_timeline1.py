#!/usr/bin/env python3
"""Mirror the timeline-1 working tree into this repository as commits.

Timeline 1 is C:/Users/Owen/bitblock — the original directory, actively
worked by another agent, with no version control of its own. This tool
never writes inside timeline 1. It copies the source tree (minus build
artifacts, using the same exclusions as the fork) into a git worktree on
the `mirror/timeline-1` branch and commits the exact state with a
timestamp, producing one commit per observed state of the other line.

The mirror branch is rooted at the `inherited-state` genesis, so every
mirror commit's diff IS the other agent's progress since the fork, and
`git diff mirror/timeline-1 timeline-2` is the divergence between the
lines. Runs with no arguments; exits 0 with a one-line summary. Safe to
run repeatedly; a state identical to the last mirror commits nothing.
"""

from __future__ import annotations

import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

SOURCE = Path(r"C:/Users/Owen/bitblock")
REPO = Path(r"C:/Users/Owen/bitblock-t2")
WORKTREE = REPO / "mirror-wt-gm"
BRANCH = "mirror/timeline-1"

# The same exclusions the fork used, so mirror trees diff cleanly against
# the genesis: build outputs are not source. /XF .git protects the gitdir
# pointer file inside the worktree from /MIR's dest-side deletion.
ROBOCOPY_ARGS = [
    "robocopy", str(SOURCE), str(WORKTREE),
    "/MIR", "/NFL", "/NDL", "/NJH", "/NP", "/NC", "/NS",
    # .git must be in /XD (directories), not only /XF (files): when timeline-1
    # gained its own .git directory (2026-08-27 ~05:08), /XF alone let robocopy
    # copy their repo OVER the worktree's .git pointer file, converting the
    # worktree into a clone of timeline-1 mid-run - the "parallel session" that
    # was never there.
    "/XD", "target", "__pycache__", ".lake", ".git",
    "/XF", ".git",
]


def git(*args: str, cwd: Path = REPO, check: bool = True) -> str:
    r = subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if check and r.returncode != 0:
        sys.exit(f"git {' '.join(args)} failed ({r.returncode}):\n{r.stderr}")
    return r.stdout


def main() -> None:
    if not SOURCE.is_dir():
        sys.exit(f"mirror: source {SOURCE} not found")

    # Ensure the worktree + branch exist, rooted at the fork point.
    branches = git("branch", "--list", BRANCH)
    if not branches.strip():
        git("branch", BRANCH, "inherited-state")
    if WORKTREE.is_dir():
        # IDENTITY GUARD (2026-08-27): a parallel session once replaced the worktree
        # directory with a fresh repository mid-run, and this tool committed into the
        # foreign history. Refuse unless the directory is a worktree of THIS repo.
        common = git("rev-parse", "--path-format=absolute", "--git-common-dir",
                     cwd=WORKTREE).strip()
        if Path(common).resolve() != (REPO / ".git").resolve():
            sys.exit(f"mirror: {WORKTREE} is not a worktree of {REPO} (common dir "
                     f"{common}) - a parallel session may have replaced it. Refusing; "
                     f"the observation must be recovered by hand.")
        head = git("branch", "--show-current", cwd=WORKTREE).strip()
        if head != BRANCH:
            sys.exit(f"mirror: {WORKTREE} is on '{head}', expected '{BRANCH}' - refusing")
    else:
        git("worktree", "add", str(WORKTREE), BRANCH)

    # Copy the exact source state over the worktree.
    r = subprocess.run(ROBOCOPY_ARGS, capture_output=True, text=True)
    if r.returncode >= 8:
        sys.exit(f"mirror: robocopy failed ({r.returncode}):\n{r.stdout}\n{r.stderr}")

    status = git("status", "--porcelain", cwd=WORKTREE)
    if not status.strip():
        print("mirror: no change since the last observed state")
        return

    n = len(status.splitlines())
    when = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    git("add", "-A", cwd=WORKTREE)
    git(
        "commit", "-q",
        "-m", f"mirror: timeline-1 observed state, {when}",
        "-m", f"{n} changed paths vs the previous observed state. The source "
              f"directory has no version control; this commit is the observed "
              f"tree, copied read-only (target/, __pycache__/, .lake excluded "
              f"as at the fork). Produced by tools/mirror_timeline1.py.",
        cwd=WORKTREE,
    )
    # POST-COMMIT VERIFICATION (2026-08-27): a parallel session twice replaced the
    # worktree directory mid-run. Verify from the MAIN repo that the commit we just
    # made is the tip of OUR branch; if it landed in a foreign repository, say so.
    tip = git("log", "-1", "--format=%h", cwd=WORKTREE).strip()
    our_tip = git("log", "-1", "--format=%h", BRANCH).strip()
    if tip != our_tip:
        sys.exit(f"mirror: committed {tip} but {BRANCH} is at {our_tip} - the commit "
                 f"landed in a foreign repository (parallel session in the worktree "
                 f"path). The observation must be recovered by hand.")
    diffstat = git("diff", "--stat", "HEAD~1", "HEAD", cwd=WORKTREE).strip().splitlines()
    last = diffstat[-1] if diffstat else "(binary or rename-only changes)"
    print(f"mirror: committed {tip} ({when}); {last}")
    print(f"mirror: their line since fork: {git('rev-list', '--count', 'inherited-state..' + BRANCH).strip()} commits; "
          f"divergence `git diff {BRANCH} timeline-2` is live")


if __name__ == "__main__":
    main()
