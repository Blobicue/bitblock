#!/usr/bin/env python3
"""Read-only generator/auditor for Bitblock's content-tagged metric registry.

The tool prints either an apply_patch migration or a canonical TSV manifest. It
never writes files itself, so repository mutations still pass through the
workspace patch boundary.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import re
import struct
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
METRIC_FILES = (
    ROOT / "docs" / "metrics" / "world.md",
    ROOT / "docs" / "metrics" / "view.md",
    ROOT / "docs" / "metrics" / "system.md",
)
DOCUMENT_FILES = (
    ROOT / "README.md",
    ROOT / "METRICS.md",
    ROOT / "docs" / "PROTOCOL.md",
    ROOT / "docs" / "METRIC_EXHAUSTION_AUDIT.md",
    *METRIC_FILES,
)
LEGACY = re.compile(r"[A-Z][A-Z0-9]*-[0-9]{3}")
LEGACY_WILDCARD = re.compile(r"\b([A-Z][A-Z0-9]*)-\*")
ALIAS = re.compile(r"#bb\.metric\.([a-z0-9-]+)\.([a-z0-9-]+)")
REFERENCE = re.compile(r"#bb\.metric\.[a-z0-9-]+\.[a-z0-9-]+")
LIST_START = re.compile(
    r"^- \*\*(?P<key>[A-Z][A-Z0-9]*-[0-9]{3}|#bb\.metric\.[a-z0-9-]+\.[a-z0-9-]+) — (?P<name>.+?)\*\*"
)
TABLE_START = re.compile(
    r"^\|\s*`?(?P<key>[A-Z][A-Z0-9]*-[0-9]{3}|#bb\.metric\.[a-z0-9-]+\.[a-z0-9-]+)`?\s*\|(?P<rest>.*)$"
)
TAG_DOMAIN = b"bitblock/content-tag/v1"
TAG_KIND = b"metric-definition"
HASH_SUITE = b"sha256"
METRIC_SCHEMA = b"bitblock/metric-definition/markdown-v1"


@dataclass(frozen=True)
class Record:
    key: str
    alias: str
    name: str
    owner: str
    definition: str


def slug(text: str) -> str:
    text = text.replace("`", "").replace("*", "")
    value = re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")
    if not value:
        raise ValueError(f"metric name has no ASCII slug: {text!r}")
    return value


def table_name(rest: str) -> str:
    first_cell = rest.split("|", 1)[0].strip()
    if first_cell.startswith("**"):
        end = first_cell.find("**", 2)
        if end < 0:
            raise ValueError(f"unclosed bold metric name: {first_cell!r}")
        return first_cell[2:end].strip().rstrip(":.")
    return first_cell.strip("`* ").rstrip(":.")


def canonical_list_block(block: list[str], key: str) -> str:
    first = block[0].replace(key + " — ", "", 1)
    return "\n".join([first, *block[1:]]).rstrip()


def canonical_table_row(line: str, key: str) -> str:
    match = TABLE_START.match(line)
    if not match or match.group("key") != key:
        raise ValueError(f"cannot strip table key {key!r}: {line!r}")
    return ("|" + match.group("rest")).rstrip()


def parse_records(path: Path) -> list[Record]:
    lines = path.read_text(encoding="utf-8").replace("\r\n", "\n").split("\n")
    owner = path.stem
    records: list[Record] = []
    index = 0
    while index < len(lines):
        line = lines[index]
        list_match = LIST_START.match(line)
        table_match = TABLE_START.match(line)
        if list_match:
            key = list_match.group("key")
            name = list_match.group("name").strip().rstrip(".")
            end = index + 1
            while end < len(lines):
                if LIST_START.match(lines[end]) or lines[end].startswith("## "):
                    break
                end += 1
            definition = canonical_list_block(lines[index:end], key)
            index = end
        elif table_match:
            key = table_match.group("key")
            name = table_name(table_match.group("rest"))
            definition = canonical_table_row(line, key)
            index += 1
        else:
            index += 1
            continue

        if LEGACY.fullmatch(key):
            family = key.split("-", 1)[0].lower()
            alias = f"#bb.metric.{family}.{slug(name)}"
        else:
            alias = key
        records.append(Record(key, alias, name, owner, definition))
    return records


def all_records() -> list[Record]:
    records = [record for path in METRIC_FILES for record in parse_records(path)]
    aliases: dict[str, Record] = {}
    keys: dict[str, Record] = {}
    for record in records:
        if record.alias in aliases:
            other = aliases[record.alias]
            raise ValueError(
                f"duplicate alias {record.alias}: {other.key}/{other.name} and {record.key}/{record.name}"
            )
        if record.key in keys:
            raise ValueError(f"duplicate definition key {record.key}")
        aliases[record.alias] = record
        keys[record.key] = record
    return records


def be16(data: bytes) -> bytes:
    if len(data) > 0xFFFF:
        raise ValueError("u16-framed field is too long")
    return struct.pack(">H", len(data)) + data


def tag_record(record: Record) -> str:
    owner = record.owner.encode("ascii")
    definition = record.definition.encode("utf-8")
    payload = be16(owner) + struct.pack(">Q", len(definition)) + definition
    framed = (
        be16(TAG_DOMAIN)
        + be16(TAG_KIND)
        + be16(HASH_SUITE)
        + be16(METRIC_SCHEMA)
        + struct.pack(">Q", len(payload))
        + payload
    )
    digest = hashlib.sha256(framed).digest()
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.metric-definition.sha256.{encoded}"


def emit_plan(records: list[Record]) -> None:
    legacy = sum(bool(LEGACY.fullmatch(record.key)) for record in records)
    named = len(records) - legacy
    print(f"records={len(records)} legacy={legacy} named={named}")
    print(f"unique_aliases={len({record.alias for record in records})}")
    tags = [tag_record(record) for record in records]
    print(f"unique_definition_tags={len(set(tags))}")


def emit_patch(records: list[Record]) -> None:
    mapping = {
        record.key: record.alias for record in records if LEGACY.fullmatch(record.key)
    }
    if not mapping:
        raise ValueError("catalog has no numeric handles to migrate")

    print("*** Begin Patch")
    for path in DOCUMENT_FILES:
        original = path.read_text(encoding="utf-8").replace("\r\n", "\n")
        changed_lines: list[tuple[str, str]] = []
        for line in original.split("\n"):
            changed = LEGACY.sub(
                lambda match: mapping.get(match.group(0), match.group(0)), line
            )
            changed = LEGACY_WILDCARD.sub(
                lambda match: f"#bb.metric.{match.group(1).lower()}.*", changed
            )
            if changed != line:
                changed_lines.append((line, changed))
        if not changed_lines:
            continue
        print(f"*** Update File: {path}")
        for before, after in changed_lines:
            print("@@")
            print("-" + before)
            print("+" + after)
    print("*** End Patch")


def manifest(records: list[Record]) -> str:
    if any(LEGACY.fullmatch(record.key) for record in records):
        raise ValueError("migrate numeric handles before generating authoritative manifest")
    lines = [
        "# Bitblock metric definition registry v1",
        "# registry_name\tdefinition_tag\towner\tmetric_name",
    ]
    for record in records:
        lines.append(
            "\t".join((record.alias, tag_record(record), record.owner, record.name))
        )
    return "\n".join(lines) + "\n"


def audit(records: list[Record]) -> None:
    emit_plan(records)
    owner_counts: dict[str, int] = {}
    for record in records:
        owner_counts[record.owner] = owner_counts.get(record.owner, 0) + 1
    print(
        "owner_counts="
        + ",".join(f"{owner}:{owner_counts[owner]}" for owner in sorted(owner_counts))
    )

    defined = {record.alias for record in records}
    unresolved: list[str] = []
    for path in DOCUMENT_FILES:
        for line_number, line in enumerate(
            path.read_text(encoding="utf-8").splitlines(), start=1
        ):
            for reference in REFERENCE.findall(line):
                if reference not in defined:
                    unresolved.append(
                        f"{path.relative_to(ROOT)}:{line_number}:{reference}"
                    )
    print(f"unresolved_references={len(unresolved)}")
    if unresolved:
        print("\n".join(unresolved[:20]))
        raise SystemExit(1)

    malformed_table_rows: list[str] = []
    for path in METRIC_FILES:
        for line_number, line in enumerate(
            path.read_text(encoding="utf-8").splitlines(), start=1
        ):
            if TABLE_START.match(line):
                unescaped_pipes = len(re.findall(r"(?<!\\)\|", line))
                if unescaped_pipes != 7:
                    malformed_table_rows.append(
                        f"{path.relative_to(ROOT)}:{line_number}:pipes={unescaped_pipes}"
                    )
    print(f"malformed_metric_table_rows={len(malformed_table_rows)}")
    if malformed_table_rows:
        print("\n".join(malformed_table_rows[:20]))
        raise SystemExit(1)

    families = sorted({record.alias.split(".")[2].upper() for record in records})
    metric_handle = re.compile(
        rf"\b(?:{'|'.join(re.escape(family) for family in families)})-[0-9]{{3}}\b"
    )
    legacy_mentions: list[str] = []
    for path in DOCUMENT_FILES:
        text = path.read_text(encoding="utf-8")
        for match in metric_handle.finditer(text):
            legacy_mentions.append(f"{path.relative_to(ROOT)}:{match.group(0)}")
    print(f"legacy_mentions={len(legacy_mentions)}")
    if legacy_mentions:
        print("\n".join(legacy_mentions[:20]))
        raise SystemExit(1)

    manifest_path = ROOT / "docs" / "metrics" / "registry.tsv"
    if manifest_path.exists():
        expected = manifest(records)
        actual = manifest_path.read_text(encoding="utf-8").replace("\r\n", "\n")
        print(f"manifest_exact={actual == expected}")
        if actual != expected:
            raise SystemExit(1)
    else:
        print("manifest_exact=N/A (missing)")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=("plan", "patch", "manifest", "audit"))
    args = parser.parse_args()
    try:
        records = all_records()
        if args.mode == "plan":
            emit_plan(records)
        elif args.mode == "patch":
            emit_patch(records)
        elif args.mode == "manifest":
            sys.stdout.write(manifest(records))
        else:
            audit(records)
    except (KeyError, ValueError) as error:
        print(f"metric-registry error: {error}", file=sys.stderr)
        raise SystemExit(2) from error


if __name__ == "__main__":
    main()
