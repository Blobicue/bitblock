#!/usr/bin/env python3
"""Run deterministic headless simulations for explicit action sequences.

The script accepts either:
  - a Bitblock interaction tape (``.tape`` text format), or
  - a JSON scenario that is converted into a tape.

The conversion is one way: JSON -> text tape -> ``bitblock --simulate-actions``.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path


def _error(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    sys.exit(1)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build and run a Bitblock interaction simulation from a scenario.",
    )
    parser.add_argument("scenario", help="Path to a .json scenario or .tape sequence")
    parser.add_argument(
        "--binary",
        default="cargo",
        help="Bitblock executable path, or `cargo` to run via cargo",
    )
    parser.add_argument(
        "--release",
        action="store_true",
        default=True,
        help="Use `cargo run --release` (only applies when --binary is cargo)",
    )
    parser.add_argument(
        "--expect",
        action="store_true",
        help="Fail if any expectations in the scenario are not met",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Print parsed final and per-tick state before optional expectation checks",
    )
    return parser.parse_args()


def normalize_action(action: object) -> str:
    if not isinstance(action, str):
        _error(f"action must be text, got {type(action).__name__}")
    action = action.strip()
    if not action:
        _error("empty action line is not allowed inside a tick")
    return action


def normalize_tick(tick: object, index: int) -> list[str]:
    if isinstance(tick, list):
        actions = tick
    elif isinstance(tick, dict):
        if "actions" in tick:
            actions = tick["actions"]
        elif "commands" in tick:
            actions = tick["commands"]
        else:
            _error(
                f"tick {index} has no actions/commands field "
                "(expected e.g. {{\"actions\": [\"move ...\"]}})"
            )
    else:
        _error(
            f"tick {index} must be a list or object, got {type(tick).__name__}"
        )
    if not isinstance(actions, list):
        _error(f"tick {index} actions must be a list, got {type(actions).__name__}")
    return [normalize_action(action) for action in actions]


def scenario_to_tape_text(document: object) -> str:
    if not isinstance(document, dict):
        _error("scenario JSON must be an object")
    ticks = document.get("ticks")
    if ticks is None:
        _error("scenario JSON must have a top-level `ticks` array")
    if not isinstance(ticks, list):
        _error("scenario `ticks` must be an array")
    lines = []
    for tick_index, tick in enumerate(ticks):
        lines.append("tick")
        lines.extend(normalize_tick(tick, tick_index))
    return "\n".join(lines)


def load_and_render(source_path: Path) -> tuple[str, dict]:
    extension = source_path.suffix.lower()
    if extension == ".tape":
        tape_text = source_path.read_text(encoding="utf-8")
        # Preserve JSON metadata as empty if a user only provided a .tape file.
        return tape_text, {}
    if extension != ".json":
        _error("scenario must be .json (structured) or .tape (legacy text)")
    raw = json.loads(source_path.read_text(encoding="utf-8"))
    return scenario_to_tape_text(raw), raw.get("expected", {})


def run_simulation(binary: str, tape_path: Path, release: bool) -> tuple[str, str, int]:
    if Path(binary).name.lower() == "cargo" or binary == "cargo":
        command = [
            binary,
            "run",
            "--release" if release else "",
            "--",
            "--simulate-actions",
            str(tape_path),
        ]
        # remove empty placeholder when --release not requested
        command = [token for token in command if token]
    else:
        command = [binary, "--simulate-actions", str(tape_path)]

    process = subprocess.run(
        command,
        text=True,
        capture_output=True,
        cwd=str(Path(__file__).resolve().parent.parent),
    )
    return process.stdout, process.stderr, process.returncode


def parse_summary_fields(raw: str) -> tuple[dict[str, object], list[dict[str, object]]]:
    final_fields: dict[str, object] = {}
    tick_summaries: list[dict[str, object]] = []

    for line in raw.splitlines():
        if line.startswith("interaction-final\t"):
            for field in line.split("\t")[1:]:
                if "=" in field:
                    key, value = field.split("=", 1)
                    final_fields[key] = value
        elif line.startswith("interaction-tick\t"):
            fields: dict[str, object] = {}
            for field in line.split("\t")[1:]:
                if "=" in field:
                    key, value = field.split("=", 1)
                    fields[key] = value
            tick_summaries.append(fields)

    # Convert known numeric fields from strings to ints for direct comparison.
    for tick in tick_summaries:
        if "ordinal" in tick and isinstance(tick["ordinal"], str) and tick["ordinal"].isdigit():
            tick["ordinal"] = int(tick["ordinal"])
    if "on_bits" in final_fields and str(final_fields["on_bits"]).isdigit():
        final_fields["on_bits"] = int(final_fields["on_bits"])
    if "coordinate_receipts" in final_fields and str(final_fields["coordinate_receipts"]).isdigit():
        final_fields["coordinate_receipts"] = int(final_fields["coordinate_receipts"])

    return final_fields, tick_summaries


def parse_expected(document: dict) -> tuple[dict, list[dict]]:
    expected = document.get("expected", {})
    if not isinstance(expected, dict):
        return {}, []
    expected_ticks = expected.get("ticks", [])
    if not isinstance(expected_ticks, list):
        return expected, []
    return expected.get("final", expected), expected_ticks


def fail_if_mismatch(path: Path, actual: object, expected: object, label: str) -> list[str]:
    failures: list[str] = []
    if isinstance(expected, dict) and isinstance(actual, dict):
        for key, value in expected.items():
            if key not in actual:
                failures.append(f"{path.name} expected key missing: {label}.{key}")
                continue
            if actual[key] != value:
                failures.append(
                    f"{path.name} mismatch {label}.{key}: expected {value!r}, got {actual[key]!r}"
                )
    elif expected is not None and actual != expected:
        failures.append(
            f"{path.name} mismatch {label}: expected {expected!r}, got {actual!r}"
        )
    return failures


def compare_expectations(document: dict, final_fields: dict, tick_summaries: list[dict]) -> int:
    expected_final, expected_ticks = parse_expected(document)
    failures: list[str] = []
    scenario_name = document.get("name") or Path(document.get("_source_path", "scenario")).stem

    failures.extend(
        fail_if_mismatch(
            Path(f"{scenario_name}.json"), final_fields, expected_final, "final"
        )
    )

    for expected in expected_ticks:
        if not isinstance(expected, dict):
            failures.append(
                "expected_ticks entry must be object with at least an ordinal field"
            )
            continue
        ordinal = expected.get("ordinal")
        if ordinal is None:
            failures.append("expected_ticks entry missing `ordinal`")
            continue
        try:
            ordinal = int(ordinal)
        except (TypeError, ValueError):
            failures.append(f"invalid expected_ticks.ordinal {ordinal!r}")
            continue
        matches = [tick for tick in tick_summaries if tick.get("ordinal") == ordinal]
        if not matches:
            failures.append(f"no observed tick summary for ordinal {ordinal}")
            continue
        if len(matches) > 1:
            failures.append(f"multiple observed tick summaries for ordinal {ordinal}")
            continue
        failures.extend(
            fail_if_mismatch(
                Path(f"{scenario_name}.json"),
                matches[0],
                {k: v for k, v in expected.items() if k != "ordinal"},
                f"tick[{ordinal}]",
            )
        )

    if failures:
        print("simulation expectation check failed:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    return 0


def show_expectation_report(final_fields: dict, tick_summaries: list[dict]) -> None:
    print("final:")
    for key in sorted(final_fields):
        print(f"  {key}: {final_fields[key]}")
    print("ticks:")
    for tick in tick_summaries:
        ordinal = tick.get("ordinal", "?")
        print(f"  - ordinal {ordinal}:")
        for key in sorted(tick):
            if key == "ordinal":
                continue
            print(f"      {key}: {tick[key]}")


def main() -> int:
    args = parse_args()
    source_path = Path(args.scenario).expanduser().resolve()
    if not source_path.exists():
        _error(f"scenario file not found: {source_path}")

    # For .json scenario files, keep the parsed document around for expectation checks.
    with source_path.open("r", encoding="utf-8") as source:
        raw_document: dict = {}
        if source_path.suffix.lower() == ".json":
            raw_document = json.load(source)
            raw_document["_source_path"] = str(source_path)

    tape_text, expected = load_and_render(source_path)
    if source_path.suffix.lower() != ".tape" and "expected" not in raw_document:
        expected = {}

    with tempfile.NamedTemporaryFile("w", suffix=".tape", delete=False, encoding="utf-8") as tmp:
        tmp.write(tape_text)
        tmp_path = Path(tmp.name)

    try:
        stdout, stderr, status = run_simulation(args.binary, tmp_path, args.release)
    finally:
        try:
            os.unlink(tmp_path)
        except OSError:
            pass

    print(stdout, end="")
    if stderr:
        print(stderr, file=sys.stderr)
    if status != 0:
        return status

    final_fields, tick_summaries = parse_summary_fields(stdout)
    if args.verbose or expected:
        show_expectation_report(final_fields, tick_summaries)

    if args.expect and expected:
        return compare_expectations(raw_document, final_fields, tick_summaries)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
