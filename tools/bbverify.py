#!/usr/bin/env python3
"""bbverify.py -- one command that says whether anything broke.

    python tools/bbverify.py quick        # per edit
    python tools/bbverify.py full         # per commit  (default)
    python tools/bbverify.py full --force # ignore the cache and run everything
    python tools/bbverify.py list         # what each check reads, and why

The point of a refactor gate here is narrow and unusual: this crate's value is
byte-exact canonical encodings, so "nothing broke" means *no canonical byte
moved*. 101 literal content tags are asserted across the test suite and the
`tools/*_vectors.py` suites reconstruct those same payloads without importing any Rust. A
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

Both tiers run `--workspace`: without it cargo tests only the root package, and
after the workspace split that was 43 tests of 397 with every gate still green.
`full` adds `--features protocol-research`, which is also what CI now runs.

How many tests ran is therefore a gate of its own -- see EXPECTED_TESTS.

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
import re
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
    "check_planning_docs.py",
    "metric_registry.py", "mirror_timeline1.py", "simulate_sequence.py",
    "bbindex.py", "bbgraph.py", "bbverify.py", "bbmutate.py", "bbtime.py",
}


def vector_suites() -> list[Path]:
    """Every bare validator in tools/, globbed rather than enumerated.

    A suite dropped into tools/ is run without anyone remembering to register
    it, and the count reported below cannot drift from the count that ran. The
    label used to be a hand-written "32"; it stayed 32 when the thirty-third
    arrived, which is the same silent-shrink failure EXPECTED_TESTS guards.
    """
    return [p for p in sorted((ROOT / "tools").glob("*.py")) if p.name not in SKIP_BARE]

# What each kind of check reads. A check is skipped only when every path here is
# byte-identical to the last run that passed it.
RUST_INPUTS = ("src/**/*.rs", "crates/*/src/**/*.rs", "crates/core/src/chem/fixtures/*.txt",
               "docs/gallery/*.bb", "docs/metrics/registry.tsv", "examples/*.tape",
               "tests/**/*.rs", "examples/**/*.rs", "Cargo.toml", "crates/*/Cargo.toml",
               "Cargo.lock", "rust-toolchain.toml")
VECTOR_INPUTS = ("tools/*.py", "docs/*.md")
PLANNING_INPUTS = ("tools/check_planning_docs.py", "docs/**/*.md",
                   "docs/design-source-receipt-v1.json", "README.md", ".gitattributes")
REGISTRY_INPUTS = ("tools/metric_registry.py", "docs/metrics/*", "docs/*.md",
                   "README.md", "METRICS.md")
INDEX_INPUTS = ("src/**/*.rs", "crates/*/src/**/*.rs", "tools/*.py", "tools/*.json", "docs/**/*.md",
                "docs/metrics/*", "examples/*", "tests/*.rs",
                "README.md", "METRICS.md", "proof/**/*.lean", "proof/README.md")

CLIPPY = ["cargo", "clippy", "--workspace", "--all-targets", "-q"]

# A floor on how many tests must actually execute.
#
# This exists because the suite once shrank silently. Splitting into a workspace
# made `cargo test` run only the root package -- 43 tests of 397 -- and every
# gate stayed green, including CI, because each one was asking "did what ran
# pass" and none was asking "did everything run". A suite that quietly stops
# running is indistinguishable from a suite that passes.
#
# Raise it when tests are added. If it ever needs lowering, that is a deletion
# and it belongs in the commit message.
# 386 -> 389 / 397 -> 400: three literal-tag tests, one each in sequential.rs,
# replay_evidence.rs and optimization.rs, pinning the seven newly vectored
# schemas against their independent Python reconstructions.
# 394 -> 400 / 405 -> 411: six ACCOUNT-1 tests -- three in tick.rs for the
# control lanes, the reconciliation rule and the over-limit refusal, three in
# main.rs for the live path, the refused action and startup-program edits.
# 400 -> 402 / 411 -> 413: the swapped-lane control, which pins that the
# reconciliation is per-lane and not per-total, and the replayed-tape test
# that accounts for control actions arriving from a tape.
# mathematical over-limit total from debug overflow and release wrapping.
# 403 -> 406 / 414 -> 417: three production-path gates for the exact-edit world
# seam -- equal admitted streams across Resize contexts, one moved coordinate,
# and separately characterized viewport-dependent local aim resolution.
# 406 -> 411 / 417 -> 422: five COMMIT-R1 tests in commit_r1.rs for CoordinateKeyV1,
# SMT and Patricia determinism, point proof verification, side-by-side assay, and rebuild parity.
# 411 -> 417 / 422 -> 428: six LOCAL-BOUNDS-1 tests in local_bounds.rs for LocalStorageProfile,
# checked preallocation arithmetic, recovery scanner bounds, and prune limits.
# 417 -> 418 / 428 -> 429: one profile-backed history journal create/reopen test in history_store.rs.
# 418 -> 420 / 429 -> 431: two LEDGER-1 occupancy episode ledger tests in coordinate_episode.rs.
# 420 -> 437 / 431 -> 448: seventeen tests across world.rs, camera.rs, render.rs,
# commit_r1.rs, octree_commit.rs, scale_assay.rs, and optimization.rs.
# 437 -> 646 / 448 -> 657: chem (planet6) and bitblock-wasm suites merged via PR #2.
# 646 -> 654 / 657 -> 665: eight audit regression tests across tick.rs, camera.rs,
# render.rs, display.rs, presentation.rs, coordinate_episode.rs, commit_r1.rs,
# and octree_commit.rs.
# 713 -> 719 / 724 -> 732: four COMMIT-R1 proof-policy/boundary/fork/truncation
# tests in commit_r1.rs and four CODEC-1@local-v1 dual-language integration tests
# in tests/codec_parity_v1.rs (2 default + 2 protocol-research).
# 719 -> 725 / 732 -> 739: three TRANSITION-SPEC-D1 unit tests in transition_spec.rs,
# three REFINE-1@world correspondence integration tests in tests/world_refinement_correspondence.rs,
# and one protocol-research integration test in tests/transition_spec_d1.rs.
# 725 -> 728 / 739 -> 747: three COMMIT-1 unit tests in commit_state.rs (including 19 golden
# Python cross-parity assertions) and five protocol-research integration tests in
# tests/commit_1_authenticated_state.rs (depth-192/256 boundaries, adversarial proof controls,
# logical pruning, GC write-barrier race, and 5-crash-point filesystem recovery gauntlet).
# 728 -> 731 / 747 -> 753: three default-feature REFINE-1@accounting correspondence tests in
# tests/accounting_refinement_correspondence.rs and three protocol-research REFINE-1@commitment
# correspondence tests in tests/commitment_refinement_correspondence.rs.
# 731 -> 733 / 753 -> 758: two unit tests in crates/research/src/verkle_feasibility.rs (run in
# workspace quick and full) plus three protocol-research integration tests in
# tests/verkle_r1_feasibility.rs (VERKLE-R1@feasibility).
# 733 -> 739 / 758 -> 764: six default-feature SCALE-2 integration tests in
# tests/scale_2_parity.rs (chunk-relative i64 extrema, negative chunk edges, Hi-Z culling +
# delta unpacking, sub-pixel LOD parity, and bounded SpatialRenderCache caps/invalidation).
# 764 -> 765 (full): one protocol-research DurableCommitmentStore pack-compaction GC & torn-tail
# recovery test in tests/commit_1_authenticated_state.rs.
# 739 -> 746 / 765 -> 774: seven Wave 1 unit tests across crypto_d1, identity_d1, asset_d1, and
# consensus_gov_d1 (run in workspace quick and full) plus two protocol-research integration tests
# in tests/pre_genesis_d1_contracts.rs.
# 746 -> 747 / 774 -> 777: one RULE-1 unit test in rule_1.rs plus two protocol-research
# integration tests in tests/rule_1_b5s45.rs.
# 747 -> 749 / 777 -> 781: two Wave 3 unit tests in profile_bootstrap.rs and language_r1.rs plus
# two protocol-research integration tests in tests/wave3_bootstrap_and_language_r1.rs.
# 749 -> 751 / 781 -> 785: two Wave 4 unit tests in threat_1.rs and vm_d1.rs plus two
# protocol-research integration tests in tests/wave4_threat_1_and_vm_d1.rs.
# 751 -> 752 / 785 -> 787: one Wave 5 unit test in bounds_1.rs plus one protocol-research
# integration test in tests/wave5_bounds_1.rs.
# 752 -> 753 / 787 -> 789: one Wave 6 unit test in emergence_r1.rs plus one protocol-research
# integration test in tests/wave6_emergence_r1.rs.
# 753 -> 754 / 789 -> 791: one Wave 7 unit test in resource_1.rs plus one protocol-research
# integration test in tests/wave7_resource_1.rs.
# 754 -> 755 / 791 -> 793: one Wave 8A unit test in genesis_d1.rs plus one protocol-research
# integration test in tests/wave8a_genesis_d1.rs.
# 755 -> 756 / 793 -> 795: one Wave 8C unit test in protocol_id_1.rs plus one protocol-research
# integration test in tests/wave8c_protocol_id_1.rs.
# 756 -> 757 / 795 -> 798: one Wave 9A unit test in vm_1.rs plus two protocol-research
# integration tests in tests/wave9a_vm_1.rs.
# 798 -> 801 (full): three protocol-research REFINE-1@vm correspondence tests in
# tests/vm_refinement_correspondence.rs.
# 757 -> 758 / 801 -> 804: one Wave 10 unit test in crypto_1.rs plus two protocol-research
# integration tests in tests/wave10_crypto_1.rs.
# 758 -> 759 / 804 -> 807: one Wave 11 unit test in profile_d1.rs plus two protocol-research
# integration tests in tests/wave11_profile_d1.rs.
# 759 -> 760 / 807 -> 810: one Wave 12A unit test in identity_1.rs plus two protocol-research
# integration tests in tests/wave12a_identity_1.rs.
# 760 -> 761 / 810 -> 813: one Wave 12C unit test in transition_spec_1.rs plus two protocol-research
# integration tests in tests/wave12c_transition_spec_1.rs.
# 761 -> 762 / 813 -> 816: one Wave 13A unit test in auth_commit_1.rs plus two protocol-research
# integration tests in tests/wave13_auth_commit_1.rs.
# 762 -> 763 / 816 -> 819: one Wave 14A unit test in asset_1.rs plus two protocol-research
# integration tests in tests/wave14a_asset_1.rs.
WINDOWS_ONLY_TESTS = {
    "quick": 29,  # 654 on Windows - 625 on Linux
    "full": 38,   # 665 on Windows - 627 on Linux
}
EXPECTED_TESTS = {
    t: (count if sys.platform == "win32" else count - WINDOWS_ONLY_TESTS[t])
    for t, count in {"quick": 763, "full": 819}.items()
}
RESULT_LINE = re.compile(r"^test result: ok\. (\d+) passed", re.M)
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


def run_counting(path: str, tally: list) -> tuple[bool, str]:
    """Run one test binary and record how many tests it actually reported."""
    result = subprocess.run([path, "--quiet"], cwd=ROOT, capture_output=True,
                            text=True, encoding="utf-8", errors="replace")
    output = result.stdout + result.stderr
    tally.extend(int(n) for n in RESULT_LINE.findall(output))
    if result.returncode == 0:
        return True, ""
    tail = output.strip().splitlines()[-12:]
    return False, "\n".join("      " + line for line in tail)


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
    command = ["cargo", "test", "--no-run", "-q", "--workspace",
               "--message-format=json", *features]
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
    tools = vector_suites()
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
                       "%d bare validators, %d of them independent vector reconstructions; "
                       "they catch the two sides disagreeing"
                       % (len(vector_suites()),
                          sum(p.name.endswith("_vectors.py") for p in vector_suites()))),
    "planning docs": (PLANNING_INPUTS,
                      lambda: run([sys.executable, "-B", "tools/check_planning_docs.py", "--self-test"]),
                      "powered planning-document and frozen-audit negative controls"),
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
    tally: list = []

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
        clippy_cmd = [*CLIPPY, *features, "--", "-D", "warnings"]
        clippy_future = clippy_pool.submit(lambda: run(clippy_cmd, CLIPPY_ENV))

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
                         (lambda p: (lambda: run_counting(p, tally)))(path),
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

    # The count gate. Only meaningful when the tests actually ran; a cached skip
    # says nothing about how many there are.
    if tally:
        total = sum(tally)
        floor = EXPECTED_TESTS.get(name, 0)
        ok = total >= floor
        rows.append(("tests executed", "ok" if ok else "FAIL", None,
                     "%d ran, %d expected" % (total, floor)))
        if not ok:
            failures.append(("tests executed",
                             "      only %d of an expected %d tests ran -- the suite "
                             "shrank rather than failed" % (total, floor)))

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
    flags = [a for a in argv[1:] if a.startswith("-")]
    if "-h" in flags or "--help" in flags:
        print(__doc__)
        return 0
    unknown = [f for f in flags if f != "--force"]
    if unknown:
        # Unknown flags used to be dropped silently, so `--help` ran the full suite.
        print("bbverify: unknown option(s): %s\n" % " ".join(unknown), file=sys.stderr)
        print(__doc__, file=sys.stderr)
        return 2
    args = [a for a in argv[1:] if not a.startswith("-")]
    force = "--force" in flags
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
