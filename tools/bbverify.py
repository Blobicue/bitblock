#!/usr/bin/env python3
"""bbverify.py -- one command that says whether anything broke.

    python tools/bbverify.py quick        # per edit
    python tools/bbverify.py full         # per commit  (default)
    python tools/bbverify.py full --force # ignore the cache and run everything
    python tools/bbverify.py list         # what each check reads, and why

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

Three things make it fast, in the order they matter.

**Every check declares what it reads.** Editing `docs/PROTOCOL.md` cannot change
what `cargo test` does, so a check whose inputs are byte-identical to the last
run that passed is skipped rather than repeated. This is the same
content-addressing the crate itself is built on: the inputs are hashed, and an
unchanged digest means the answer is already known. It is also why the checks
are declarative records rather than a script -- the input set is the interesting
half.

**One build, then everything at once.** `cargo test` compiles three test
binaries and walks them one after another -- 2.5s + 2.6s + 2.3s of mostly idle
CPU on a sixteen-core machine. `cargo test --no-run` costs 0.4s warm and reports
where those binaries are, so they run concurrently, with clippy and the Python
work alongside. A tier costs its slowest single job instead of the sum.

**The cache is conservative.** It records only checks that passed, it keys on
this file's own bytes as well as the subject's, and anything it cannot read
makes it run rather than skip. A stale skip would be a silent false green, which
is worse than any amount of waiting.

Every stage here has been watched failing before being trusted, per
docs/TIMELINES.md: "a check that has not been watched failing is not yet
enforcement".
"""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT / "target" / ".bbverify-cache.json"

# Not bare validators, so the sweep must not run them. bbmutate rewrites src/ and
# restores it from git; bbtime spends half a minute profiling every test. Both
# were caught by this file's own per-job timing after being added to tools/ --
# bbtime turned the vector sweep from 3s into 29s, and until each job reported
# its own seconds the concurrent group was one opaque number that had simply
# grown.
SKIP_BARE = {
    "metric_registry.py", "mirror_timeline1.py", "simulate_sequence.py",
    "bbindex.py", "bbgraph.py", "bbverify.py", "bbmutate.py", "bbtime.py",
}

# What each kind of check reads. A check is skipped only when every path here is
# byte-identical to the last run that passed it.
RUST_INPUTS = ("src/**/*.rs", "crates/*/src/**/*.rs", "tests/**/*.rs",
               "examples/**/*.rs", "Cargo.toml", "crates/*/Cargo.toml",
               "Cargo.lock", "rust-toolchain.toml")
VECTOR_INPUTS = ("tools/*.py",)
REGISTRY_INPUTS = ("tools/metric_registry.py", "docs/metrics/*", "docs/*.md",
                   "README.md", "METRICS.md")
INDEX_INPUTS = ("src/**/*.rs", "crates/*/src/**/*.rs", "tools/*.py", "tools/*.json", "docs/**/*.md",
                "docs/metrics/*", "examples/*", "tests/*.rs",
                "README.md", "METRICS.md", "proof/**/*.lean", "proof/README.md")

CLIPPY = ["cargo", "clippy", "--all-targets", "-q", "--", "-D", "warnings"]
# Clippy gets its own target directory so it does not queue behind the test
# build on cargo's lock. Sharing one, the two cargo invocations serialise: 7.1s
# after an edit against 4.7s when they genuinely overlap. It costs ~158MB inside
# target/, which is already ignored and far larger.
CLIPPY_ENV = {"CARGO_TARGET_DIR": str(ROOT / "target" / "clippy")}


def digest(patterns) -> str:
    """A digest of every file the patterns name, or "" if anything is unreadable.

    Returning "" on any failure is deliberate: an empty digest never matches a
    stored one, so an unreadable input makes the check run. The cache may only
    ever cost time, never correctness.
    """
    hasher = hashlib.sha256()
    try:
        paths = sorted({p for pattern in patterns for p in ROOT.glob(pattern)})
        for path in paths:
            if not path.is_file():
                continue
            hasher.update(path.relative_to(ROOT).as_posix().encode())
            hasher.update(path.read_bytes())
        # This file decides what every check means, so its own bytes are part of
        # every key: editing bbverify invalidates the whole cache.
        hasher.update(Path(__file__).read_bytes())
    except OSError:
        return ""
    return hasher.hexdigest()


def load_cache() -> dict:
    try:
        return json.loads(CACHE.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}


def save_cache(cache: dict) -> None:
    try:
        CACHE.parent.mkdir(parents=True, exist_ok=True)
        CACHE.write_text(json.dumps(cache, indent=1, sort_keys=True),
                         encoding="utf-8", newline="\n")
    except OSError:
        pass  # a cache that cannot be written is a slow run, not a wrong one


def run(command: list[str], env: dict | None = None) -> tuple[bool, str]:
    environment = None
    if env:
        environment = dict(os.environ)
        environment.update(env)
    result = subprocess.run(
        command, cwd=ROOT, capture_output=True, text=True,
        encoding="utf-8", errors="replace", env=environment,
    )
    if result.returncode == 0:
        return True, ""
    tail = (result.stdout + result.stderr).strip().splitlines()
    return False, "\n".join("      " + line for line in tail[-12:])


def build_tests(features: list[str]) -> tuple[bool, list, str]:
    """Compile the test binaries and report where they are.

    Warm this is ~0.4s. Splitting it out is what lets the binaries run
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
            # target kind is the only thing telling them apart in the output.
            binaries.append(("%s (%s)" % (target.get("name", "?"), kinds[0]),
                             message["executable"]))
    return True, binaries, ""


def run_vectors() -> tuple[bool, str]:
    tools = [t for t in sorted((ROOT / "tools").glob("*.py")) if t.name not in SKIP_BARE]
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


def index_then_selftest() -> tuple[bool, str]:
    """Serial by necessity: bbindex writes index/, selftest reads it."""
    ok, detail = run([sys.executable, "tools/bbindex.py"])
    if not ok:
        return False, detail
    return run([sys.executable, "tools/bbgraph.py", "selftest"])


EXTRA_CHECKS = {
    "cargo fmt": (RUST_INPUTS, lambda: run(["cargo", "fmt", "--check"]),
                  "formatting is a CI gate; rustfmt skips macro bodies"),
    "python vectors": (VECTOR_INPUTS, run_vectors,
                       "24 independent reconstructions; they catch the two sides disagreeing"),
    "metric registry": (REGISTRY_INPUTS,
                        lambda: run([sys.executable, "-B", "tools/metric_registry.py", "audit"]),
                        "1,112 aliases must still resolve to their definition tags"),
    "index + selftest": (INDEX_INPUTS, index_then_selftest,
                         "three accounting gates, then the recorded expectations"),
}


def tier(name: str, features: list[str], extras: dict, force: bool) -> int:
    print("bbverify %s%s" % (name, "  (--force)" if force else ""))
    started = time.time()
    cache = {} if force else load_cache()
    rust_key = digest(RUST_INPUTS)
    rows, jobs = [], []

    # The Rust checks share one input set, so one digest decides all of them --
    # including whether the build is needed at all.
    rust_fresh = bool(rust_key) and cache.get("rust::" + name) == rust_key
    binaries = []
    clippy_pool = clippy_future = None
    if rust_fresh:
        rows.append(("rust tests + clippy", "skip", None,
                     "unchanged since the last pass"))
    else:
        # Clippy starts here rather than joining the group below, because after a
        # source edit it is the second-largest cost (3.2s against the build's
        # 5.2s) and nothing it needs comes from the build. On its own target dir
        # the two overlap instead of queueing on cargo's lock: 7.1s to 4.7s.
        clippy_pool = ThreadPoolExecutor(max_workers=1)
        clippy_started = time.time()
        clippy_future = clippy_pool.submit(lambda: run(CLIPPY, CLIPPY_ENV))

        mark = time.time()
        ok, binaries, detail = build_tests(features)
        rows.append(("build test binaries", "ok" if ok else "FAIL",
                     time.time() - mark, "0.4s warm; lets the rest overlap"))
        if not ok:
            clippy_future.result()
            clippy_pool.shutdown()
            return report(rows, [("build test binaries", detail)], started)
        for label, path in binaries:
            jobs.append(("test %s" % label,
                         (lambda p: (lambda: run([p, "--quiet"])))(path),
                         "literal content tags plus behaviour"))

    keys = {}
    for label, (inputs, fn, why) in extras.items():
        key = digest(inputs)
        keys[label] = key
        if key and cache.get(label) == key:
            rows.append((label, "skip", None, "unchanged since the last pass"))
        else:
            jobs.append((label, fn, why))

    failures = []
    parallel = 0.0
    if jobs:
        # Each job times itself. Without this the concurrent group is one opaque
        # number, and a job that quietly became the bottleneck is invisible --
        # which is exactly how a 5s tier drifted to 20s without anything saying
        # which part had changed.
        def measured(label, fn):
            def go():
                mark = time.time()
                ok, detail = fn()
                return ok, detail, time.time() - mark
            return go

        mark = time.time()
        with ThreadPoolExecutor(max_workers=len(jobs)) as pool:
            futures = [(label, why, pool.submit(measured(label, fn)))
                       for label, fn, why in jobs]
            for label, why, future in futures:
                ok, detail, elapsed = future.result()
                rows.append((label, "ok" if ok else "FAIL", elapsed, why))
                if not ok:
                    failures.append((label, detail))
        parallel = time.time() - mark

    if clippy_future is not None:
        ok, detail = clippy_future.result()
        clippy_pool.shutdown()
        rows.append(("clippy", "ok" if ok else "FAIL",
                     time.time() - clippy_started,
                     "own target dir, started alongside the build"))
        if not ok:
            failures.append(("clippy", detail))

    if not failures:
        if not rust_fresh and rust_key:
            cache["rust::" + name] = rust_key
        for label, key in keys.items():
            if key:
                cache[label] = key
        save_cache(cache)

    return report(rows, failures, started, parallel)


def report(rows, failures, started, parallel: float = 0.0) -> int:
    ran = 0
    for label, verdict, elapsed, why in rows:
        stamp = ("%.1fs" % elapsed) if elapsed else ""
        print("  %-34s %-5s %6s   %s" % (label, verdict, stamp, why))
        if verdict != "skip":
            ran += 1
    if parallel:
        print("  %-34s %-5s %6.1fs" % ("(concurrent)", "", parallel))
    print()
    for label, detail in failures:
        print("  %s failed:" % label)
        if detail:
            print(detail)
    total = time.time() - started
    skipped = len(rows) - ran
    tail = ", %d skipped as unchanged" % skipped if skipped else ""
    if failures:
        print("BROKEN -- %d of %d checks failed in %.1fs%s"
              % (len(failures), ran, total, tail))
        return 1
    print("NOTHING BROKE -- %d checks in %.1fs%s" % (ran, total, tail))
    return 0


def main(argv: list[str]) -> int:
    args = [a for a in argv[1:] if not a.startswith("--")]
    force = "--force" in argv
    mode = (args[0] if args else "full").lower()
    if mode == "quick":
        return tier("quick", [], {}, force)
    if mode == "full":
        return tier("full", ["--features", "protocol-research"], EXTRA_CHECKS, force)
    if mode == "list":
        print("rust tests + clippy  reads: %s" % ", ".join(RUST_INPUTS))
        for label, (inputs, _fn, why) in EXTRA_CHECKS.items():
            print("%-20s reads: %s" % (label, ", ".join(inputs)))
            print("%-20s   %s" % ("", why))
        print("\nA check whose inputs are unchanged since the last pass is skipped.")
        print("--force ignores the cache.")
        return 0
    print(__doc__)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
