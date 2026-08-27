#!/usr/bin/env python3
"""bbverify.py -- one command that says whether anything broke.

    python tools/bbverify.py quick     # per edit
    python tools/bbverify.py full      # per commit  (default)
    python tools/bbverify.py list      # what each tier runs, and why

The point of a refactor gate here is narrow and unusual: this crate's value is
byte-exact canonical encodings, so "nothing broke" means *no canonical byte
moved*. 101 literal content tags are asserted across the test suite and 24
Python suites reconstruct those same payloads without importing any Rust. A
refactor that changes a byte fails both, from opposite directions.

Two tiers, because they detect different things at different speeds.

**quick** is `cargo test` plus clippy. This is what actually catches a moved
byte: the literal tag assertions live inside the Rust tests, so they run here.
It is the per-edit loop and it is deliberately small.

**full** adds the independent Python vectors, `cargo fmt`, the metric registry
audit, and the index gates. The vector suites execute no Rust at all -- they
derive the expected digests from the schema -- so they cannot detect a Rust
change on their own. What they detect is the two implementations *disagreeing*,
which is why they belong to the per-commit tier rather than the inner loop.

`full` runs the Rust tests with `--features protocol-research`, which CI does
not. CI's rust job runs bare `cargo test`; the `all(windows, feature)` modules
are invisible to it. That gap is real -- see AGENTS.md.

Every stage here has been watched failing before being trusted, per
docs/TIMELINES.md: "a check that has not been watched failing is not yet
enforcement".
"""

from __future__ import annotations

import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

QUICK = [
    ("cargo test", ["cargo", "test", "-q"],
     "101 literal content tags plus behaviour; this is what catches a moved byte"),
    ("clippy", ["cargo", "clippy", "--all-targets", "-q", "--", "-D", "warnings"],
     "lint clean is a CI gate, so a refactor that trips it is already broken"),
]

FULL = [
    ("cargo test (protocol-research)",
     ["cargo", "test", "-q", "--features", "protocol-research"],
     "the windows+feature modules CI never compiles"),
    ("clippy", ["cargo", "clippy", "--all-targets", "-q", "--", "-D", "warnings"],
     "lint clean is a CI gate"),
    ("cargo fmt", ["cargo", "fmt", "--check"],
     "formatting is a CI gate; note rustfmt does not format inside macro bodies"),
    ("python vectors", None,
     "24 independent reconstructions; they detect the two sides disagreeing"),
    ("metric registry", ["python", "-B", "tools/metric_registry.py", "audit"],
     "1,112 aliases must still resolve to their definition tags"),
    ("index", ["python", "tools/bbindex.py"],
     "the three accounting gates"),
    ("index selftest", ["python", "tools/bbgraph.py", "selftest"],
     "the recorded expectations about what the index answers"),
]

# Not bare validators. bbmutate deliberately rewrites src/ and restores it from
# git, so it must never run inside a verification sweep -- or in CI.
SKIP_BARE = {
    "metric_registry.py", "mirror_timeline1.py", "simulate_sequence.py",
    "bbindex.py", "bbgraph.py", "bbverify.py", "bbmutate.py",
}


def run(command: list[str]) -> tuple[bool, str]:
    result = subprocess.run(
        command, cwd=ROOT, capture_output=True, text=True,
        encoding="utf-8", errors="replace",
    )
    if result.returncode == 0:
        return True, ""
    tail = (result.stdout + result.stderr).strip().splitlines()
    return False, "\n".join("      " + l for l in tail[-12:])


def run_vectors() -> tuple[bool, str]:
    broken = []
    for tool in sorted((ROOT / "tools").glob("*.py")):
        if tool.name in SKIP_BARE:
            continue
        ok, _ = run([sys.executable, "-B", str(tool)])
        if not ok:
            broken.append(tool.name)
    if broken:
        return False, "      disagreed or failed: %s" % ", ".join(broken)
    return True, ""


def tier(name: str, stages) -> int:
    print("bbverify %s" % name)
    started = time.time()
    failures = []
    for label, command, why in stages:
        mark = time.time()
        ok, detail = run_vectors() if command is None else run(command)
        elapsed = time.time() - mark
        print("  %-32s %-5s %5.1fs   %s"
              % (label, "ok" if ok else "FAIL", elapsed, why))
        if not ok:
            failures.append((label, detail))
    total = time.time() - started
    print()
    for label, detail in failures:
        print("  %s failed:" % label)
        if detail:
            print(detail)
    if failures:
        print("BROKEN -- %d of %d stages failed in %.1fs"
              % (len(failures), len(stages), total))
        return 1
    print("NOTHING BROKE -- %d stages in %.1fs" % (len(stages), total))
    return 0


def main(argv: list[str]) -> int:
    mode = (argv[1] if len(argv) > 1 else "full").lower()
    if mode == "quick":
        return tier("quick", QUICK)
    if mode == "full":
        return tier("full", FULL)
    if mode == "list":
        for name, stages in (("quick", QUICK), ("full", FULL)):
            print("%s:" % name)
            for label, _c, why in stages:
                print("  %-32s %s" % (label, why))
            print()
        return 0
    print(__doc__)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
