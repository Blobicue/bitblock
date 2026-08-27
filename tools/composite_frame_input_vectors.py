#!/usr/bin/env python3
"""Independent stdlib vectors for the Bitblock sidebar composite frame input.

This file intentionally imports no Bitblock/Rust code. It reconstructs the canonical 112-byte
payload and independently applies the content-tag wrapper.

The composite input is the outer frame boundary the sidebar adds. It commits to the inner game
`FrameInput`, the exact inner game `Frame` that was rendered from it, the composition overlay
input carrying that tick's metric snapshot, and both the simulation and the output dimensions.
Committing the inner frame *and* its input is what lets the compositor reuse a cached left region
only when the cached tag is the selected one, and committing both dimension pairs is what keeps
the simulation renderer from ever seeing the sidebar width.

It hashes under tag kind `frame-input`, the same kind as the inner game input it contains. Only
the schema separates the two, so a bare tag cannot say which of the two frame boundaries it names.

The vectors compose rather than invent. Every nested digest is one an existing suite already
derives independently:

    game input      tools/frame_chain_vectors.py
    game frame      tools/frame_chain_vectors.py
    composite policy    tools/sidebar_vectors.py
    metric snapshot     tools/sidebar_snapshot_vectors.py
"""

from __future__ import annotations

import base64
import hashlib
import struct

TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
COMPOSITE_INPUT_SCHEMA = b"bitblock/sidebar/composite-frame-input/v1"
OVERLAY_INPUT_SCHEMA = b"bitblock/overlay-input/v1"

COMPOSITE_INPUT_PAYLOAD_LEN = 112
OVERLAY_INPUT_PAYLOAD_LEN = 65

# Anchors, each derived from its own payload by the suite named above.
GAME_INPUT = "oiwdnnaxuumavgducowfx7n4uc4gh4zoawgxj56zn5aaux2vzuvq"
GAME_FRAME = "j7t5627svcsb3jhjgr65f7tfvhi6qbur2mcpt6bgptkrnaxozaca"
COMPOSITE_POLICY = "kyd57oandtrj4iuis54zozpv6pwgmcoy4hmsvlkp2fqvhy6noe5q"
METRIC_SNAPSHOT = "ayjgabj4ttvzgagldztg4ceaxtob6oob4icpxuoha5r3xvb44tga"

# The initial full-client geometry: an 801 x 801 simulation beside a 400-wide sidebar.
SIMULATION = (801, 801)
OUTPUT = (1201, 801)

EXPECTED_OVERLAY_INPUT = (
    "#bb.policy.sha256.mzcw7fxkb3owzo4ofsu5dxdecdphjgwiqfmxehkx6sjirjvhmoba"
)
EXPECTED_COMPOSITE_INPUT = (
    "#bb.frame-input.sha256.xt5sklsl47sta5tf5oespjwu3vmtcc5llli4yls23icku7gdlkdq"
)


def field(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def content_tag_digest(kind: bytes, schema: bytes, payload: bytes) -> bytes:
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


def decode(encoded: str) -> bytes:
    return base64.b32decode(encoded.upper() + "=" * ((8 - len(encoded) % 8) % 8))


def overlay_input_digest(policy: bytes, payload: bytes) -> bytes:
    """The composition overlay always carries a payload: the snapshot the sidebar drew."""
    body = policy + b"\x01" + payload
    if len(body) != OVERLAY_INPUT_PAYLOAD_LEN:
        raise ValueError(f"overlay input payload is {len(body)} bytes")
    return content_tag_digest(b"policy", OVERLAY_INPUT_SCHEMA, body)


def encode_composite_input(
    game_input: bytes,
    game_frame: bytes,
    overlay_input: bytes,
    simulation_width: int,
    simulation_height: int,
    output_width: int,
    output_height: int,
) -> bytes:
    for name, digest in (
        ("game input", game_input),
        ("game frame", game_frame),
        ("overlay input", overlay_input),
    ):
        if len(digest) != 32:
            raise ValueError(f"{name} link must be a full digest")
    payload = (
        game_input
        + game_frame
        + overlay_input
        + struct.pack(
            ">IIII", simulation_width, simulation_height, output_width, output_height
        )
    )
    if len(payload) != COMPOSITE_INPUT_PAYLOAD_LEN:
        raise ValueError(f"composite frame input payload is {len(payload)} bytes")
    return payload


def composite_digest(**changes: object) -> bytes:
    base: dict[str, object] = {
        "game_input": decode(GAME_INPUT),
        "game_frame": decode(GAME_FRAME),
        "overlay_input": overlay_input_digest(
            decode(COMPOSITE_POLICY), decode(METRIC_SNAPSHOT)
        ),
        "simulation_width": SIMULATION[0],
        "simulation_height": SIMULATION[1],
        "output_width": OUTPUT[0],
        "output_height": OUTPUT[1],
    }
    base.update(changes)
    return content_tag_digest(
        b"frame-input",
        COMPOSITE_INPUT_SCHEMA,
        encode_composite_input(**base),  # type: ignore[arg-type]
    )


def require_distinct(label: str, tags: list[tuple[str, str]]) -> None:
    seen: dict[str, str] = {}
    for name, value in tags:
        if value in seen:
            raise SystemExit(f"{label}: {name} collided with {seen[value]}")
        seen[value] = name
    print(f"  {len(tags)} distinct tags across {label}")


def main() -> None:
    overlay = overlay_input_digest(decode(COMPOSITE_POLICY), decode(METRIC_SNAPSHOT))
    composite = composite_digest()

    print("sidebar composite frame input")
    print(f"  composition overlay  {render(b'policy', overlay)}")
    print(f"  composite input      {render(b'frame-input', composite)}")

    for label, actual, expected in (
        ("composition overlay", render(b"policy", overlay), EXPECTED_OVERLAY_INPUT),
        ("composite input", render(b"frame-input", composite), EXPECTED_COMPOSITE_INPUT),
    ):
        if actual != expected:
            raise SystemExit(f"{label} tag moved: {actual}")

    variants = [
        ("declared", render(b"frame-input", composite)),
        (
            "other game input",
            render(b"frame-input", composite_digest(game_input=bytes(range(32)))),
        ),
        (
            "other game frame",
            render(b"frame-input", composite_digest(game_frame=bytes(range(32)))),
        ),
        (
            "other metric snapshot",
            render(
                b"frame-input",
                composite_digest(
                    overlay_input=overlay_input_digest(
                        decode(COMPOSITE_POLICY), bytes(range(32))
                    )
                ),
            ),
        ),
        (
            "other composition policy",
            render(
                b"frame-input",
                composite_digest(
                    overlay_input=overlay_input_digest(
                        bytes(range(32)), decode(METRIC_SNAPSHOT)
                    )
                ),
            ),
        ),
        (
            "wider simulation",
            render(b"frame-input", composite_digest(simulation_width=SIMULATION[0] + 1)),
        ),
        (
            "taller simulation",
            render(b"frame-input", composite_digest(simulation_height=SIMULATION[1] + 1)),
        ),
        ("wider output", render(b"frame-input", composite_digest(output_width=OUTPUT[0] + 1))),
        ("taller output", render(b"frame-input", composite_digest(output_height=OUTPUT[1] + 1))),
    ]
    print()
    require_distinct("one changed composite-input field at a time", variants)

    # The sidebar width is not a stored field: it is exactly the gap between the two committed
    # widths. Hiding the sidebar therefore changes the output width and moves the tag, and the
    # inner game input and frame it names are untouched by that change.
    hidden = composite_digest(output_width=SIMULATION[0])
    if render(b"frame-input", hidden) == render(b"frame-input", composite):
        raise SystemExit("output width is not bound by the composite input tag")
    print(f"  sidebar hidden       {render(b'frame-input', hidden)}")
    print(f"  sidebar width        {OUTPUT[0] - SIMULATION[0]} derived, never stored")


if __name__ == "__main__":
    main()
