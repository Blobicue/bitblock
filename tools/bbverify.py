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

`tools/bbmutate.py` measures whether that is true rather than assuming it:
25 of 25 modules fail when a canonical byte order is flipped.

Two tiers, because they detect different things at different speeds.

**quick** is the Rust tests plus clippy. This is what actually catches a moved
byte: the literal tag assertions live inside the Rust tests, so they run here.

**full** adds the independent Python vectors, `cargo fmt`, the metric registry
audit, and the index gates. The vector suites execute no Rust at all -- they
derive the expected digests from the schema -- so they cannot detect a Rust
change on their own. What they detect is the two implementations *disagreeing*,
which is why they belong to the per-commit tier rather than the inner loop.

`full` runs the Rust tests with `--features protocol-research`, which CI does
not. CI's rust job runs bare `cargo test`; the `all(windows, feature)` modules
are invisible to it. That gap is real -- see AGENTS.md.

**Why this is not just `cargo test`.** `cargo test` builds three test binaries
and then runs them one after another -- 2.5s + 2.6s + 2.3s, sequential, for
7.2s of mostly idle CPU. `cargo test --no-run` costs 0.4s warm and reports where
those binaries are, so bbverify builds once and runs them concurrently, with
clippy alongside. The tier is then as slow as its slowest single binary rather
than the sum, and a per-edit check costs about three seconds instead of eleven.

Every stage here has been watched failing before being trusted, per
docs/TIMELINES.md: "a check that has not been watched failing is not yet
enforcement".
"""

from __future__ import annotations

import json
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# Not bare validators. bbmutate deliberately rewrites src/ and restores it from
# git, so it must never run inside a verification sweep -- or in CI.
SKIP_BARE = {
    "metric_registry.py", "mirror_timeline1.py", "simulate_sequence.py",
    "bbindex.py", "bbgraph.py", "bbverify.py", "bbmutate.py",
}

CLIPPY = ["cargo", "clippy", "--all-targets", "-q", "--", "-D", "warnings"]


def run(command: list[str]) -> tuple[bool, str]:
    result = subprocess.run(
        command, cwd=ROOT, capture_output=True, text=True,
        encoding="utf-8", errors="replace",
    )
    if result.returncode == 0:
        return True, ""
    tail = (result.stdout + result.stderr).strip().splitlines()
    return False, "\n".join("      " + line for line in tail[-12:])


def build_tests(features: list[str]) -> tuple[bool, list[str], str]:
    """Compile the test binaries and report where they are.

    Warm this is ~0.4s. It is the only part that must happen before anything
    else, and splitting it out is what lets the actual test binaries run
    concurrently instead of cargo walking them one at a time.
    """
    command = ["cargo", "test", "--no-run", "-q", "--message-format=json", *features]
    result = subprocess.run(
        command, cwd=ROOT, capture_output=True, text=True,
        encoding="utf-8", errors="replace",
    )
    if result.returncode != 0:
        tail = (result.stdout + result.stderr).strip().splitlines()
        return False, [], "\n".join("      " + line for line in tail[-12:])
    binaries = []
    for line in result.stdout.splitlines():
        try:
            message = json.loads(line)
        except ValueError:
            continue
        if (
            message.get("reason") == "compiler-artifact"
            and message.get("executable")
            and message.get("profile", {}).get("test")
        ):
            target = message.get("target", {})
            kinds = target.get("kind") or ["?"]
            # lib and bin test binaries are both named after the crate, so the
            # target kind is the only thing that tells them apart in the output.
            binaries.append((f"{target.get('name', '?')} ({kinds[0]})",
                             message["executable"]))
    return True, binaries, ""


def run_binary(path: str) -> tuple[bool, str]:
    ok, detail = run([path, "--quiet"])
    return ok, detail


def run_vectors() -> tuple[bool, str]:
    tools = [
        t for t in sorted((ROOT / "tools").glob("*.py")) if t.name not in SKIP_BARE
    ]
    broken = []
    with ThreadPoolExecutor(max_workers=8) as pool:
        for tool, (ok, _detail) in zip(
            tools, pool.map(lambda t: run([sys.executable, "-B", str(t)]), tools)
        ):
            if not ok:
                broken.append(tool.name)
    if broken:
        return False, "      disagreed or failed: %s" % ", ".join(broken)
    return True, ""


def tier(name: str, features: list[str], extras) -> int:
    print("bbverify %s" % name)
    started = time.time()
    results = []

    mark = time.time()
    ok, binaries, detail = build_tests(features)
    results.append(("build test binaries", ok, time.time() - mark, detail,
                    "0.4s warm; splitting it out is what allows the rest to overlap"))
    if not ok:
        return report(results, started)

    # Everything below is independent, so it all runs at once. The tier costs
    # the slowest single job rather than the sum of them.
    jobs = [("clippy", lambda: run(CLIPPY),
             "lint clean is a CI gate, so a refactor that trips it is already broken")]
    for name, path in binaries:
        jobs.append(("test %s" % name, (lambda p: (lambda: run_binary(p)))(path),
                     "literal content tags plus behaviour"))
    jobs.extend(extras)

    with ThreadPoolExecutor(max_workers=len(jobs)) as pool:
        futures = [(label, why, pool.submit(fn)) for label, fn, why in jobs]
        mark = time.time()
        for label, why, future in futures:
            ok, detail = future.result()
            results.append((label, ok, None, detail, why))
        parallel = time.time() - mark
    print("  %-34s %-5s %5.1fs   %s" % (results[0][0], "ok", results[0][2], results[0][4]))
    for label, ok, _elapsed, _detail, why in results[1:]:
        print("  %-34s %-5s %5s   %s" % (label, "ok" if ok else "FAIL", "", why))
    print("  %-34s %-5s %5.1fs" % ("(the above ran concurrently)", "", parallel))
    return report(results, started, printed=True)


def report(results, started, printed: bool = False) -> int:
    if not printed:
        for label, ok, elapsed, _detail, why in results:
            print("  %-34s %-5s %5s   %s"
                  % (label, "ok" if ok else "FAIL",
                     ("%.1fs" % elapsed) if elapsed else "", why))
    failures = [(label, detail) for label, ok, _e, detail, _w in results if not ok]
    total = time.time() - started
    print()
    for label, detail in failures:
        print("  %s failed:" % label)
        if detail:
            print(detail)
    if failures:
        print("BROKEN -- %d of %d checks failed in %.1fs"
              % (len(failures), len(results), total))
        return 1
    print("NOTHING BROKE -- %d checks in %.1fs" % (len(results), total))
    return 0


def index_then_selftest() -> tuple[bool, str]:
    """Serial by necessity: bbindex writes index/, selftest reads it."""
    ok, detail = run([sys.executable, "tools/bbindex.py"])
    if not ok:
        return False, detail
    return run([sys.executable, "tools/bbgraph.py", "selftest"])


FULL_EXTRAS = [
    ("cargo fmt", lambda: run(["cargo", "fmt", "--check"]),
     "formatting is a CI gate; rustfmt does not format inside macro bodies"),
    ("python vectors", run_vectors,
     "24 independent reconstructions; they detect the two sides disagreeing"),
    ("metric registry", lambda: run([sys.executable, "-B", "tools/metric_registry.py", "audit"]),
     "1,112 aliases must still resolve to their definition tags"),
    ("index + selftest", index_then_selftest,
     "three accounting gates, then the recorded expectations"),
]


def main(argv: list[str]) -> int:
    mode = (argv[1] if len(argv) > 1 else "full").lower()
    if mode == "quick":
        return tier("quick", [], [])
    if mode == "full":
        return tier("full", ["--features", "protocol-research"], FULL_EXTRAS)
    if mode == "list":
        print("quick   build the test binaries, then run them and clippy at once.")
        print("        Catches a moved canonical byte. Per edit.")
        print("full    the same with --features protocol-research, plus fmt, the")
        print("        independent Python vectors, the metric registry, and the")
        print("        index gates. Per commit.")
        return 0
    print(__doc__)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
