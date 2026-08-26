#!/usr/bin/env python3
"""Independently reconstruct Bitblock's declared sidebar/composite policy tags."""

from __future__ import annotations

import base64
import hashlib
import re
import struct
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
SIDEBAR_SCHEMA = b"bitblock/sidebar/raster-policy/v1"
COMPOSITE_SCHEMA = b"bitblock/sidebar/composite-policy/v1"
SIDEBAR_WIDTH = 400
COLORS = (0x00101318, 0x0038424D, 0x00F2C14E, 0x008DA2B8, 0x00E8EDF2)
FONT_POLICY = b"font=bitblock-3x5-ascii/v1;glyph-scale=1;row-gap=2;predecessor-only=v1"
RENDERER_POLICY = "tsxgj54fonmrrlkoygcn5dtv65iu6neymvyee3x6hartgevwyynq"
CROSSHAIR_POLICY = "pyj2osdlm4onqqyrhhlvy2qjfxxkfpfsg7flmjxeodoho55i4hta"


def be16(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def content_digest(kind: bytes, schema: bytes, payload: bytes) -> bytes:
    framed = (
        be16(TAG_DOMAIN)
        + be16(kind)
        + be16(HASH_SUITE)
        + be16(schema)
        + struct.pack(">Q", len(payload))
        + payload
    )
    return hashlib.sha256(framed).digest()


def text_tag(kind: str, digest: bytes) -> str:
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.{kind}.sha256.{encoded}"


def decode_digest(encoded: str) -> bytes:
    return base64.b32decode(encoded.upper() + "=" * ((8 - len(encoded) % 8) % 8))


def bindings() -> list[tuple[bytes, bytes]]:
    source = (ROOT / "src" / "sidebar.rs").read_text(encoding="utf-8")
    matches = re.findall(
        r'binding!\(\s*"([^"]+)",\s*"([^"]+)"\s*\)', source, re.MULTILINE
    )
    if not matches:
        raise SystemExit("no sidebar bindings found")
    registry = (ROOT / "docs" / "metrics" / "registry.tsv").read_text(encoding="utf-8")
    for alias, definition in matches:
        prefix = f"{alias}\t{definition}\t"
        if sum(line.startswith(prefix) for line in registry.splitlines()) != 1:
            raise SystemExit(f"non-authoritative binding: {alias}")
    return [(alias.encode("ascii"), definition.encode("ascii")) for alias, definition in matches]


def main() -> None:
    payload = struct.pack(">I", SIDEBAR_WIDTH)
    payload += b"".join(struct.pack(">I", color) for color in COLORS)
    payload += FONT_POLICY
    for alias, definition in bindings():
        payload += be16(alias) + be16(definition)
    sidebar = content_digest(b"policy", SIDEBAR_SCHEMA, payload)
    composite_payload = (
        decode_digest(RENDERER_POLICY) + decode_digest(CROSSHAIR_POLICY) + sidebar
    )
    composite = content_digest(b"policy", COMPOSITE_SCHEMA, composite_payload)
    print(text_tag("policy", sidebar))
    print(text_tag("policy", composite))


if __name__ == "__main__":
    main()
