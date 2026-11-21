#!/usr/bin/env python3
"""Independent vectors for one coordinate's ordered history semantics.

This is a stdlib-only oracle.  It imports no Bitblock code and does not launch
the Rust program: it reconstructs the current world, genesis, event, and
receipt encodings directly.  Its scope is intentionally finite: one fixed
coordinate per receipt, beginning from its genesis parent.

Passing this tool demonstrates agreement of these bounded known-answer cases.
It does not demonstrate a general proof, LadderLang execution, iota execution
cost, proof of history, proof of stake, ownership, scarcity, consensus, or a
Layer 1.
"""

from __future__ import annotations

import base64
import hashlib
import struct
from dataclasses import dataclass
from typing import Iterable, Sequence


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
WORLD_SCHEMA = b"bitblock/world/chunk-merkle-sha256/v2"
WORLD_CHUNK_SCHEMA = b"bitblock/world/chunk-bits-i64-be/v2"
GENESIS_SCHEMA = b"bitblock/history/genesis/v1"
EVENT_SCHEMA = b"bitblock/history/event/v1"
RECEIPT_SCHEMA = b"bitblock/history/receipt/v1"
BEDROCK = (0, 0, 0)
P = (1, -2, 3)
EDGE = (-(1 << 63), 0, (1 << 63) - 1)

Coord = tuple[int, int, int]


@dataclass(frozen=True)
class Tag:
    """A semantic kind plus the complete SHA-256 digest it names."""

    kind: str
    digest: bytes

    def text(self) -> str:
        encoded = base64.b32encode(self.digest).decode("ascii").lower().rstrip("=")
        return f"#bb.{self.kind}.sha256.{encoded}"


@dataclass(frozen=True)
class Effect:
    """One derived state transition, retaining the occurrence event tag."""

    kind: str
    event: Tag
    consumed_mint: Tag | None = None


@dataclass(frozen=True)
class ScalarRun:
    """Complete result of one bounded, same-coordinate history receipt."""

    effects: tuple[Effect, ...]
    final_on: bool
    final_mint: Tag | None
    changes: int
    post_world: Tag
    receipt: Tag | None


def field(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def tag(kind: str, schema: bytes, payload: bytes) -> Tag:
    """Rebuild Bitblock's complete content-tag preimage independently."""

    preimage = (
        field(TAG_DOMAIN)
        + field(kind.encode("ascii"))
        + field(HASH_SUITE)
        + field(schema)
        + struct.pack(">Q", len(payload))
        + payload
    )
    return Tag(kind, hashlib.sha256(preimage).digest())


def checked_i64(value: int) -> int:
    if not -(1 << 63) <= value < (1 << 63):
        raise ValueError(f"coordinate is outside i64: {value}")
    return value


def checked_u64(value: int, name: str) -> int:
    if not 0 <= value < (1 << 64):
        raise ValueError(f"{name} is outside u64: {value}")
    return value


def encode_coord(coord: Coord) -> bytes:
    if len(coord) != 3:
        raise ValueError("coordinate must have exactly three components")
    return b"".join(struct.pack(">q", checked_i64(component)) for component in coord)


def operation_code(operation: str) -> int:
    codes = {"on": 0, "off": 1, "toggle": 2}
    try:
        return codes[operation]
    except KeyError as error:
        raise ValueError(f"undefined operation: {operation}") from error


def encode_edit(operation: str, coord: Coord) -> bytes:
    return bytes((operation_code(operation),)) + encode_coord(coord)


def _chunk_merkle_payload(coords, encode_digest):
    """v2 world fingerprint root payload: count of non-empty 16^3 chunks, then per chunk in
    ascending (cx, cy, cz) order its i64-be coordinate and the SHA-256 leaf digest over the same
    coordinate plus 64 u64-be occupancy words (bit index x + 16 (y + 16 z))."""
    chunks = {}
    for x, y, z in set(coords):
        key = (x // 16, y // 16, z // 16)
        index = x % 16 + 16 * (y % 16 + 16 * (z % 16))
        words = chunks.setdefault(key, [0] * 64)
        words[index // 64] |= 1 << (index % 64)
    payload = struct.pack(">Q", len(chunks))
    for key in sorted(chunks):
        encoded_key = struct.pack(">qqq", *key)
        leaf = encoded_key + b"".join(struct.pack(">Q", w) for w in chunks[key])
        payload += encoded_key + encode_digest(leaf)
    return payload


def world_tag(coords: Iterable[Coord]) -> Tag:
    coords = list(coords)
    for coord in coords:
        encode_coord(coord)
    payload = _chunk_merkle_payload(
        coords, lambda leaf: tag("world", WORLD_CHUNK_SCHEMA, leaf).digest
    )
    return tag("world", WORLD_SCHEMA, payload)


def genesis_tag(genesis_world: Tag) -> Tag:
    if genesis_world.kind != "world":
        raise ValueError("genesis requires a world tag")
    return tag("receipt", GENESIS_SCHEMA, genesis_world.digest)


def event_tag(parent: Tag, tick: int, operation: str, coord: Coord) -> Tag:
    payload = parent.digest + struct.pack(">Q", checked_u64(tick, "tick"))
    return tag("event", EVENT_SCHEMA, payload + encode_edit(operation, coord))


def receipt_tag(
    parent: Tag,
    tick: int,
    edits: Sequence[bytes],
    post_world: Tag,
    final_event: Tag,
) -> Tag:
    if parent.kind != "receipt":
        raise ValueError("receipt parent must be a receipt tag")
    if not edits:
        raise ValueError("receipt cannot contain zero edits")
    if post_world.kind != "world" or final_event.kind != "event":
        raise ValueError("receipt fields have an invalid semantic kind")
    payload = (
        parent.digest
        + struct.pack(">Q", checked_u64(tick, "tick"))
        + struct.pack(">Q", len(edits))
        + b"".join(edits)
        + post_world.digest
        + final_event.digest
        + b"\x00\x00"
    )
    return tag("receipt", RECEIPT_SCHEMA, payload)


def run_scalar(
    coord: Coord,
    initial_on: bool,
    initial_mint: Tag | None,
    history_parent: Tag,
    tick: int,
    operations: Sequence[str],
) -> ScalarRun:
    """Derive the live mint/burn/no-op semantics for one coordinate.

    `initial_mint` may be a genesis receipt for a mutable initially-on bit or
    an event for a continuation.  Bedrock is always on and never has mutable
    mint provenance.  Every accepted instruction gets an event, including a
    no-op; that event becomes the parent of the next instruction.
    """

    encode_coord(coord)
    checked_u64(tick, "tick")
    if history_parent.kind != "receipt":
        raise ValueError("history parent must be a receipt tag")
    if coord == BEDROCK:
        if not initial_on or initial_mint is not None:
            raise ValueError("bedrock must begin on without mint provenance")
    elif initial_on and initial_mint is None:
        raise ValueError("mutable occupied coordinate lacks mint provenance")
    elif not initial_on and initial_mint is not None:
        raise ValueError("off coordinate cannot carry mint provenance")
    elif initial_mint is not None and initial_mint.kind not in {"receipt", "event"}:
        raise ValueError("mutable mint provenance must be a receipt or event tag")

    on = initial_on
    active_mint = initial_mint
    event_head = history_parent
    effects: list[Effect] = []
    edits: list[bytes] = []
    changes = 0

    for operation in operations:
        encoded = encode_edit(operation, coord)
        event = event_tag(event_head, tick, operation, coord)
        edits.append(encoded)

        if coord == BEDROCK:
            effect = Effect("bedrock-protected", event)
        elif operation == "on":
            if on:
                effect = Effect("already-on", event)
            else:
                on = True
                active_mint = event
                changes += 1
                effect = Effect("mint", event)
        elif operation == "off":
            if not on:
                effect = Effect("already-off", event)
            else:
                if active_mint is None:
                    raise ValueError("on mutable coordinate lacks active mint")
                on = False
                consumed_mint = active_mint
                active_mint = None
                changes += 1
                effect = Effect("burn", event, consumed_mint)
        elif operation == "toggle":
            if on:
                if active_mint is None:
                    raise ValueError("on mutable coordinate lacks active mint")
                on = False
                consumed_mint = active_mint
                active_mint = None
                changes += 1
                effect = Effect("burn", event, consumed_mint)
            else:
                on = True
                active_mint = event
                changes += 1
                effect = Effect("mint", event)
        else:
            raise AssertionError("operation_code must have rejected this operation")

        effects.append(effect)
        event_head = event

    final_coords = {BEDROCK}
    if coord != BEDROCK and on:
        final_coords.add(coord)
    post_world = world_tag(final_coords)
    receipt = (
        None
        if not edits
        else receipt_tag(history_parent, tick, edits, post_world, event_head)
    )
    return ScalarRun(
        effects=tuple(effects),
        final_on=on,
        final_mint=active_mint,
        changes=changes,
        post_world=post_world,
        receipt=receipt,
    )


def run_scalar_edits(
    coord: Coord,
    initial_on: bool,
    initial_mint: Tag | None,
    history_parent: Tag,
    tick: int,
    edits: Sequence[tuple[str, Coord]],
) -> ScalarRun:
    """Boundary check for callers holding full edits instead of operation names."""

    for index, (_, edit_coord) in enumerate(edits):
        if edit_coord != coord:
            raise ValueError(f"edit {index} is outside the scalar coordinate")
    return run_scalar(
        coord,
        initial_on,
        initial_mint,
        history_parent,
        tick,
        [operation for operation, _ in edits],
    )


def require_rejected(label: str, thunk: object) -> None:
    try:
        thunk()  # type: ignore[operator]
    except ValueError:
        return
    raise AssertionError(f"negative control accepted: {label}")


def require_tag(actual: Tag | None, expected: str, label: str) -> None:
    if actual is None or actual.text() != expected:
        got = "none" if actual is None else actual.text()
        raise AssertionError(f"{label}: expected {expected}, got {got}")


def main() -> None:
    initial_off_world = world_tag([BEDROCK])
    initial_off_parent = genesis_tag(initial_off_world)
    initial_on_world = world_tag([BEDROCK, P])
    initial_on_parent = genesis_tag(initial_on_world)

    cases = {
        "empty": run_scalar(P, False, None, initial_off_parent, 19, []),
        "cold-cycle": run_scalar(
            P,
            False,
            None,
            initial_off_parent,
            4,
            ["on", "on", "off", "off", "toggle"],
        ),
        "same-tick-reuse": run_scalar(
            P,
            False,
            None,
            initial_off_parent,
            7,
            ["on", "off", "on", "off"],
        ),
        "warm-burn": run_scalar(P, True, initial_on_parent, initial_on_parent, 9, ["off"]),
        "noop-head": run_scalar(P, False, None, initial_off_parent, 11, ["off", "on"]),
        "toggle-pair": run_scalar(P, False, None, initial_off_parent, 13, ["toggle", "toggle"]),
        "bedrock": run_scalar(
            BEDROCK,
            True,
            None,
            initial_off_parent,
            17,
            ["on", "off", "toggle"],
        ),
        "edge": run_scalar(EDGE, False, None, initial_off_parent, 23, ["on"]),
    }

    expected_receipts = {
        "cold-cycle": "#bb.receipt.sha256.d7qzlret5ep47ewjymdfylx3lelasfvp5s6tmt72qjjh34bqwmea",
        "same-tick-reuse": "#bb.receipt.sha256.iozg4ylucv37oyjoceamtnvaqwdavpfk366dlo44kividto2q32a",
        "warm-burn": "#bb.receipt.sha256.6bkjjhs2qprx3dfgkuamon5dei5v6gqcnqk6k7ugsjxnx4nc4l4a",
        "noop-head": "#bb.receipt.sha256.rmfiy36x6e4kisgi5n6qemrpa3o6lpnbtrfqmmkohzlrixezmbaq",
        "toggle-pair": "#bb.receipt.sha256.rong4dap7mwzs2zztqioqo56ix2rmnjheutosombipvzgnt7dgiq",
        "bedrock": "#bb.receipt.sha256.exlpidihytljgbcr2vbgazsmifshk52eveiet5bfa32fzv6hfq6a",
        "edge": "#bb.receipt.sha256.suix4q6f7pj7qakempurczmewpjzq6hruzblpcueyezmiqwephha",
    }
    expected_events = {
        ("cold-cycle", 0): "#bb.event.sha256.7juld2z7wmvaol3xbf2kdlniwzy4tcbr7gfy5yojwaofqkmgws3a",
        ("cold-cycle", 4): "#bb.event.sha256.qk4stjl5xesq72glgtz7thj4lk4s4zsbb6fp7oljjsp45xfylmtq",
        ("same-tick-reuse", 0): "#bb.event.sha256.qhwms7spp3gkyjy5534zvkknwlwzbcwjoizszruhokslatavip5q",
        ("same-tick-reuse", 2): "#bb.event.sha256.c2aidqhefgmovroufymmonnh2yzk77dgg7z55xiw4cag54euht2q",
        ("warm-burn", 0): "#bb.event.sha256.j7h56ji37pgsj3s672a532cdm3eb3vu6zstx6pqehpwx36wbobwa",
    }
    expected_effects = {
        "empty": [],
        "cold-cycle": ["mint", "already-on", "burn", "already-off", "mint"],
        "same-tick-reuse": ["mint", "burn", "mint", "burn"],
        "warm-burn": ["burn"],
        "noop-head": ["already-off", "mint"],
        "toggle-pair": ["mint", "burn"],
        "bedrock": ["bedrock-protected"] * 3,
        "edge": ["mint"],
    }
    expected_final = {
        "empty": (False, None, 0),
        "cold-cycle": (True, "event[4]", 3),
        "same-tick-reuse": (False, None, 4),
        "warm-burn": (False, None, 1),
        "noop-head": (True, "event[1]", 1),
        "toggle-pair": (False, None, 2),
        "bedrock": (True, None, 0),
        "edge": (True, "event[0]", 1),
    }

    assert cases["empty"].receipt is None
    for name, expected in expected_receipts.items():
        require_tag(cases[name].receipt, expected, f"{name} receipt")
    for (name, index), expected in expected_events.items():
        require_tag(cases[name].effects[index].event, expected, f"{name} event[{index}]")
    for name, run in cases.items():
        assert [effect.kind for effect in run.effects] == expected_effects[name]
        final_on, active_label, changes = expected_final[name]
        assert run.final_on == final_on and run.changes == changes
        if active_label is None:
            assert run.final_mint is None
        else:
            active_index = int(active_label[6:-1])
            assert run.final_mint == run.effects[active_index].event

    cold = cases["cold-cycle"]
    assert cold.effects[2].consumed_mint == cold.effects[0].event
    reuse = cases["same-tick-reuse"]
    assert reuse.effects[1].consumed_mint == reuse.effects[0].event
    assert reuse.effects[3].consumed_mint == reuse.effects[2].event
    warm = cases["warm-burn"]
    assert warm.effects[0].consumed_mint == initial_on_parent

    noop = cases["noop-head"]
    correct_mint = event_tag(noop.effects[0].event, 11, "on", P)
    skipped_noop_mint = event_tag(initial_off_parent, 11, "on", P)
    assert noop.effects[1].event == correct_mint
    assert noop.effects[1].event != skipped_noop_mint

    negative_controls = {
        "undefined-operation": lambda: run_scalar(P, False, None, initial_off_parent, 1, ["erase"]),
        "different-coordinate": lambda: run_scalar_edits(
            P, False, None, initial_off_parent, 1, [("on", (9, 9, 9))]
        ),
        "off-with-mint": lambda: run_scalar(P, False, initial_off_parent, initial_off_parent, 1, []),
        "on-without-mint": lambda: run_scalar(P, True, None, initial_on_parent, 1, []),
        "bedrock-with-mint": lambda: run_scalar(
            BEDROCK, True, initial_off_parent, initial_off_parent, 1, []
        ),
        "non-receipt-parent": lambda: run_scalar(
            P, False, None, cases["cold-cycle"].effects[0].event, 1, []
        ),
        "out-of-range-coordinate": lambda: run_scalar(
            (1 << 63, 0, 0), False, None, initial_off_parent, 1, []
        ),
    }
    for label, thunk in negative_controls.items():
        require_rejected(label, thunk)

    for name, run in cases.items():
        active = (
            "none"
            if run.final_mint is None
            else f"event[{next(i for i, effect in enumerate(run.effects) if effect.event == run.final_mint)}]"
        )
        receipt = "none" if run.receipt is None else run.receipt.text()
        print(
            f"{name:16} receipt={receipt} changes={run.changes} "
            f"final={'on' if run.final_on else 'off'} active={active}"
        )
    print(f"negative-controls  {len(negative_controls)} rejected")
    print(f"semantic-batch-v1 vectors: PASS ({len(cases)} cases)")


if __name__ == "__main__":
    main()
