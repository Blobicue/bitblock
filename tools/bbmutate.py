#!/usr/bin/env python3
"""bbmutate.py -- measure whether the test suite would actually notice.

    python tools/bbmutate.py            # every module with a canonical encoder
    python tools/bbmutate.py frame tick # just these

A green suite proves nothing on its own; it has to be shown *failing* for the
change you care about. This crate's whole claim is byte-exact canonical
encodings, so the change worth measuring is a canonical byte moving.

The mutation is surgical: every `to_be_bytes()` in one module's production scope
becomes `to_le_bytes()`. That is exactly a byte-order change inside the hashed
domain and nothing else -- no behaviour, no error text, no control flow. If a
module's tests pin its canonical bytes, this must fail them. If it survives, the
suite does not pin that module's encoding and a refactor there is unguarded.

Whole-module rather than per-site, deliberately: the question is "does anything
notice this module's bytes changing", not which line is best covered. It also
sidesteps a false blind spot -- `u8::to_be_bytes` and `to_le_bytes` are the same
single byte, so a one-site run on a u8 would survive while proving nothing.

Local only, never CI. It rewrites files and restores them from git, so the tree
must be clean before it runs.
"""

from __future__ import annotations

import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC_DIRS = (ROOT / "crates" / "core" / "src",
            ROOT / "crates" / "research" / "src",
            ROOT / "src")


def git(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", *args], cwd=ROOT, capture_output=True, text=True,
        encoding="utf-8", errors="replace",
    )


def test_boundary(lines: list[str]) -> int:
    for num, line in enumerate(lines):
        if line.strip() == "#[cfg(test)]":
            for follower in lines[num + 1:]:
                if not follower.strip():
                    continue
                if follower.lstrip().startswith("mod "):
                    return num
                break
    return len(lines)


def mutate(path: Path) -> int:
    """Flip every to_be_bytes() in production scope. Returns the site count."""
    lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
    edge = test_boundary([l.rstrip("\n") for l in lines])
    count = 0
    for i in range(edge):
        swapped, n = re.subn(r"to_be_bytes\(\)", "to_le_bytes()", lines[i])
        if n:
            lines[i] = swapped
            count += n
    if count:
        path.write_text("".join(lines), encoding="utf-8", newline="")
    return count


def main(argv: list[str]) -> int:
    # Only src/ has to be clean: that is the only thing restored, and requiring a
    # clean tree everywhere blocks the run on unrelated edits, including this
    # tool's own.
    dirty = git("status", "--porcelain", "--", "src", "crates").stdout.strip()
    if dirty:
        print("bbmutate: src/ must be clean -- it is restored from git.\n%s" % dirty)
        return 2

    wanted = set(argv[1:])
    targets = []
    for path in sorted(p for d in SRC_DIRS for p in d.glob("*.rs")):
        if wanted and path.stem not in wanted:
            continue
        text = path.read_text(encoding="utf-8")
        edge = test_boundary(text.splitlines())
        if "to_be_bytes()" in "\n".join(text.splitlines()[:edge]):
            targets.append(path)
    if not targets:
        print("bbmutate: no module in scope has a canonical encoder to mutate")
        return 2

    print("bbmutate  %d modules, one whole-module byte-order flip each" % len(targets))
    print("  a KILLED module has its canonical bytes pinned; a SURVIVED one does not\n")
    killed, survived, broken = [], [], []
    started = time.time()
    try:
        for path in targets:
            sites = mutate(path)
            result = None
            crate_dir = path.parent.parent.name
            if crate_dir in ("core", "research"):
                pkg = "bitblock-core" if crate_dir == "core" else "bitblock-research"
                # Fast-screen the owning crate's unit tests for this module first
                # so a killed library module avoids relinking all 5 workspace test
                # binaries; fall back to the full workspace suite only if it survives.
                fast = subprocess.run(
                    ["cargo", "test", "-q", "-p", pkg, "--lib", path.stem],
                    cwd=ROOT, capture_output=True, text=True,
                    encoding="utf-8", errors="replace",
                )
                if fast.returncode != 0:
                    result = fast
            if result is None:
                result = subprocess.run(
                    # --features protocol-research, and every target rather than --lib.
                    # Without the feature the gated code is not compiled at all, so a
                    # mutation inside it cannot fail anything and reports as survived --
                    # main.rs's 18 canonical sites are all behind that flag and were
                    # never built. Without every target, main.rs is skipped entirely,
                    # because it is the binary rather than the library.
                    ["cargo", "test", "-q", "--workspace", "--features", "protocol-research"], cwd=ROOT,
                    capture_output=True, text=True, encoding="utf-8", errors="replace",
                )
            output = result.stdout + result.stderr
            if "error[" in output or "error:" in output and "test failed" not in output:
                verdict, bucket = "uncompilable", broken
            elif result.returncode != 0:
                verdict, bucket = "KILLED", killed
            else:
                verdict, bucket = "SURVIVED", survived
            bucket.append(path.stem)
            print("  %-28s %-12s %d sites" % (path.stem, verdict, sites))
            git("checkout", "--", str(path.relative_to(ROOT)).replace("\\", "/"))
    finally:
        git("checkout", "--", "src", "crates")

    total = time.time() - started
    print("\n  killed       %2d" % len(killed))
    print("  SURVIVED     %2d  %s" % (len(survived), ", ".join(survived)))
    if broken:
        print("  uncompilable %2d  %s" % (len(broken), ", ".join(broken)))
    print("\n%d of %d modules have their canonical bytes pinned  (%.0f%%), in %.0fs"
          % (len(killed), len(targets), 100.0 * len(killed) / len(targets), total))
    if survived:
        print("\nA survivor is not necessarily a bug: the module may encode only through")
        print("another module's tested path. It does mean nothing here would catch that")
        print("module's own bytes moving, so refactor it with more care than the rest.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
