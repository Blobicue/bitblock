#!/usr/bin/env python3
"""Independent stdlib vectors for Bitblock TickOutcome v3.

This file intentionally imports no Bitblock/Rust code. It reconstructs the canonical payload,
strictly decodes it, re-encodes it, and independently applies the content-tag wrapper.
"""

from __future__ import annotations

import base64
import hashlib
import math
import struct


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
KIND = b"tick-outcome"
SCHEMA = b"bitblock/tick-outcome/v3"
FIXED_PAYLOAD_LEN = 469

# v3 adds ten u32 accepted-action counters between the input dimensions and the
# attempt count. Lane order is the interaction-tape opcode order.
ACTION_LANE_COUNT = 10
MAX_ACTIONS_PER_TICK = 65_536
LANE_AIM_DELETE = 3
LANE_AIM_PLACE = 4
LANE_EXACT_EDIT = 5


def lanes_for_attempts(attempts) -> tuple:
    """The minimal ledger a set of dispositions implies.

    Derived from the attempt codes alone, so this file never has to be told what
    the reconciliation should be -- it recomputes it. Codes: 0/2 are aim-delete,
    1/3/4 aim-place, 5/6 exact.
    """
    lanes = [0] * ACTION_LANE_COUNT
    for code, _ in attempts:
        if code in (0, 2):
            lanes[LANE_AIM_DELETE] += 1
        elif code in (1, 3, 4):
            lanes[LANE_AIM_PLACE] += 1
        elif code in (5, 6):
            lanes[LANE_EXACT_EDIT] += 1
        else:
            raise AssertionError("unknown attempt code %d" % code)
    return tuple(lanes)
HEADER_LEN = 81
FLAG_MASK = (1 << 11) - 1
EDIT_LEN = 25
MIN_OBSERVATION_DISTANCE = 1.0e-9
MAX_OBSERVATION_DISTANCE = 1.0e150
MAX_ABS_PITCH = math.pi / 2.0 - 1.0e-6


def framed(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def u16(value: int) -> bytes:
    return struct.pack(">H", value)


def u32(value: int) -> bytes:
    return struct.pack(">I", value)


def u64(value: int) -> bytes:
    return struct.pack(">Q", value)


def i64(value: int) -> bytes:
    return struct.pack(">q", value)


def f64_bits(value: float) -> int:
    return struct.unpack(">Q", struct.pack(">d", value))[0]


def sequence(start: int) -> bytes:
    return bytes((start + index) & 0xFF for index in range(32))


def edit(operation: int, x: int, y: int, z: int) -> bytes:
    if operation not in (0, 1, 2):
        raise ValueError("undefined edit operation")
    return bytes((operation,)) + i64(x) + i64(y) + i64(z)


def optional_digest(digest: bytes | None) -> bytes:
    return (b"\x00" + bytes(32)) if digest is None else (b"\x01" + digest)


def optional_frame(pair: tuple[bytes, bytes] | None) -> bytes:
    if pair is None:
        return b"\x00" + bytes(64)
    frame_input, frame = pair
    return b"\x01" + frame_input + frame


def encode(fields: dict) -> bytes:
    payload = bytearray()
    payload += optional_digest(fields["parent"])
    payload += u64(fields["tick"])
    payload += fields["pre_world"]
    payload += fields["pre_history"]
    payload += u16(fields["flags"])
    for bits in fields["motion_bits"]:
        payload += u64(bits)
    for dimension in fields["dimensions"]:
        payload += u32(dimension)
    lanes = fields.get("lanes")
    if lanes is None:
        lanes = lanes_for_attempts(fields["attempts"])
    assert len(lanes) == ACTION_LANE_COUNT, "a ledger has one counter per lane"
    for lane in lanes:
        payload += u32(lane)
    payload += u64(len(fields["attempts"]))
    for code, encoded_edit in fields["attempts"]:
        payload += bytes((code,))
        if encoded_edit is not None:
            payload += encoded_edit
    payload += fields["post_world"]
    payload += fields["post_history"]
    for bits in fields["camera_bits"]:
        payload += u64(bits)
    payload += optional_frame(fields["game"])
    payload += optional_frame(fields["composite"])
    return bytes(payload)


def decode_optional(data: bytes, cursor: int, size: int, field: str):
    marker = data[cursor]
    body = data[cursor + 1 : cursor + 1 + size]
    if marker == 0:
        if body != bytes(size):
            raise ValueError(f"{field}: nonzero absent padding")
        return None, cursor + 1 + size
    if marker == 1:
        return body, cursor + 1 + size
    raise ValueError(f"{field}: unknown marker {marker}")


def decode(payload: bytes) -> dict:
    if len(payload) < FIXED_PAYLOAD_LEN:
        raise ValueError("payload too short")
    cursor = 0
    parent, cursor = decode_optional(payload, cursor, 32, "parent")
    tick = struct.unpack(">Q", payload[cursor : cursor + 8])[0]
    cursor += 8
    if (parent is None) != (tick == 0):
        raise ValueError("only tick zero may omit its parent")
    pre_world = payload[cursor : cursor + 32]
    cursor += 32
    pre_history = payload[cursor : cursor + 32]
    cursor += 32
    flags = struct.unpack(">H", payload[cursor : cursor + 2])[0]
    cursor += 2
    if flags & ~FLAG_MASK:
        raise ValueError("reserved flags")
    motion_bits = []
    for _ in range(3):
        bits = struct.unpack(">Q", payload[cursor : cursor + 8])[0]
        cursor += 8
        value = struct.unpack(">d", u64(bits))[0]
        if not math.isfinite(value):
            raise ValueError("non-finite motion")
        motion_bits.append(bits)
    dimensions = []
    for _ in range(4):
        dimensions.append(struct.unpack(">I", payload[cursor : cursor + 4])[0])
        cursor += 4
    sim_width, sim_height, output_width, output_height = dimensions
    if min(dimensions) == 0 or output_width < sim_width or output_height != sim_height:
        raise ValueError("inconsistent dimensions")
    lanes = []
    for _ in range(ACTION_LANE_COUNT):
        lanes.append(struct.unpack(">I", payload[cursor : cursor + 4])[0])
        cursor += 4
    lanes = tuple(lanes)
    for lane, count in enumerate(lanes):
        if count > MAX_ACTIONS_PER_TICK:
            raise ValueError(
                f"action lane {lane} exceeds per-tick limit "
                f"{MAX_ACTIONS_PER_TICK}: {count}"
            )
    action_total = sum(lanes)
    if action_total > MAX_ACTIONS_PER_TICK:
        raise ValueError(
            f"action total exceeds per-tick limit "
            f"{MAX_ACTIONS_PER_TICK}: {action_total}"
        )
    count = struct.unpack(">Q", payload[cursor : cursor + 8])[0]
    cursor += 8
    if count > len(payload) - FIXED_PAYLOAD_LEN:
        raise ValueError("attempt count cannot fit")
    attempts = []
    for _ in range(count):
        code = payload[cursor]
        cursor += 1
        if code in (0, 1):
            attempts.append((code, None))
            continue
        if code not in (2, 3, 4, 5, 6):
            raise ValueError(f"unknown attempt code {code}")
        encoded_edit = payload[cursor : cursor + EDIT_LEN]
        if len(encoded_edit) != EDIT_LEN:
            raise ValueError("truncated edit")
        cursor += EDIT_LEN
        if encoded_edit[0] not in (0, 1, 2):
            raise ValueError("unknown edit operation")
        if code == 2 and encoded_edit[0] != 1:
            raise ValueError("aim delete must resolve to off")
        if code in (3, 4) and encoded_edit[0] != 0:
            raise ValueError("aim place must resolve to on")
        if code == 5 and encoded_edit[0] == 1:
            raise ValueError("off cannot be rejected for enclosing the observer")
        attempts.append((code, encoded_edit))
    expected = cursor + 274
    if len(payload) != expected:
        raise ValueError(f"wrong payload length {len(payload)} != {expected}")
    # Lane by lane, not by total: attempts carry their kind, so a ledger with a
    # correct sum and a wrong split (two aim-deletes against one aim-delete and
    # one aim-place) must still be refused.
    expected_lanes = lanes_for_attempts(attempts)
    for lane in (LANE_AIM_DELETE, LANE_AIM_PLACE, LANE_EXACT_EDIT):
        if lanes[lane] != expected_lanes[lane]:
            raise ValueError(
                "lane %d counted %d submitted edits against %d dispositions"
                % (lane, lanes[lane], expected_lanes[lane])
            )
    post_world = payload[cursor : cursor + 32]
    cursor += 32
    post_history = payload[cursor : cursor + 32]
    cursor += 32
    camera_bits = []
    for lane in range(10):
        bits = struct.unpack(">Q", payload[cursor : cursor + 8])[0]
        cursor += 8
        value = struct.unpack(">d", u64(bits))[0]
        if not math.isfinite(value):
            raise ValueError(f"non-finite camera lane {lane}")
        camera_bits.append(bits)
    distance = struct.unpack(">d", u64(camera_bits[8]))[0]
    if not MIN_OBSERVATION_DISTANCE <= distance <= MAX_OBSERVATION_DISTANCE:
        raise ValueError("observation distance outside supported range")
    yaw = struct.unpack(">d", u64(camera_bits[3]))[0]
    if not -math.pi <= yaw < math.pi:
        raise ValueError("unwrapped camera yaw")
    pitch = struct.unpack(">d", u64(camera_bits[4]))[0]
    if not -MAX_ABS_PITCH <= pitch <= MAX_ABS_PITCH:
        raise ValueError("unclamped camera pitch")
    fov = struct.unpack(">d", u64(camera_bits[9]))[0]
    if not 0.0 < fov < math.pi:
        raise ValueError("invalid vertical FOV")
    game_raw, cursor = decode_optional(payload, cursor, 64, "game")
    composite_raw, cursor = decode_optional(payload, cursor, 64, "composite")
    game = None if game_raw is None else (game_raw[:32], game_raw[32:])
    composite = (
        None if composite_raw is None else (composite_raw[:32], composite_raw[32:])
    )
    if game is not None and composite is None:
        raise ValueError("game frame without composite")
    if game is None and (
        flags & ((1 << 6) | (1 << 7) | (1 << 10))
        or struct.unpack(">d", u64(motion_bits[0]))[0] != 0.0
        or struct.unpack(">d", u64(motion_bits[1]))[0] != 0.0
    ):
        raise ValueError("tick inputs force a missing game frame")
    if flags & (1 << 8) and composite is None:
        raise ValueError("sidebar dirty state requires a composite frame")
    if composite is not None and game is None and not flags & (1 << 8):
        raise ValueError("unexpected composite frame")
    if pre_world != post_world and game is None:
        raise ValueError("changed world without game frame")
    has_executed_attempt = any(code in (2, 4, 6) for code, _ in attempts)
    history_advanced = pre_history != post_history
    if has_executed_attempt != history_advanced:
        raise ValueError("executed-attempt presence differs from history advancement")
    if pre_world != post_world and not history_advanced:
        raise ValueError("changed world without history advancement")
    if cursor != len(payload):
        raise ValueError("trailing bytes")
    return {
        "parent": parent,
        "tick": tick,
        "pre_world": pre_world,
        "pre_history": pre_history,
        "flags": flags,
        "motion_bits": tuple(motion_bits),
        "dimensions": tuple(dimensions),
        "lanes": lanes,
        "attempts": tuple(attempts),
        "post_world": post_world,
        "post_history": post_history,
        "camera_bits": tuple(camera_bits),
        "game": game,
        "composite": composite,
    }


def canonical_preimage(payload: bytes) -> bytes:
    return (
        framed(TAG_DOMAIN)
        + framed(KIND)
        + framed(HASH_SUITE)
        + framed(SCHEMA)
        + u64(len(payload))
        + payload
    )


def content_tag(payload: bytes) -> str:
    digest = hashlib.sha256(canonical_preimage(payload)).digest()
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.tick-outcome.sha256.{encoded}"


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


def genesis_idle() -> dict:
    return {
        "parent": None,
        "tick": 0,
        "pre_world": sequence(0x00),
        "pre_history": sequence(0x20),
        "flags": 0,
        "motion_bits": (f64_bits(0.0), f64_bits(-0.0), f64_bits(0.0)),
        "dimensions": (801, 801, 1201, 801),
        "attempts": (),
        "post_world": sequence(0x00),
        "post_history": sequence(0x20),
        "camera_bits": CODEC_CAMERA,
        "game": None,
        "composite": None,
    }


def mixed_all_variants() -> dict:
    return {
        "parent": sequence(0x00),
        "tick": 0x0102_0304_0506_0708,
        "pre_world": sequence(0x20),
        "pre_history": sequence(0x40),
        "flags": FLAG_MASK,
        "motion_bits": (0x8000_0000_0000_0000, 1, 0x7FEF_FFFF_FFFF_FFFF),
        "dimensions": (0x0102_0304, 0x1112_1314, 0x2122_2324, 0x1112_1314),
        "attempts": (
            (0, None),
            (1, None),
            (2, edit(1, -(1 << 63), -2, (1 << 63) - 1)),
            (3, edit(0, -1, 0, 1)),
            (4, edit(0, 2, -3, 4)),
            (5, edit(2, -5, 6, -7)),
            (6, edit(1, 8, -9, 10)),
        ),
        "post_world": sequence(0x60),
        "post_history": sequence(0x80),
        "camera_bits": (
            0x0000_0000_0000_0000,
            0x8000_0000_0000_0000,
            0x0000_0000_0000_0001,
            0x8000_0000_0000_0001,
            0x3FF0_0000_0000_0000,
            0xBFF0_0000_0000_0000,
            0x7FEF_FFFF_FFFF_FFFF,
            0xFFEF_FFFF_FFFF_FFFF,
            f64_bits(2.0),
            f64_bits(1.0),
        ),
        "game": (sequence(0xA0), sequence(0xC0)),
        "composite": (sequence(0xE0), sequence(0x10)),
    }


def sidebar_only() -> dict:
    return {
        "parent": sequence(0x11),
        "tick": 1,
        "pre_world": sequence(0x31),
        "pre_history": sequence(0x51),
        "flags": (1 << 8) | (1 << 9),
        "motion_bits": (f64_bits(0.0), f64_bits(0.0), f64_bits(-0.0)),
        "dimensions": (801, 801, 1201, 801),
        "attempts": (),
        "post_world": sequence(0x31),
        "post_history": sequence(0x51),
        "camera_bits": CODEC_CAMERA,
        "game": None,
        "composite": (sequence(0x71), sequence(0x91)),
    }


def control_lanes_only() -> dict:
    """An idle tick with four control occurrences across the three kinds v2 could not see.

    No edit was submitted, so nothing appears in the ordered dispositions, and
    The sidebar toggle occurs twice, with one resize and one motion clear. Under
    v2 this payload was byte-identical to a tick where the user did
    nothing at all. That indistinguishability is what v3 removes.
    """
    fields = genesis_idle()
    lanes = [0] * ACTION_LANE_COUNT
    lanes[7] = 2  # toggle sidebar, twice
    lanes[8] = 1  # resize
    lanes[9] = 1  # clear motion
    fields["lanes"] = tuple(lanes)
    return fields


VECTORS = {
    "genesis-idle": (
        genesis_idle,
        "1671e2c245695f10e592de025895140eca84f9936031a7fc28995ab07a3b7c7c",
        "#bb.tick-outcome.sha256.mdrfmhsgskam6cmt4n3zncrclswc26fjfowva6e7itavrs7zx2tq",
    ),
    "mixed-all-seven": (
        mixed_all_variants,
        "5983dd2fc0cd797a9bd6fb70fdeb19ae316c587a4c1ea406f766000718ea68d8",
        "#bb.tick-outcome.sha256.nkazbofarz6mjgss625grz4cxx62itnqhbplgcwwjyrr3657mqda",
    ),
    "control-lanes-only": (
        control_lanes_only,
        "42968ac648afd01427128e9c9551a2bee9579a7bf107909c6c3f97e7cbb41973",
        "#bb.tick-outcome.sha256.ckeafgihrlyc57iiyef4mjvkqrxsyfbiucrrb4zndfvp57h45xhq",
    ),
    "sidebar-only": (
        sidebar_only,
        "6cf9f6274bd78be3420ee5b568e651288d8c8ea6c9917e5af5f9a7c2130ed0b2",
        "#bb.tick-outcome.sha256.bzqggwzw2eygclvjcidemqtrhdpdozhioimfzrl44mbphbgoqvna",
    ),
}


REJECTED: list[str] = []


def expect_reject(name: str, payload: bytes, expected_error: str | None = None) -> None:
    """One mutation the strict decoder must refuse.

    Counted as it runs rather than tallied by hand, so adding a control moves
    the reported number. That count used to be a literal in the summary line and
    would have stayed 34 while a 35th control was added right beside it.
    """
    try:
        decode(payload)
    except (IndexError, struct.error, ValueError) as error:
        if expected_error is not None and expected_error not in str(error):
            raise SystemExit(
                f"negative control failed by the wrong mechanism: {name}: {error}"
            ) from error
        REJECTED.append(name)
        return
    raise SystemExit(f"negative control accepted: {name}")


def negative_controls() -> None:
    genesis = bytearray(encode(genesis_idle()))
    mixed = bytearray(encode(mixed_all_variants()))

    mutant = genesis.copy()
    mutant[33:41] = u64(1)
    expect_reject("missing parent after tick zero", mutant)

    mutant = mixed.copy()
    mutant[33:41] = u64(0)
    expect_reject("parent present at tick zero", mutant)

    mutant = genesis.copy()
    mutant[0] = 2
    expect_reject("unknown parent marker", mutant)

    mutant = genesis.copy()
    mutant[1] = 1
    expect_reject("nonzero absent parent padding", mutant)

    mutant = genesis.copy()
    mutant[105:107] = u16(1 << 11)
    expect_reject("reserved input flag", mutant)

    mutant = genesis.copy()
    mutant[107:115] = u64(0x7FF8_0000_0000_0001)
    expect_reject("non-finite motion", mutant)

    for index, value in enumerate((0, 800, 800, 800)):
        mutant = genesis.copy()
        mutant[131 + index * 4 : 135 + index * 4] = u32(value)
        expect_reject(f"dimension relation {index}", mutant)

    for lane in range(ACTION_LANE_COUNT):
        mutant = genesis.copy()
        start = 147 + lane * 4
        mutant[start : start + 4] = u32(MAX_ACTIONS_PER_TICK + 1)
        expect_reject(
            f"action lane {lane} exceeds per-tick limit",
            mutant,
            f"action lane {lane} exceeds per-tick limit",
        )

    mutant = genesis.copy()
    mutant[147:151] = u32(MAX_ACTIONS_PER_TICK)
    mutant[151:155] = u32(1)
    expect_reject(
        "action total exceeds per-tick limit",
        mutant,
        "action total exceeds per-tick limit",
    )

    mutant = genesis.copy()
    mutant[147:151] = u32(0xFFFF_FFFF)
    mutant[151:155] = u32(1)
    expect_reject(
        "action total cannot wrap through u32",
        mutant,
        "action lane 0 exceeds per-tick limit",
    )

    mutant = genesis.copy()
    mutant[187:195] = u64(1)
    expect_reject("attempt count exceeds payload", mutant)

    mutant = genesis.copy()
    mutant[187:195] = u64(1)
    mutant.insert(195, 7)
    expect_reject("unknown attempt code", mutant)

    mutant = mixed.copy()
    mutant[198] = 0  # code 2's edit must be Off (operation 1)
    expect_reject("aim operation mismatch", mutant)

    mutant = mixed.copy()
    mutant[276] = 1  # code 5 cannot reject Off for observer enclosure
    expect_reject("impossible exact rejection", mutant)

    mutant = genesis.copy()
    mutant[105:107] = u16(1 << 6)
    expect_reject("fit request without game frame", mutant)

    mutant = genesis.copy()
    mutant[105:107] = u16(1 << 10)
    expect_reject("viewport refit without game frame", mutant)

    mutant = genesis.copy()
    mutant[107:115] = u64(f64_bits(1.0))
    expect_reject("mouse motion without game frame", mutant)

    mutant = genesis.copy()
    mutant[195] ^= 1
    expect_reject("changed world without game frame", mutant)

    mutant = genesis.copy()
    mutant[227] ^= 1
    expect_reject("history advance without executed attempt", mutant)

    mutant = mixed.copy()
    mutant[-242:-210] = mutant[73:105]
    expect_reject("executed attempts without history advance", mutant)

    world_without_history = genesis_idle()
    world_without_history["post_world"] = sequence(0x60)
    world_without_history["game"] = (sequence(0xA0), sequence(0xC0))
    world_without_history["composite"] = (sequence(0xE0), sequence(0x10))
    expect_reject("world change without history advance", encode(world_without_history))

    mutant = genesis.copy()
    mutant[105:107] = u16(1 << 8)
    expect_reject("sidebar dirty without composite frame", mutant)

    camera_start = len(genesis) - 2 * 65 - 80
    mutant = genesis.copy()
    mutant[camera_start : camera_start + 8] = u64(0x7FF0_0000_0000_0000)
    expect_reject("non-finite camera", mutant)

    mutant = genesis.copy()
    mutant[camera_start + 64 : camera_start + 72] = u64(0)
    expect_reject("non-positive camera distance", mutant)

    mutant = genesis.copy()
    mutant[camera_start + 64 : camera_start + 72] = u64(
        f64_bits(MIN_OBSERVATION_DISTANCE / 2.0)
    )
    expect_reject("camera distance below supported minimum", mutant)

    mutant = genesis.copy()
    mutant[camera_start + 24 : camera_start + 32] = u64(f64_bits(math.pi))
    expect_reject("unwrapped camera yaw", mutant)

    mutant = genesis.copy()
    mutant[camera_start + 32 : camera_start + 40] = u64(
        f64_bits(MAX_ABS_PITCH + 1.0e-9)
    )
    expect_reject("unclamped camera pitch", mutant)

    mutant = genesis.copy()
    mutant[camera_start + 72 : camera_start + 80] = u64(f64_bits(math.pi))
    expect_reject("invalid camera FOV", mutant)

    game_without_composite = genesis_idle()
    game_without_composite["game"] = (sequence(0xA0), sequence(0xC0))
    expect_reject("game without composite", encode(game_without_composite))

    unexpected_composite = genesis_idle()
    unexpected_composite["composite"] = (sequence(0xA0), sequence(0xC0))
    expect_reject("unexpected composite", encode(unexpected_composite))

    mutant = genesis.copy()
    mutant[-130] = 2
    expect_reject("unknown frame marker", mutant)

    mutant = genesis.copy()
    mutant[-129] = 1
    expect_reject("nonzero absent frame padding", mutant)

    # Correct total, wrong split: mixed has one aim-delete-executed and one
    # aim-place-executed among its seven; move one count across and the sum is
    # unchanged. A total-only check would accept this.
    mutant = bytearray(mixed)
    mutant[147 + LANE_AIM_DELETE * 4 : 151 + LANE_AIM_DELETE * 4] = u32(
        struct.unpack(">I", bytes(mutant[147 + LANE_AIM_DELETE * 4 : 151 + LANE_AIM_DELETE * 4]))[0] + 1
    )
    mutant[147 + LANE_AIM_PLACE * 4 : 151 + LANE_AIM_PLACE * 4] = u32(
        struct.unpack(">I", bytes(mutant[147 + LANE_AIM_PLACE * 4 : 151 + LANE_AIM_PLACE * 4]))[0] - 1
    )
    expect_reject("edit lane split with a correct total", mutant)

    expect_reject("truncated payload", genesis[:-1])
    expect_reject("overlong payload", genesis + b"\x00")


def main() -> None:
    if HEADER_LEN != len(canonical_preimage(b"")):
        raise SystemExit("canonical header length drifted")
    for name, (build, expected_payload_hash, expected_tag) in VECTORS.items():
        payload = encode(build())
        decoded = decode(payload)
        if encode(decoded) != payload:
            raise SystemExit(f"{name}: decode/re-encode mismatch")
        payload_hash = hashlib.sha256(payload).hexdigest()
        preimage_len = len(canonical_preimage(payload))
        actual_tag = content_tag(payload)
        print(
            f"{name}\t{len(payload)}\t{preimage_len}\t{payload_hash}\t{actual_tag}"
        )
        if payload_hash != expected_payload_hash:
            raise SystemExit(f"{name}: payload hash mismatch: {payload_hash}")
        if actual_tag != expected_tag:
            raise SystemExit(f"{name}: tag mismatch: {actual_tag}")
    negative_controls()
    print("negative-controls	%d	rejected" % len(REJECTED))


if __name__ == "__main__":
    main()
