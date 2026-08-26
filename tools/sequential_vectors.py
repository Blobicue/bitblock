#!/usr/bin/env python3
"""Independent SHA-256 vectors for Bitblock's local sequential-history protocol."""

from __future__ import annotations

import base64
import hashlib
import struct


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
FUNCTION_SCHEMA = b"bitblock/sequential-history/function/sha256-chain-v1"
INPUT_SCHEMA = b"bitblock/sequential-history/input/v1"
STEP_SCHEMA = b"bitblock/sequential-history/step/v1"
SEGMENT_SCHEMA = b"bitblock/sequential-history/segment/v1"
EVIDENCE_SCHEMA = b"bitblock/sequential-history/coordinate-evidence/v1"
OPEN_LOCAL_EVIDENCE_SCHEMA = b"bitblock/history-mode/open-local-evidence/v1"


def be16(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def content_digest(kind: bytes, schema: bytes, payload: bytes) -> bytes:
    preimage = (
        be16(TAG_DOMAIN)
        + be16(kind)
        + be16(HASH_SUITE)
        + be16(schema)
        + struct.pack(">Q", len(payload))
        + payload
    )
    return hashlib.sha256(preimage).digest()


def text_tag(kind: str, digest: bytes) -> str:
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.{kind}.sha256.{encoded}"


def step(
    function: bytes,
    absolute_iteration: int,
    previous: bytes,
    events: tuple[bytes, ...],
) -> bytes:
    payload = (
        function
        + struct.pack(">Q", absolute_iteration)
        + previous
        + struct.pack(">Q", len(events))
        + b"".join(events)
    )
    return content_digest(b"sequential-history", STEP_SCHEMA, payload)


def run_segment(
    function: bytes,
    start: int,
    count: int,
    initial: bytes,
    injections: tuple[tuple[int, bytes], ...],
) -> tuple[bytes, tuple[bytes, ...]]:
    state = initial
    states: list[bytes] = []
    cursor = 0
    for relative in range(count):
        first = cursor
        while cursor < len(injections) and injections[cursor][0] == relative:
            cursor += 1
        events = tuple(event for _, event in injections[first:cursor])
        state = step(function, start + relative, state, events)
        states.append(state)
    if cursor != len(injections):
        raise ValueError("an injection offset is outside the segment")
    return state, tuple(states)


def segment_payload(
    function: bytes,
    parent: bytes | None,
    start: int,
    count: int,
    initial: bytes,
    injections: tuple[tuple[int, bytes], ...],
    output: bytes,
) -> bytes:
    parent_field = b"\x00" + bytes(32) if parent is None else b"\x01" + parent
    return (
        function
        + parent_field
        + struct.pack(">QQ", start, count)
        + initial
        + struct.pack(">Q", len(injections))
        + b"".join(struct.pack(">Q", offset) + event for offset, event in injections)
        + output
    )


def coordinate_evidence(
    mode: bytes,
    history_genesis: bytes,
    history_head: bytes,
    segment_head: bytes,
    output: bytes,
    total_iterations: int,
    event_count: int,
) -> bytes:
    payload = (
        mode
        + history_genesis
        + history_head
        + segment_head
        + output
        + struct.pack(">QQ", total_iterations, event_count)
    )
    assert len(payload) == 176
    return content_digest(b"sequential-history", EVIDENCE_SCHEMA, payload)


def open_local_evidence(
    mode: bytes,
    history_genesis: bytes,
    history_head: bytes,
    final_world: bytes,
) -> bytes:
    payload = mode + history_genesis + history_head + final_world
    assert len(payload) == 128
    return content_digest(b"receipt", OPEN_LOCAL_EVIDENCE_SCHEMA, payload)


def main() -> None:
    function = content_digest(b"policy", FUNCTION_SCHEMA, b"")
    source = bytes(range(0x00, 0x20))
    initial = content_digest(b"sequential-history", INPUT_SCHEMA, be16(b"receipt") + source)
    events = tuple(bytes(range(start, start + 0x20)) for start in (0x20, 0x40, 0x60, 0x80))

    injections_a = ((0, events[0]), (0, events[1]), (3, events[2]))
    output_a, states_a = run_segment(function, 0, 4, initial, injections_a)
    payload_a = segment_payload(function, None, 0, 4, initial, injections_a, output_a)
    tag_a = content_digest(b"sequential-history", SEGMENT_SCHEMA, payload_a)

    injections_b = ((1, events[3]),)
    output_b, states_b = run_segment(function, 4, 2, output_a, injections_b)
    payload_b = segment_payload(function, tag_a, 4, 2, output_a, injections_b, output_b)
    tag_b = content_digest(b"sequential-history", SEGMENT_SCHEMA, payload_b)

    injections_joined = ((0, events[0]), (0, events[1]), (3, events[2]), (5, events[3]))
    output_joined, _ = run_segment(function, 0, 6, initial, injections_joined)
    payload_joined = segment_payload(
        function, None, 0, 6, initial, injections_joined, output_joined
    )
    tag_joined = content_digest(b"sequential-history", SEGMENT_SCHEMA, payload_joined)
    evidence = coordinate_evidence(
        bytes(range(0x00, 0x20)),
        bytes(range(0x20, 0x40)),
        bytes(range(0x40, 0x60)),
        bytes(range(0x60, 0x80)),
        bytes(range(0x80, 0xA0)),
        6,
        4,
    )
    open_evidence = open_local_evidence(
        bytes(range(0x00, 0x20)),
        bytes(range(0x20, 0x40)),
        bytes(range(0x40, 0x60)),
        bytes(range(0x60, 0x80)),
    )

    assert function.hex() == "dba40edbdd77458572cfb1664e7834ab9d4e7f85c3f2ba5bbd21d7a54f2142f0"
    assert initial.hex() == "67f3794d9202b4d670973d981c3816c7777f88a1b3b0c691b446fddac5e3a3a0"
    assert [state.hex() for state in states_a] == [
        "3687283f436d425bd82234eadfc1ba288bcde511f3e17a186ae060aab8dfbdc5",
        "6c5f21bdde4a84c2ef949f8e6d637990823e2cc9de2549354ce3cab20af4f031",
        "8fc88d67db62dceace956d3f6c717943b4bd0452abe8dfb922fa567cb8889f58",
        "05ae4e8ef013fab927d04dff53285a8dc91e7607fe7b52079c508d96321015c5",
    ]
    assert [state.hex() for state in states_b] == [
        "36be1a5d5775c05c7b3285f70b538058b0a65935b14ed32b6ee966c84b780406",
        "14434c2973e199790a467b75c1abbcbf6d93a54244c540fbe247543443e652d9",
    ]
    assert len(payload_a) == 273
    assert tag_a.hex() == "65a11acf802c61ea3fd09b08b5d594d767841a33413b842f3a5085473ebacb96"
    assert len(payload_b) == 193
    assert tag_b.hex() == "81f76fa460f9999e29c3f6a85ca1ac5ec0db2a96d98527a4e019fca540a95097"
    assert output_joined == output_b
    assert len(payload_joined) == 313
    assert tag_joined.hex() == "1a5f7b91b68ab89d9da14b4accb7e428e4386afa0545f5c6523a07449d518a12"
    assert evidence.hex() == "f12be205157bd0371862856852e144d5567cf4d60442ef60c28dcc026a489fa2"
    assert open_evidence.hex() == "df7f4e9c3aa676520d67e697fa70ecf32b8508d6083d08832c0b25bfe4529f09"

    print("function", text_tag("policy", function))
    print("input", text_tag("sequential-history", initial))
    for index, state in enumerate((*states_a, *states_b)):
        print(f"state[{index}]", state.hex())
    print("segment-a", len(payload_a), text_tag("sequential-history", tag_a))
    print("segment-b", len(payload_b), text_tag("sequential-history", tag_b))
    print("segment-joined", len(payload_joined), text_tag("sequential-history", tag_joined))
    print("coordinate-evidence", text_tag("sequential-history", evidence))
    print("open-local-evidence", text_tag("receipt", open_evidence))


if __name__ == "__main__":
    main()
