#!/usr/bin/env python3
"""Independent stdlib vectors for the three headless local-replay policy tags.

This file intentionally imports no Bitblock/Rust code. It reconstructs each canonical payload
from the declared constants and independently applies the content-tag wrapper.

`src/main.rs` declares three policies before it replays an interaction tape headlessly:

    bitblock/local-replay-genesis/headless-v1           the deterministic starting state
    bitblock/local-replay-rules/headless-v1             the fold and renderer rules
    bitblock/local-replay-admission-policy/headless-v1  the bounded work admission policy

Those three tags were pinned as Rust literals but had no second derivation: the Rust test and
the Rust encoder agreed with each other, which is not the same as being checked. This file is
the missing half. Every field below is rebuilt from the declared source constant -- the world is
one bedrock coordinate, the camera is the observer spawn, the renderer/crosshair/sidebar policy
digests are recomputed from their own constants -- so nothing here is a digest copied out of the
implementation it is supposed to check.

Two inputs are read as *data* rather than restated: the displayed-metric binding table in
`crates/core/src/sidebar.rs` and `docs/metrics/registry.tsv`, which the sidebar raster policy
commits to. `tools/sidebar_vectors.py` reads them the same way and for the same reason -- the
table is authored content, not a protocol constant, and transcribing it here would only create a
second copy to drift.

A policy tag is a transparent declaration of state and constants. It is not a hash of the
implementation, so it does not prove that any executable obeyed it.
"""

from __future__ import annotations

import base64
import hashlib
import re
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"

GENESIS_POLICY_SCHEMA = b"bitblock/local-replay-genesis/headless-v1"
RULES_POLICY_SCHEMA = b"bitblock/local-replay-rules/headless-v1"
ADMISSION_POLICY_SCHEMA = b"bitblock/local-replay-admission-policy/headless-v1"

WORLD_SCHEMA = b"bitblock/world/sorted-i64-be/v1"
HISTORY_GENESIS_SCHEMA = b"bitblock/history/genesis/v1"
RENDERER_PROFILE_SCHEMA = b"bitblock/renderer-profile/host-f64/v1"
CROSSHAIR_POLICY_SCHEMA = b"bitblock/overlay-policy/crosshair/v1"
SIDEBAR_POLICY_SCHEMA = b"bitblock/sidebar/raster-policy/v1"
COMPOSITE_POLICY_SCHEMA = b"bitblock/sidebar/composite-policy/v1"

POLICY_REVISION = 1

# The three tags `headless_local_replay_policy_tags_are_pinned` asserts in src/main.rs.
#
# Every other suite here only prints its tags and lets the Rust literal be the second checker.
# These three are the exception because there is no *new* Rust test to add: main.rs already
# pinned them, and what was missing was this derivation. Comparing against the Rust literals
# closes the other direction -- a wrong constant in this file now fails here rather than
# printing a plausible digest nobody reads. Nothing below is derived from these values.
PINNED_IN_MAIN_RS = {
    "genesis": "#bb.policy.sha256.eibnyx3zeig5myau5wyq5y4qsmp2pp6chclpeapjaiz5xojz7zcq",
    "rules": "#bb.policy.sha256.abg53isb3svtuxvxo2yq6mymbwnkv4rkwfzpg2euenjj3ug57yxq",
    "admission": "#bb.policy.sha256.f5mg62hs64szq5f4xumd76tpjd5dcmtxbqufwn2jnoxsmylzbrga",
}

# --- src/main.rs -------------------------------------------------------------------------
INITIAL_VIEWPORT = 801
SIDEBAR_WIDTH = 400
INITIAL_OUTPUT_WIDTH = INITIAL_VIEWPORT + SIDEBAR_WIDTH
TICK_RATE = 60.0
TICK_DURATION_NANOS = 1_000_000_000 // int(TICK_RATE)
BLOCK_FILL_DISTANCE = 0.25
OBSERVER_COLLISION_CLEARANCE = 1.0e-6
HEADLESS_LOCAL_REPLAY_MAX_EXECUTABLE_BYTES = 64 * 1024 * 1024
HEADLESS_SIMULATION_MAX_OUTPUT_PIXELS = 16_777_216
HEADLESS_RENDER_TRIANGLES_PER_BIT_BOUND = 12
HEADLESS_SIMULATION_MAX_RESIZE_PIXEL_WRITES = 268_435_456
HEADLESS_SIMULATION_MAX_OUTPUT_PIXEL_TICKS = 268_435_456
HEADLESS_SIMULATION_MAX_RASTER_SAMPLE_TESTS = 1_073_741_824
HEADLESS_SIMULATION_MAX_WORLD_COORDINATE_TESTS = 268_435_456
HEADLESS_SIMULATION_MAX_EDIT_COORDINATE_TESTS = 268_435_456
HEADLESS_SIMULATION_MAX_CHUNK_WORD_TESTS = 1_073_741_824
HEADLESS_SIMULATION_MAX_WORLD_ORDERING_UNITS = 1_073_741_824
HEADLESS_SIMULATION_MAX_POSSIBLE_CHUNKS = 65_536

# --- crates/core/src/interaction.rs ------------------------------------------------------
MAX_INTERACTION_TAPE_PAYLOAD_BYTES = 16 * 1024 * 1024
MAX_INTERACTION_TAPE_RECORDS = 16_384
MAX_INTERACTION_ACTIONS_PER_RECORD = 65_536
MAX_INTERACTION_TOTAL_ACTIONS = 262_144
HEADLESS_SIMULATION_MAX_SOURCE_BYTES = MAX_INTERACTION_TAPE_PAYLOAD_BYTES

# --- crates/core/src/camera.rs -----------------------------------------------------------
DEFAULT_MOUSE_SENSITIVITY = 0.0025
DEFAULT_DOLLY_LOG_STEP = 0.15
ONE_PIXEL_EPSILON = 1.0e-9
# core::f64::consts::FRAC_PI_3, spelled as its own decimal literal. `math.pi / 3` is one ulp
# away from the correctly rounded constant, and that single bit moves the genesis tag.
FRAC_PI_3 = 1.04719755119659774615421446109316763

# --- crates/core/src/render.rs -----------------------------------------------------------
NEAR_PLANE = 1.0e-7
BARYCENTRIC_EPSILON = -1.0e-10
SMALL_BIT_EXTENT = 1.0
PIXEL_CENTER_OFFSET = 0.5
F64_EPSILON = float.fromhex("0x1.0p-52")
BACKGROUND = 0x000000
AGGREGATE_BIT = 0x00FFFFFF
BEDROCK_PIXEL = 0x00FF9F1C
BEDROCK_TOP = 0x00FFC15A
BEDROCK_SIDE = 0x00C86F0A
BEDROCK_FRONT = 0x00E8890C
BEDROCK_BACK = 0x00A95508
RGB_MASK = 0x00FFFFFF
ORDINARY_TOP = 0x00FFFFFF
ORDINARY_SIDE = 0x00BDBDBD
ORDINARY_BACK = 0x00D8D8D8
ORDINARY_FRONT = 0x00989898
CROSSHAIR_GAP_RADIUS = 2
CROSSHAIR_ARM_LENGTH = 3

# --- crates/core/src/sidebar.rs ----------------------------------------------------------
SIDEBAR_COLORS = (0x00101318, 0x0038424D, 0x00F2C14E, 0x008DA2B8, 0x00E8EDF2)
SIDEBAR_POLICY_TEXT = b"font=bitblock-3x5-ascii/v1;glyph-scale=1;row-gap=2;predecessor-only=v1"

# --- crates/core/src/world.rs / history.rs -----------------------------------------------
BEDROCK_COORD = (0, 0, 0)

# --- crates/core/src/sidebar.rs, `enum DisplayStatus` ------------------------------------
DISPLAY_STATUS_AUDIT_DISABLED = 1


def field(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def content_digest(kind: bytes, schema: bytes, payload: bytes) -> bytes:
    preimage = (
        field(TAG_DOMAIN)
        + field(kind)
        + field(HASH_SUITE)
        + field(schema)
        + struct.pack(">Q", len(payload))
        + payload
    )
    return hashlib.sha256(preimage).digest()


def render(kind: bytes, digest: bytes) -> str:
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.{kind.decode('ascii')}.sha256.{encoded}"


def u16(value: int) -> bytes:
    return struct.pack(">H", value)


def u32(value: int) -> bytes:
    return struct.pack(">I", value)


def u64(value: int) -> bytes:
    return struct.pack(">Q", value)


def u128(value: int) -> bytes:
    if not 0 <= value < (1 << 128):
        raise ValueError(f"value must fit u128, found {value}")
    return value.to_bytes(16, "big")


def f64(value: float) -> bytes:
    return struct.pack(">d", value)


# --- upstream objects the three policies name --------------------------------------------


def world_digest(coords: tuple[tuple[int, int, int], ...]) -> bytes:
    """Sorted i64-be coordinates. The schema is repeated inside the payload, then the count."""
    ordered = sorted(coords)
    payload = u16(len(WORLD_SCHEMA)) + WORLD_SCHEMA + u64(len(ordered))
    for x, y, z in ordered:
        payload += struct.pack(">qqq", x, y, z)
    return content_digest(b"world", WORLD_SCHEMA, payload)


def history_genesis_digest(world: bytes) -> bytes:
    return content_digest(b"receipt", HISTORY_GENESIS_SCHEMA, world)


def renderer_profile_digest() -> bytes:
    payload = b"".join(u16(POLICY_REVISION) for _ in range(4))
    payload += b"\x00"  # current host Rust f64 arithmetic
    payload += b"\x00"  # total_cmp depth; equal depth chooses lower RGB
    for value in (
        NEAR_PLANE,
        BARYCENTRIC_EPSILON,
        ONE_PIXEL_EPSILON,
        SMALL_BIT_EXTENT,
        PIXEL_CENTER_OFFSET,
        F64_EPSILON,
    ):
        payload += f64(value)
    for color in (
        RGB_MASK,
        BACKGROUND,
        AGGREGATE_BIT,
        BEDROCK_PIXEL,
        BEDROCK_TOP,
        BEDROCK_SIDE,
        BEDROCK_FRONT,
        BEDROCK_BACK,
        ORDINARY_TOP,
        ORDINARY_SIDE,
        ORDINARY_BACK,
        ORDINARY_FRONT,
    ):
        payload += u32(color)
    if len(payload) != 106:
        raise ValueError(f"renderer profile payload must be 106 bytes, found {len(payload)}")
    return content_digest(b"policy", RENDERER_PROFILE_SCHEMA, payload)


def crosshair_policy_digest() -> bytes:
    payload = (
        u16(POLICY_REVISION)
        + b"\x00"  # integer-divided physical-pixel center
        + u32(CROSSHAIR_GAP_RADIUS)
        + u32(CROSSHAIR_ARM_LENGTH)
        + u32(RGB_MASK)
        + b"\x00"  # the overlay never writes depth
    )
    if len(payload) != 16:
        raise ValueError(f"crosshair policy payload must be 16 bytes, found {len(payload)}")
    return content_digest(b"policy", CROSSHAIR_POLICY_SCHEMA, payload)


def displayed_metric_bindings() -> list[tuple[bytes, bytes]]:
    """The sidebar's authored alias -> definition-tag table, checked against the registry."""
    source = (ROOT / "crates" / "core" / "src" / "sidebar.rs").read_text(encoding="utf-8")
    matches = re.findall(r'binding!\(\s*"([^"]+)",\s*"([^"]+)"\s*\)', source, re.MULTILINE)
    if not matches:
        raise SystemExit("no displayed-metric bindings found in sidebar.rs")
    registry = (ROOT / "docs" / "metrics" / "registry.tsv").read_text(encoding="utf-8")
    rows = registry.splitlines()
    for alias, definition in matches:
        prefix = f"{alias}\t{definition}\t"
        if sum(row.startswith(prefix) for row in rows) != 1:
            raise SystemExit(f"binding does not resolve to exactly one registry row: {alias}")
    return [(alias.encode("ascii"), definition.encode("ascii")) for alias, definition in matches]


def sidebar_policy_digest() -> bytes:
    payload = u32(SIDEBAR_WIDTH)
    for color in SIDEBAR_COLORS:
        payload += u32(color)
    payload += SIDEBAR_POLICY_TEXT
    for alias, definition in displayed_metric_bindings():
        payload += field(alias) + field(definition)
    return content_digest(b"policy", SIDEBAR_POLICY_SCHEMA, payload)


def composite_policy_digest() -> bytes:
    payload = renderer_profile_digest() + crosshair_policy_digest() + sidebar_policy_digest()
    if len(payload) != 96:
        raise ValueError(f"composite policy payload must be 96 bytes, found {len(payload)}")
    return content_digest(b"policy", COMPOSITE_POLICY_SCHEMA, payload)


# --- the three headless policies ---------------------------------------------------------


def genesis_policy_payload(
    world: bytes,
    history_genesis: bytes,
    camera_lanes: tuple[float, ...],
    simulation: tuple[int, int],
    output: tuple[int, int],
    simulation_width: int,
    tick: int,
    flags: tuple[int, ...],
) -> bytes:
    """Revision, world, history genesis, ten camera lanes, five dimensions, tick, six flags."""
    if len(camera_lanes) != 10:
        raise ValueError(f"camera declares ten lanes, found {len(camera_lanes)}")
    if len(flags) != 6:
        raise ValueError(f"genesis declares six boolean lanes, found {len(flags)}")
    if any(flag not in (0, 1) and index != 5 for index, flag in enumerate(flags)):
        raise ValueError("the five leading genesis flags are Booleans")
    payload = u16(POLICY_REVISION) + world + history_genesis
    for lane in camera_lanes:
        payload += f64(lane)
    payload += u32(simulation[0]) + u32(simulation[1])
    payload += u32(output[0]) + u32(output[1]) + u32(simulation_width)
    payload += u64(tick)
    payload += bytes(flags)
    if len(payload) != 180:
        raise ValueError(f"genesis policy payload must be 180 bytes, found {len(payload)}")
    return payload


def headless_genesis_policy_payload() -> bytes:
    """`App::for_headless_simulation` before its first tick.

    `World::new()` is one bedrock coordinate at the origin. `Camera::for_observer_spawn()` is
    `from_anchor(ZERO, 1.0, 0.0, 0.0, FRAC_PI_3)`: yaw wraps to exactly 0.0, pitch clamps to
    exactly 0.0, so the basis forward is (0.0, 0.0, 1.0) and the position is
    `anchor - forward * 1.0` = (0.0, 0.0, -1.0) with no negative zero in any lane.
    """
    world = world_digest((BEDROCK_COORD,))
    return genesis_policy_payload(
        world=world,
        history_genesis=history_genesis_digest(world),
        camera_lanes=(
            0.0,  # position x
            0.0,  # position y
            -1.0,  # position z
            0.0,  # yaw
            0.0,  # pitch
            0.0,  # anchor x
            0.0,  # anchor y
            0.0,  # anchor z
            1.0,  # observation distance
            FRAC_PI_3,  # vertical field of view
        ),
        simulation=(INITIAL_VIEWPORT, INITIAL_VIEWPORT),
        output=(INITIAL_OUTPUT_WIDTH, INITIAL_VIEWPORT),
        simulation_width=INITIAL_VIEWPORT,
        tick=0,
        flags=(
            1,  # sidebar visible
            1,  # needs render
            0,  # needs sidebar render
            0,  # needs present
            1,  # no queued edits
            DISPLAY_STATUS_AUDIT_DISABLED,
        ),
    )


def rules_policy_payload(
    renderer: bytes, crosshair: bytes, composite: bytes, tick_rate: float = TICK_RATE
) -> bytes:
    payload = (
        u16(POLICY_REVISION)
        + f64(tick_rate)
        + u128(TICK_DURATION_NANOS)
        + f64(DEFAULT_DOLLY_LOG_STEP)
        + f64(DEFAULT_MOUSE_SENSITIVITY)
        + f64(BLOCK_FILL_DISTANCE)
        + f64(OBSERVER_COLLISION_CLEARANCE)
        + renderer
        + crosshair
        + composite
    )
    if len(payload) != 154:
        raise ValueError(f"rules policy payload must be 154 bytes, found {len(payload)}")
    return payload


def headless_rules_policy_payload() -> bytes:
    return rules_policy_payload(
        renderer_profile_digest(), crosshair_policy_digest(), composite_policy_digest()
    )


def admission_policy_payload() -> bytes:
    """Revision, eight u64 count/byte limits, then eight u128 work limits."""
    payload = u16(POLICY_REVISION)
    for value in (
        MAX_INTERACTION_TAPE_PAYLOAD_BYTES,
        MAX_INTERACTION_TAPE_RECORDS,
        MAX_INTERACTION_ACTIONS_PER_RECORD,
        MAX_INTERACTION_TOTAL_ACTIONS,
        HEADLESS_SIMULATION_MAX_SOURCE_BYTES,
        HEADLESS_LOCAL_REPLAY_MAX_EXECUTABLE_BYTES,
        HEADLESS_SIMULATION_MAX_OUTPUT_PIXELS,
        HEADLESS_RENDER_TRIANGLES_PER_BIT_BOUND,
    ):
        payload += u64(value)
    for value in (
        HEADLESS_SIMULATION_MAX_RESIZE_PIXEL_WRITES,
        HEADLESS_SIMULATION_MAX_OUTPUT_PIXEL_TICKS,
        HEADLESS_SIMULATION_MAX_RASTER_SAMPLE_TESTS,
        HEADLESS_SIMULATION_MAX_WORLD_COORDINATE_TESTS,
        HEADLESS_SIMULATION_MAX_EDIT_COORDINATE_TESTS,
        HEADLESS_SIMULATION_MAX_CHUNK_WORD_TESTS,
        HEADLESS_SIMULATION_MAX_WORLD_ORDERING_UNITS,
        HEADLESS_SIMULATION_MAX_POSSIBLE_CHUNKS,
    ):
        payload += u128(value)
    if len(payload) != 194:
        raise ValueError(f"admission policy payload must be 194 bytes, found {len(payload)}")
    return payload


def policy_tag(schema: bytes, payload: bytes) -> str:
    return render(b"policy", content_digest(b"policy", schema, payload))


def main() -> None:
    renderer = renderer_profile_digest()
    crosshair = crosshair_policy_digest()
    composite = composite_policy_digest()
    print("upstream policies this replay names")
    print(f"  renderer profile   {render(b'policy', renderer)}")
    print(f"  crosshair overlay  {render(b'policy', crosshair)}")
    print(f"  composite          {render(b'policy', composite)}")

    genesis_bytes = headless_genesis_policy_payload()
    rules_bytes = headless_rules_policy_payload()
    admission_bytes = admission_policy_payload()

    genesis = policy_tag(GENESIS_POLICY_SCHEMA, genesis_bytes)
    rules = policy_tag(RULES_POLICY_SCHEMA, rules_bytes)
    admission = policy_tag(ADMISSION_POLICY_SCHEMA, admission_bytes)

    print("\nheadless local-replay policies")
    print(f"  genesis    payload {len(genesis_bytes)}  {genesis}")
    print(f"  rules      payload {len(rules_bytes)}  {rules}")
    print(f"  admission  payload {len(admission_bytes)}  {admission}")

    if len({genesis, rules, admission}) != 3:
        raise SystemExit("the three headless policies collided")

    for name, derived in (("genesis", genesis), ("rules", rules), ("admission", admission)):
        if derived != PINNED_IN_MAIN_RS[name]:
            raise SystemExit(
                f"{name}: this reconstruction derives {derived}, "
                f"but src/main.rs pins {PINNED_IN_MAIN_RS[name]}"
            )

    # What each schema exists to bind, varied one field at a time.

    # Genesis: the declared starting world. One extra coordinate is a different fold.
    two_block_world = world_digest((BEDROCK_COORD, (0, 1, 0)))
    moved_world = genesis_policy_payload(
        world=two_block_world,
        history_genesis=history_genesis_digest(two_block_world),
        camera_lanes=(0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, FRAC_PI_3),
        simulation=(INITIAL_VIEWPORT, INITIAL_VIEWPORT),
        output=(INITIAL_OUTPUT_WIDTH, INITIAL_VIEWPORT),
        simulation_width=INITIAL_VIEWPORT,
        tick=0,
        flags=(1, 1, 0, 0, 1, DISPLAY_STATUS_AUDIT_DISABLED),
    )
    if policy_tag(GENESIS_POLICY_SCHEMA, moved_world) == genesis:
        raise SystemExit("the initial world is not inside the genesis policy tag")

    # Genesis: the camera. One ulp of field of view -- exactly the `math.pi / 3` slip -- moves it.
    one_ulp = struct.unpack(">Q", f64(FRAC_PI_3))[0] - 1
    world = world_digest((BEDROCK_COORD,))
    moved_camera = genesis_policy_payload(
        world=world,
        history_genesis=history_genesis_digest(world),
        camera_lanes=(
            0.0,
            0.0,
            -1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            struct.unpack(">d", u64(one_ulp))[0],
        ),
        simulation=(INITIAL_VIEWPORT, INITIAL_VIEWPORT),
        output=(INITIAL_OUTPUT_WIDTH, INITIAL_VIEWPORT),
        simulation_width=INITIAL_VIEWPORT,
        tick=0,
        flags=(1, 1, 0, 0, 1, DISPLAY_STATUS_AUDIT_DISABLED),
    )
    if policy_tag(GENESIS_POLICY_SCHEMA, moved_camera) == genesis:
        raise SystemExit("the camera lanes are not inside the genesis policy tag")

    # Genesis: negative zero is a distinct lane value, and the encoding must preserve it.
    negative_zero = genesis_policy_payload(
        world=world,
        history_genesis=history_genesis_digest(world),
        camera_lanes=(-0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, FRAC_PI_3),
        simulation=(INITIAL_VIEWPORT, INITIAL_VIEWPORT),
        output=(INITIAL_OUTPUT_WIDTH, INITIAL_VIEWPORT),
        simulation_width=INITIAL_VIEWPORT,
        tick=0,
        flags=(1, 1, 0, 0, 1, DISPLAY_STATUS_AUDIT_DISABLED),
    )
    if policy_tag(GENESIS_POLICY_SCHEMA, negative_zero) == genesis:
        raise SystemExit("a negative-zero camera lane is indistinguishable from positive zero")

    # Genesis: one flipped boolean lane. The sidebar being hidden is a different starting state.
    hidden_sidebar = genesis_policy_payload(
        world=world,
        history_genesis=history_genesis_digest(world),
        camera_lanes=(0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, FRAC_PI_3),
        simulation=(INITIAL_VIEWPORT, INITIAL_VIEWPORT),
        output=(INITIAL_OUTPUT_WIDTH, INITIAL_VIEWPORT),
        simulation_width=INITIAL_VIEWPORT,
        tick=0,
        flags=(0, 1, 0, 0, 1, DISPLAY_STATUS_AUDIT_DISABLED),
    )
    if policy_tag(GENESIS_POLICY_SCHEMA, hidden_sidebar) == genesis:
        raise SystemExit("the boolean start lanes are not inside the genesis policy tag")

    # Rules: each named upstream policy, moved one at a time.
    other = bytearray(renderer)
    other[0] ^= 0x01
    for label, moved in (
        ("renderer", rules_policy_payload(bytes(other), crosshair, composite)),
        ("crosshair", rules_policy_payload(renderer, bytes(other), composite)),
        ("composite", rules_policy_payload(renderer, crosshair, bytes(other))),
        ("tick rate", headless_rules_policy_payload()[:2]
            + f64(59.0)
            + headless_rules_policy_payload()[10:]),
    ):
        if policy_tag(RULES_POLICY_SCHEMA, moved) == rules:
            raise SystemExit(f"the {label} field is not inside the rules policy tag")

    # Rules: the three digest slots are ordered, not a set. Swapping two must move the tag.
    swapped = rules_policy_payload(crosshair, renderer, composite)
    if policy_tag(RULES_POLICY_SCHEMA, swapped) == rules:
        raise SystemExit("the rules policy digest slots are unordered")

    # Admission: the eight work limits are u128 lanes, not u64. A u64 encoding halves the
    # payload's second half and must not agree.
    narrow = admission_bytes[:66] + b"".join(
        u64(value)
        for value in (
            HEADLESS_SIMULATION_MAX_RESIZE_PIXEL_WRITES,
            HEADLESS_SIMULATION_MAX_OUTPUT_PIXEL_TICKS,
            HEADLESS_SIMULATION_MAX_RASTER_SAMPLE_TESTS,
            HEADLESS_SIMULATION_MAX_WORLD_COORDINATE_TESTS,
            HEADLESS_SIMULATION_MAX_EDIT_COORDINATE_TESTS,
            HEADLESS_SIMULATION_MAX_CHUNK_WORD_TESTS,
            HEADLESS_SIMULATION_MAX_WORLD_ORDERING_UNITS,
            HEADLESS_SIMULATION_MAX_POSSIBLE_CHUNKS,
        )
    )
    if len(narrow) != 130 or policy_tag(ADMISSION_POLICY_SCHEMA, narrow) == admission:
        raise SystemExit("the admission work limits are not u128 lanes")

    # Admission: one raised limit is a different admission policy.
    raised = bytearray(admission_bytes)
    raised[9] ^= 0x01
    if policy_tag(ADMISSION_POLICY_SCHEMA, bytes(raised)) == admission:
        raise SystemExit("the declared limits are not inside the admission policy tag")

    # Domain separation across the three: the same payload under a sibling schema is not the
    # same object, so a genesis declaration can never be read as a rules declaration.
    for schema in (RULES_POLICY_SCHEMA, ADMISSION_POLICY_SCHEMA):
        if policy_tag(schema, genesis_bytes) == genesis:
            raise SystemExit("headless policy schemas are not domain-separated")

    print("\n3 policies, 3 upstream reconstructions, 11 controls")


if __name__ == "__main__":
    main()
