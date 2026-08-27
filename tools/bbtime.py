#!/usr/bin/env python3
"""bbtime.py -- measure where verification time actually goes, per test and per module.

    python tools/bbtime.py              # serial and accurate; writes target/bbtiming.json
    python tools/bbtime.py --best-of-3  # three samples per test, keep the fastest
    python tools/bbtime.py --parallel   # rough sweep only; see the warning below
    python tools/bbtime.py --top 30     # show more rows
    python tools/bbtime.py --build      # what it costs to touch each module

The point is that bottlenecks should surface themselves rather than be guessed
at. Every previous speedup in this repository came from measuring and finding
the cost somewhere other than where it looked: the index's 18s was a per-item
regex sweep and not the obvious string joining, and the full tier's 8s was a
single test hashing a 27MB executable six times.

Time is attributed to the module that owns the test, so it lands on the same
nodes `bbgraph dag` already ranks. That answers a question neither tool can
alone: `protocol` is reached by 29 of 33 modules, and now its verification cost
is a number rather than an intuition.

The output is written to target/ rather than index/, deliberately. It is a
measurement of this machine on this day -- it would differ on every host, and
index/ has to regenerate byte-identically from the tree.

**On the numbers.** Each test runs in its own process with `--test-threads 1`,
and process start is measured and subtracted.

`--parallel` is a rough sweep and its **ranking is not trustworthy**. Eight
concurrent children, each opening a full libtest thread pool, oversubscribe the
machine badly enough to reorder the results: against best-of-three isolated
runs, a 0.04s test was reported at 3.88s and ranked slowest of all, while a
genuinely 1.93s test came fourth. That was measured, not feared, and it is why
serial is the default.

Serial numbers are still wall-clock samples rather than a benchmark. Take a
difference under about 20% between two rows as noise, and use `--best-of-3` when
a specific pair matters.

**Per-test isolation double-counts anything a process pays once.** The
executable fingerprint is memoised per process, so in a real run the 27MB hash
happens once for all 53 bin tests; here every test that touches it pays again.
Isolated cost answers "what does this test cost alone", not "what would deleting
it save".

**The sum is not the wall clock.** ~30s of measured test cost becomes ~3.5s of
tier, because the binaries run concurrently and libtest parallelises within
each. What sets the floor is the slowest single binary, and inside that, its
slowest test. Optimise the longest pole, never the largest sum.
"""

from __future__ import annotations

import json
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "target" / "bbtiming.json"


def build(features: list[str]) -> list[tuple[str, str]]:
    result = subprocess.run(
        ["cargo", "test", "--no-run", "-q", "--workspace", "--message-format=json", *features],
        cwd=ROOT, capture_output=True, text=True, encoding="utf-8", errors="replace",
    )
    if result.returncode != 0:
        sys.exit("bbtime: test binaries do not build\n" + result.stderr[-2000:])
    out = []
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
            out.append(("%s (%s)" % (target.get("name", "?"), kinds[0]),
                        message["executable"]))
    return out


def test_names(exe: str) -> list[str]:
    listing = subprocess.run([exe, "--list"], cwd=ROOT, capture_output=True,
                             text=True, encoding="utf-8", errors="replace").stdout
    return [line.rsplit(": ", 1)[0] for line in listing.splitlines()
            if line.endswith(": test")]


def start_overhead(exe: str) -> float:
    """Cost of starting the binary with a filter that matches nothing."""
    samples = []
    for _ in range(3):
        mark = time.time()
        subprocess.run([exe, "--quiet", "bbtime_no_such_test"], cwd=ROOT,
                       capture_output=True)
        samples.append(time.time() - mark)
    return min(samples)


def owning_module(binary: str, test: str) -> str:
    """`frame::tests::foo` belongs to frame; a bin test belongs to main."""
    parts = test.split("::")
    if binary.endswith("(lib)") and len(parts) > 1:
        return parts[0]
    if binary.endswith("(bin)"):
        return "main"
    return binary.split(" ")[0]


def rebuild_cost(argv: list[str]) -> int:
    """What it costs to touch each module, which is now the whole edit path.

    Once tests take half a second, verification is entirely the Rust toolchain,
    and this is the part `bbgraph dag` cannot see. The DAG knows
    `coordinate_episode` is a leaf nothing depends on; the compiler charges the
    same for editing it as for editing `frame`, because the crate is one
    compilation unit and the DAG is not a build boundary.

    That gap is the measurement. A module whose rebuild cost is far above its
    dependent count is paying for coupling the source does not actually have.
    """
    import json as _json

    wanted = [a for a in argv[2:] if not a.startswith("--")]
    dag_path = ROOT / "index" / "dag.json"
    dependents = {}
    if dag_path.exists():
        dependents = _json.loads(dag_path.read_text(encoding="utf-8"))\
            .get("transitive_dependents", {})

    dirs = (ROOT / "crates" / "core" / "src", ROOT / "crates" / "research" / "src",
            ROOT / "src")
    found = {p.stem: p for d in dirs for p in d.glob("*.rs") if p.stem != "lib"}
    modules = sorted(n for n in found if not wanted or n in wanted)
    command = ["cargo", "test", "--no-run", "-q", "--workspace",
               "--features", "protocol-research"]

    def run() -> float:
        mark = time.time()
        subprocess.run(command, cwd=ROOT, capture_output=True)
        return time.time() - mark

    print("bbtime --build   %d modules, one touch each" % len(modules))
    print("  a module far above its dependent count is paying for coupling it does not have\n")
    run()
    rows = []
    for name in modules:
        path = found[name]
        # Bytes, not text. read_text translates CRLF to LF and write_text would
        # put it back as LF, silently rewriting the line endings of every module
        # probed. Nothing would break, but the working tree churns and
        # bbverify's cache hashes file content -- so a probe would invalidate
        # every check it touched.
        original = path.read_bytes()
        try:
            path.write_bytes(original + b"\n// bbtime probe\n")
            cost = run()
        finally:
            path.write_bytes(original)
            run()
        rows.append((cost, name, dependents.get(name, 0)))
        print("  %-24s %5.1fs   %2d dependents" % (name, cost, dependents.get(name, 0)))

    rows.sort(reverse=True)
    out = ROOT / "target" / "bbtiming-build.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(_json.dumps(
        {"modules": [{"module": n, "seconds": round(c, 2), "dependents": d}
                     for c, n, d in rows]}, indent=1),
        encoding="utf-8", newline="\n")
    print("\nslowest to touch:")
    for cost, name, deps in rows[:8]:
        print("  %5.1fs  %-24s %2d dependents" % (cost, name, deps))
    print("\nwritten to %s" % out.relative_to(ROOT).as_posix())
    return 0


def main(argv: list[str]) -> int:
    if "--build" in argv:
        return rebuild_cost(argv)
    # Serial by default, because the parallel mode does not merely inflate the
    # numbers -- it reorders them. Measured against best-of-three isolated runs,
    # eight concurrent children each spawning a full libtest thread pool
    # oversubscribed sixteen cores so badly that a 0.04s test was reported at
    # 3.88s and ranked slowest of all, while a genuinely 1.93s test came fourth.
    # A profiler that mis-ranks is worse than no profiler, so --parallel is now
    # opt-in and only fit for a rough sweep.
    parallel = "--parallel" in argv
    repeats = 3 if "--best-of-3" in argv else 1
    top = 20
    if "--top" in argv:
        top = int(argv[argv.index("--top") + 1])

    binaries = build(["--features", "protocol-research"])
    print("bbtime  %d test binaries, %s%s"
          % (len(binaries),
             "8 concurrent workers (RANKING UNRELIABLE)" if parallel else "serial",
             ", best of %d" % repeats if repeats > 1 else ""))

    rows = []
    started = time.time()
    for label, exe in binaries:
        names = test_names(exe)
        overhead = start_overhead(exe)
        print("  %-22s %3d tests, process start %.3fs" % (label, len(names), overhead))

        def timed(name: str):
            best = None
            for _ in range(repeats):
                mark = time.time()
                # One test does not need a thread pool, and letting each child
                # open one is what wrecked the parallel mode.
                subprocess.run([exe, "--quiet", "--exact", name,
                                "--test-threads", "1"],
                               cwd=ROOT, capture_output=True)
                sample = time.time() - mark - overhead
                best = sample if best is None else min(best, sample)
            return name, max(best or 0.0, 0.0)

        if parallel:
            with ThreadPoolExecutor(max_workers=8) as pool:
                measured = list(pool.map(timed, names))
        else:
            measured = [timed(n) for n in names]
        for name, cost in measured:
            rows.append({"binary": label, "test": name, "seconds": round(cost, 4),
                         "module": owning_module(label, name)})

    elapsed = time.time() - started
    by_module: dict[str, dict] = {}
    for row in rows:
        entry = by_module.setdefault(row["module"], {"seconds": 0.0, "tests": 0})
        entry["seconds"] += row["seconds"]
        entry["tests"] += 1
    for entry in by_module.values():
        entry["seconds"] = round(entry["seconds"], 3)

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps({
        "mode": "parallel" if parallel else "serial",
        "repeats": repeats,
        "profiled_in_seconds": round(elapsed, 1),
        "by_module": dict(sorted(by_module.items(),
                                 key=lambda kv: -kv[1]["seconds"])),
        "tests": sorted(rows, key=lambda r: -r["seconds"]),
    }, indent=1), encoding="utf-8", newline="\n")

    total = sum(r["seconds"] for r in rows)
    print("\nSLOWEST MODULES  (summed test cost)")
    for module, entry in sorted(by_module.items(), key=lambda kv: -kv[1]["seconds"])[:top]:
        share = 100.0 * entry["seconds"] / total if total else 0.0
        print("  %-26s %7.2fs  %4.1f%%  across %d tests"
              % (module, entry["seconds"], share, entry["tests"]))

    print("\nSLOWEST TESTS")
    for row in sorted(rows, key=lambda r: -r["seconds"])[:top]:
        print("  %7.3fs  %-10s %s" % (row["seconds"], row["module"],
                                      row["test"].split("::")[-1][:58]))

    print("\n%d tests, %.1fs of measured test cost, profiled in %.0fs"
          % (len(rows), total, elapsed))
    print("written to %s" % OUT.relative_to(ROOT).as_posix())
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
