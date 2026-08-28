#!/usr/bin/env python3
"""Independent stdlib vectors for the optimization-equivalence frame-ledger evidence tag.

This file intentionally imports no Bitblock/Rust code. It reconstructs the canonical payload --
including the nested TickOutcome v3 and FrameReceiptV2 bytes it frames -- and independently
applies the content-tag wrapper.

`bitblock/optimization-equivalence/frame-ledger-evidence/v1` is what an optimization-equivalence
output lane commits to instead of a bare ledger head. A head alone would let two runs that ended
on the same frame look identical while having taken different paths there, so this object commits
to the *whole retained ledger*:

    stream_role:u8
    u64_be(genesis_len) || canonical TickOutcome bytes
    u64_be(admission_count)
    per admission:
        canonical FrameReceiptV2 bytes [130]
        u64_be(successor_count)
        per successor: u64_be(len) || canonical TickOutcome bytes

Every variable-length member is length-prefixed, so the concatenation is unambiguous without a
separator, and the local verifier's own resource limits are deliberately outside the hashed
domain -- they are a property of the machine reading the ledger, not of the ledger.

The vectors below vary the three things the schema exists to bind: the stream role, the genesis
outcome, and an intermediate successor that leaves the selected final frame untouched. That last
control is the point of the object -- it is exactly the case a head-only commitment would miss.
"""

from __future__ import annotations

import base64
import hashlib
import struct

TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
KIND = b"receipt"
SCHEMA = b"bitblock/optimization-equivalence/frame-ledger-evidence/v1"

OUTCOME_KIND = b"tick-outcome"
OUTCOME_SCHEMA = b"bitblock/tick-outcome/v3"
OUTCOME_FIXED_LEN = 469

RECEIPT_KIND = b"frame-receipt"
RECEIPT_SCHEMA = b"bitblock/frame-receipt/v2"
RECEIPT_LEN = 130

STREAM_GAME = 0
STREAM_COMPOSITE = 1

VIEWPORT = 801
OUTPUT_WIDTH = 1201


def framed(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def u64(value: int) -> bytes:
    return struct.pack(">Q", value)


def u32(value: int) -> bytes:
    return struct.pack(">I", value)


def u16(value: int) -> bytes:
    return struct.pack(">H", value)


def f64_bits(value: float) -> int:
    return struct.unpack(">Q", struct.pack(">d", value))[0]


def sequence(start: int) -> bytes:
    """The same byte-ladder the sibling TickOutcome and FrameReceiptV2 vectors use."""
    return bytes((start + index) & 0xFF for index in range(32))


def content_digest(kind: bytes, schema: bytes, payload: bytes) -> bytes:
    preimage = (
        framed(TAG_DOMAIN)
        + framed(kind)
        + framed(HASH_SUITE)
        + framed(schema)
        + u64(len(payload))
        + payload
    )
    return hashlib.sha256(preimage).digest()


def render(kind: bytes, digest: bytes) -> str:
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.{kind.decode('ascii')}.sha256.{encoded}"


# --- nested TickOutcome v3 ---------------------------------------------------------------

CODEC_CAMERA = (
    f64_bits(0.0),
    f64_bits(-0.0),
    f64_bits(-1.0),
    f64_bits(0.0),
    f64_bits(-0.0),
    f64_bits(0.0),
    f64_bits(0.0),
    f64_bits(0.0),
    f64_bits(1.0),
    f64_bits(1.0),
)


def optional_digest(digest: bytes | None) -> bytes:
    return (b"\x00" + bytes(32)) if digest is None else (b"\x01" + digest)


def optional_frame(pair: tuple[bytes, bytes] | None) -> bytes:
    if pair is None:
        return b"\x00" + bytes(64)
    return b"\x01" + pair[0] + pair[1]


def encode_outcome(
    parent: bytes | None,
    tick: int,
    pre_world: bytes,
    pre_history: bytes,
    post_world: bytes,
    post_history: bytes,
    game: tuple[bytes, bytes] | None = None,
    composite: tuple[bytes, bytes] | None = None,
    flags: int = 0,
) -> bytes:
    """One attempt-free TickOutcome v3. The fixed payload is 469 bytes."""
    if (parent is None) != (tick == 0):
        raise ValueError("only tick zero may omit its parent")
    payload = optional_digest(parent) + u64(tick) + pre_world + pre_history + u16(flags)
    for bits in (f64_bits(0.0), f64_bits(-0.0), f64_bits(0.0)):
        payload += u64(bits)
    for dimension in (VIEWPORT, VIEWPORT, OUTPUT_WIDTH, VIEWPORT):
        payload += u32(dimension)
    for _ in range(10):  # v3 accepted-action ledger; these ticks consumed nothing
        payload += u32(0)
    payload += u64(0)  # attempt count
    payload += post_world + post_history
    for bits in CODEC_CAMERA:
        payload += u64(bits)
    payload += optional_frame(game) + optional_frame(composite)
    if len(payload) != OUTCOME_FIXED_LEN:
        raise ValueError(f"attempt-free outcome must be {OUTCOME_FIXED_LEN} bytes")
    return payload


def outcome_digest(payload: bytes) -> bytes:
    return content_digest(OUTCOME_KIND, OUTCOME_SCHEMA, payload)


# --- nested FrameReceiptV2 ---------------------------------------------------------------


def encode_receipt(
    parent: bytes | None, stream: int, outcome: bytes, frame_input: bytes, frame: bytes
) -> bytes:
    if stream not in (STREAM_GAME, STREAM_COMPOSITE):
        raise ValueError(f"unknown stream code {stream}")
    payload = optional_digest(parent) + bytes((stream,)) + outcome + frame_input + frame
    if len(payload) != RECEIPT_LEN:
        raise ValueError(f"frame receipt v2 must be {RECEIPT_LEN} bytes")
    return payload


# --- the ledger evidence payload ---------------------------------------------------------


def encode_ledger_evidence(
    stream: int,
    genesis: bytes,
    admissions: tuple[tuple[bytes, tuple[bytes, ...]], ...] = (),
) -> bytes:
    if stream not in (STREAM_GAME, STREAM_COMPOSITE):
        raise ValueError(f"unknown stream role {stream}")
    payload = bytearray((stream,))
    payload += u64(len(genesis)) + genesis
    payload += u64(len(admissions))
    for receipt, successors in admissions:
        if len(receipt) != RECEIPT_LEN:
            raise ValueError(f"admission receipt must be {RECEIPT_LEN} bytes")
        payload += receipt
        payload += u64(len(successors))
        for successor in successors:
            payload += u64(len(successor)) + successor
    return bytes(payload)


def evidence_tag(payload: bytes) -> str:
    return render(KIND, content_digest(KIND, SCHEMA, payload))


# --- fixtures ----------------------------------------------------------------------------

GENESIS_WORLD = sequence(0x00)
GENESIS_HISTORY = sequence(0x20)
GAME_INPUT = sequence(0xA0)
GAME_FRAME = sequence(0xC0)
COMPOSITE_INPUT = sequence(0xE0)
COMPOSITE_FRAME = sequence(0x10)
ALTERNATE_INPUT = sequence(0x30)
ALTERNATE_FRAME = sequence(0x50)


def genesis_outcome() -> bytes:
    """Tick zero, idle: no parent, no attempts, no frames, world and history unchanged."""
    return encode_outcome(
        parent=None,
        tick=0,
        pre_world=GENESIS_WORLD,
        pre_history=GENESIS_HISTORY,
        post_world=GENESIS_WORLD,
        post_history=GENESIS_HISTORY,
    )


def presenting_outcome(parent: bytes, tick: int) -> bytes:
    """A successor that commits both frame pairs, so either stream can select from it."""
    return encode_outcome(
        parent=parent,
        tick=tick,
        pre_world=GENESIS_WORLD,
        pre_history=GENESIS_HISTORY,
        post_world=GENESIS_WORLD,
        post_history=GENESIS_HISTORY,
        game=(GAME_INPUT, GAME_FRAME),
        composite=(COMPOSITE_INPUT, COMPOSITE_FRAME),
    )


def idle_outcome(parent: bytes, tick: int) -> bytes:
    """An unpresented intermediate tick. It changes nothing a head-only commitment would see."""
    return encode_outcome(
        parent=parent,
        tick=tick,
        pre_world=GENESIS_WORLD,
        pre_history=GENESIS_HISTORY,
        post_world=GENESIS_WORLD,
        post_history=GENESIS_HISTORY,
    )


def one_admission_evidence(stream: int, skip_an_idle_tick: bool) -> bytes:
    """A ledger holding exactly one admission, optionally through one extra idle tick.

    Both spellings end on the same selected frame and the same receipt fields except for the
    named endpoint outcome. Only the retained path differs.
    """
    genesis = genesis_outcome()
    successors: list[bytes] = []
    parent = outcome_digest(genesis)
    tick = 1
    if skip_an_idle_tick:
        idle = idle_outcome(parent, tick)
        successors.append(idle)
        parent = outcome_digest(idle)
        tick += 1
    endpoint = presenting_outcome(parent, tick)
    successors.append(endpoint)
    frame_input, frame = (
        (GAME_INPUT, GAME_FRAME) if stream == STREAM_GAME else (COMPOSITE_INPUT, COMPOSITE_FRAME)
    )
    receipt = encode_receipt(None, stream, outcome_digest(endpoint), frame_input, frame)
    return encode_ledger_evidence(stream, genesis, ((receipt, tuple(successors)),))


def main() -> None:
    genesis = genesis_outcome()
    print(f"nested genesis outcome  {len(genesis)} bytes  {render(OUTCOME_KIND, outcome_digest(genesis))}")

    game_only = encode_ledger_evidence(STREAM_GAME, genesis)
    composite_only = encode_ledger_evidence(STREAM_COMPOSITE, genesis)
    print("\ngenesis-only ledgers, no admissions")
    print(f"  game       payload {len(game_only)}  {evidence_tag(game_only)}")
    print(f"  composite  payload {len(composite_only)}  {evidence_tag(composite_only)}")
    if len(game_only) != 1 + 8 + OUTCOME_FIXED_LEN + 8:
        raise SystemExit("genesis-only framing is not stream || len || outcome || count")

    game_one = one_admission_evidence(STREAM_GAME, skip_an_idle_tick=False)
    composite_one = one_admission_evidence(STREAM_COMPOSITE, skip_an_idle_tick=False)
    print("\none admission, one successor")
    print(f"  game       payload {len(game_one)}  {evidence_tag(game_one)}")
    print(f"  composite  payload {len(composite_one)}  {evidence_tag(composite_one)}")

    game_skipped = one_admission_evidence(STREAM_GAME, skip_an_idle_tick=True)
    print("\none admission through one extra idle tick, same selected frame")
    print(f"  game       payload {len(game_skipped)}  {evidence_tag(game_skipped)}")

    # The stream role is hashed content, not an in-memory label: one serialized ledger must not
    # be relabelled between the inner game raster and the full client composite.
    if evidence_tag(game_only) == evidence_tag(composite_only):
        raise SystemExit("the stream role is not inside the evidence tag")
    if evidence_tag(game_one) == evidence_tag(composite_one):
        raise SystemExit("the stream role is not inside an admission-bearing evidence tag")

    # This is the property a head-only commitment cannot have. The skipped-tick ledger ends on
    # the same frame, under the same stream, with a receipt naming a different endpoint only
    # because one otherwise invisible idle tick was retained -- and the tag must move.
    if evidence_tag(game_skipped) == evidence_tag(game_one):
        raise SystemExit("an intermediate successor does not move the evidence tag")
    if len(game_skipped) - len(game_one) != OUTCOME_FIXED_LEN + 8:
        raise SystemExit("one retained successor must cost its length prefix plus its bytes")

    # Admitting is not free either: the count lane distinguishes an empty ledger from one that
    # admitted a receipt, even before any of the receipt bytes are read.
    if evidence_tag(game_one) == evidence_tag(game_only):
        raise SystemExit("the admission count is not inside the evidence tag")

    # The genesis outcome is inside the tag, not merely its length.
    moved_genesis = bytearray(genesis)
    moved_genesis[33] ^= 0x01  # one bit of the pre-world digest
    if evidence_tag(encode_ledger_evidence(STREAM_GAME, bytes(moved_genesis))) == evidence_tag(
        game_only
    ):
        raise SystemExit("the genesis outcome bytes are not inside the evidence tag")

    # Length prefixes make the concatenation unambiguous. Moving a byte across the boundary
    # between the successor count and the first successor must not re-parse as the same ledger.
    shifted = bytearray(game_one)
    boundary = game_one.index(b"\x00" * 7 + b"\x01", 1 + 8 + OUTCOME_FIXED_LEN + 8 + RECEIPT_LEN)
    shifted[boundary + 7] = 0x02
    if evidence_tag(bytes(shifted)) == evidence_tag(game_one):
        raise SystemExit("the successor count is not inside the evidence tag")

    # Domain separation: the same bytes are not an optimization output or a v1 ledger evidence.
    for schema in (
        b"bitblock/optimization-equivalence/output/v3",
        b"bitblock/optimization-equivalence/frame-ledger-evidence/v2",
    ):
        if render(KIND, content_digest(KIND, schema, game_only)) == evidence_tag(game_only):
            raise SystemExit(f"evidence collides with {schema!r}")

    # Refusals: the framing has no spelling for an unknown stream or a mis-sized receipt.
    for bad_stream in (2, 255):
        try:
            encode_ledger_evidence(bad_stream, genesis)
        except ValueError:
            continue
        raise SystemExit(f"accepted stream role {bad_stream}")
    for bad_receipt in (b"", bytes(RECEIPT_LEN - 1), bytes(RECEIPT_LEN + 1)):
        try:
            encode_ledger_evidence(STREAM_GAME, genesis, ((bad_receipt, ()),))
        except ValueError:
            continue
        raise SystemExit(f"accepted a {len(bad_receipt)}-byte admission receipt")

    print("\n5 pinned ledgers, 7 controls, 5 refusals")


if __name__ == "__main__":
    main()
