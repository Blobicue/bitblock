#!/usr/bin/env python3
"""bbtime.py -- measure where verification time actually goes, per test and per module.

    python tools/bbtime.py              # profile everything, write target/bbtiming.json
    python tools/bbtime.py --serial     # slower, but no contention in the numbers
    python tools/bbtime.py --top 30     # show more rows

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

**On the numbers.** Each test is run in its own process, so process start is
measured once and subtracted. Under `--parallel` (the default) tests contend for
cores, which inflates absolute times but leaves the ranking stable; `--serial`
gives true single-test cost at several times the wall clock. Neither is a
benchmark: they are wall-clock samples taken once, and a difference under about
20% between two rows is not a real difference.
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
        ["cargo", "test", "--no-run", "-q", "--message-format=json", *features],
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


def main(argv: list[str]) -> int:
    serial = "--serial" in argv
    top = 20
    if "--top" in argv:
        top = int(argv[argv.index("--top") + 1])

    binaries = build(["--features", "protocol-research"])
    print("bbtime  %d test binaries, %s"
          % (len(binaries), "serial" if serial else "8 concurrent workers"))

    rows = []
    started = time.time()
    for label, exe in binaries:
        names = test_names(exe)
        overhead = start_overhead(exe)
        print("  %-22s %3d tests, process start %.3fs" % (label, len(names), overhead))

        def timed(name: str):
            mark = time.time()
            subprocess.run([exe, "--quiet", "--exact", name], cwd=ROOT,
                           capture_output=True)
            return name, max(time.time() - mark - overhead, 0.0)

        if serial:
            measured = [timed(n) for n in names]
        else:
            with ThreadPoolExecutor(max_workers=8) as pool:
                measured = list(pool.map(timed, names))
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
        "mode": "serial" if serial else "parallel",
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
