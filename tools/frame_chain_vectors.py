#!/usr/bin/env python3
"""Independent stdlib vectors for the Bitblock frame input and v1 frame receipt.

This file intentionally imports no Bitblock/Rust code. It reconstructs both canonical payloads
and independently applies the content-tag wrapper.

The pair is the implemented frame chain: a `FrameInput` commits to everything a renderer
invocation consumed, and a `FrameReceipt` v1 commits to one occurrence -- its parent, the tick it
came from, the input it consumed, and the exact framebuffer that was submitted. The vectors
compose rather than invent: the renderer profile and crosshair overlay input come from
`tools/render_policy_vectors.py`, and the committed framebuffer is a canonical frame under the
domain `tools/reference_renderer_vectors.py` and the Rust encoder already share.

What each object exists to bind is varied one field at a time. The camera is ten separate f64 bit
lanes, so a lane swap must move the tag; the receipt's parent is optional, and its absent form
pads with 32 zero bytes, so `None` and a genuinely all-zero parent differ only in one marker byte.

These are canonical-encoding vectors. A receipt commits a frame *identity*; whether that frame's
shape agrees with the viewport its input declared is a separate semantic-admission predicate, and
nothing here claims the named objects form an admissible set.
"""

from __future__ import annotations

import base64
import hashlib
import struct

TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
FRAME_INPUT_SCHEMA = b"bitblock/frame-input/v1"
FRAME_RECEIPT_SCHEMA = b"bitblock/frame-receipt/v1"
CANONICAL_FRAME_DOMAIN = b"bitblock/frame/xrgb8888-row-major/v1"

FRAME_INPUT_PAYLOAD_LEN = 184
FRAME_RECEIPT_PAYLOAD_LEN = 105

# Derived by tools/render_policy_vectors.py, which rebuilds both payloads from the declared
# constants. Naming them here keeps this suite standalone while pinning the two suites together:
# if the renderer profile or the crosshair overlay moved, the Rust literals below would not
# reproduce from these anchors.
RENDERER_PROFILE = "tsxgj54fonmrrlkoygcn5dtv65iu6neymvyee3x6hartgevwyynq"
CROSSHAIR_OVERLAY_INPUT = "3cjxsg2du7fhxwr6ihi7hno4sdwd7xbojoidnpvsrq5ubbaa7wfa"

# The exact camera the vectors capture, in the declared lane order: position xyz, yaw, pitch,
# anchor xyz, observation distance, vertical FOV. Lane 1 is negative zero, which shares every
# comparison with lane 0's positive zero and differs only in its bits.
CAMERA_LANES = (0.0, -0.0, 3.0, 0.25, -0.5, 6.0, 7.0, 8.0, 9.0, 1.0)
# The initial simulation viewport, so tools/composite_frame_input_vectors.py can name this exact
# input as the inner game boundary of a 1201 x 801 full-client composition.
VIEWPORT = (801, 801)
FRAME_WORDS = (0x00123456,)
FRAME_SIZE = (1, 1)

EXPECTED_FRAME = "#bb.frame.sha256.j7t5627svcsb3jhjgr65f7tfvhi6qbur2mcpt6bgptkrnaxozaca"
EXPECTED_FRAME_INPUT = "#bb.frame-input.sha256.oiwdnnaxuumavgducowfx7n4uc4gh4zoawgxj56zn5aaux2vzuvq"
EXPECTED_ROOT_RECEIPT = (
    "#bb.frame-receipt.sha256.b765k2pnxuxwlwxbbp4w6tn6fdw7fa676e2fpgg5jnhvcfo42yxq"
)
EXPECTED_LINKED_RECEIPT = (
    "#bb.frame-receipt.sha256.jiwqu7gg62732shlykypv4q3xzaisywfaijtgjs6a5yzfo2ur5ga"
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


def sequential_digest(start: int) -> bytes:
    """The 32 bytes start, start+1, ... wrapping at 256. A stand-in object identity."""
    return bytes((start + index) & 0xFF for index in range(32))


def encode_frame(width: int, height: int, words: tuple[int, ...]) -> bytes:
    if len(words) != width * height:
        raise ValueError("frame word count must be the dimension product")
    return (
        field(CANONICAL_FRAME_DOMAIN)
        + struct.pack(">IIQ", width, height, len(words))
        + b"".join(struct.pack(">I", word) for word in words)
    )


def frame_digest(width: int, height: int, words: tuple[int, ...]) -> bytes:
    return content_tag_digest(
        b"frame", CANONICAL_FRAME_DOMAIN, encode_frame(width, height, words)
    )


def encode_frame_input(
    world: bytes,
    camera: tuple[float, ...],
    width: int,
    height: int,
    renderer_profile: bytes,
    overlay_input: bytes,
) -> bytes:
    """world || ten f64 camera bit lanes || width || height || profile || overlay."""
    if len(camera) != 10:
        raise ValueError("the camera is exactly ten declared f64 lanes")
    for name, digest in (
        ("world", world),
        ("renderer profile", renderer_profile),
        ("overlay input", overlay_input),
    ):
        if len(digest) != 32:
            raise ValueError(f"{name} link must be a full digest")
    payload = (
        world
        + b"".join(struct.pack(">d", lane) for lane in camera)
        + struct.pack(">II", width, height)
        + renderer_profile
        + overlay_input
    )
    if len(payload) != FRAME_INPUT_PAYLOAD_LEN:
        raise ValueError(f"frame input payload is {len(payload)} bytes")
    return payload


def encode_frame_receipt(
    parent: bytes | None, source_tick: int, frame_input: bytes, frame: bytes
) -> bytes:
    """optional parent || u64_be(source_tick) || input || frame."""
    if not 0 <= source_tick < (1 << 64):
        raise ValueError("source tick must fit u64")
    if parent is None:
        payload = b"\x00" + bytes(32)
    else:
        if len(parent) != 32:
            raise ValueError("parent link must be a full digest")
        payload = b"\x01" + parent
    payload += struct.pack(">Q", source_tick) + frame_input + frame
    if len(payload) != FRAME_RECEIPT_PAYLOAD_LEN:
        raise ValueError(f"frame receipt payload is {len(payload)} bytes")
    return payload


def frame_input_digest(**changes: object) -> bytes:
    base: dict[str, object] = {
        "world": sequential_digest(0x00),
        "camera": CAMERA_LANES,
        "width": VIEWPORT[0],
        "height": VIEWPORT[1],
        "renderer_profile": decode(RENDERER_PROFILE),
        "overlay_input": decode(CROSSHAIR_OVERLAY_INPUT),
    }
    base.update(changes)
    return content_tag_digest(
        b"frame-input", FRAME_INPUT_SCHEMA, encode_frame_input(**base)  # type: ignore[arg-type]
    )


def frame_receipt_digest(
    parent: bytes | None, source_tick: int, frame_input: bytes, frame: bytes
) -> bytes:
    return content_tag_digest(
        b"frame-receipt",
        FRAME_RECEIPT_SCHEMA,
        encode_frame_receipt(parent, source_tick, frame_input, frame),
    )


def require_distinct(label: str, tags: list[tuple[str, str]]) -> None:
    seen: dict[str, str] = {}
    for name, value in tags:
        if value in seen:
            raise SystemExit(f"{label}: {name} collided with {seen[value]}")
        seen[value] = name
    print(f"  {len(tags)} distinct tags across {label}")


def main() -> None:
    frame = frame_digest(*FRAME_SIZE, FRAME_WORDS)
    frame_input = frame_input_digest()
    root = frame_receipt_digest(None, 4, frame_input, frame)
    linked = frame_receipt_digest(root, 9, frame_input, frame)

    print("frame chain")
    print(f"  frame              {render(b'frame', frame)}")
    print(f"  frame input        {render(b'frame-input', frame_input)}")
    print(f"  root receipt       {render(b'frame-receipt', root)}")
    print(f"  linked receipt     {render(b'frame-receipt', linked)}")

    for label, actual, expected in (
        ("frame", render(b"frame", frame), EXPECTED_FRAME),
        ("frame input", render(b"frame-input", frame_input), EXPECTED_FRAME_INPUT),
        ("root receipt", render(b"frame-receipt", root), EXPECTED_ROOT_RECEIPT),
        ("linked receipt", render(b"frame-receipt", linked), EXPECTED_LINKED_RECEIPT),
    ):
        if actual != expected:
            raise SystemExit(f"{label} tag moved: {actual}")

    input_variants = [("declared", render(b"frame-input", frame_input))]
    for index in range(len(CAMERA_LANES)):
        camera = list(CAMERA_LANES)
        bits = struct.unpack(">Q", struct.pack(">d", camera[index]))[0] ^ 1
        camera[index] = struct.unpack(">d", struct.pack(">Q", bits))[0]
        input_variants.append(
            (f"camera lane {index}", render(b"frame-input", frame_input_digest(camera=tuple(camera))))
        )
    # Lane 0 is +0.0 and lane 1 is -0.0. They compare equal as numbers, so a tag that separates
    # them is proof the lanes are hashed as bits rather than as values.
    swapped = list(CAMERA_LANES)
    swapped[0], swapped[1] = swapped[1], swapped[0]
    input_variants.append(
        ("signed zero swapped", render(b"frame-input", frame_input_digest(camera=tuple(swapped))))
    )
    input_variants += [
        ("other world", render(b"frame-input", frame_input_digest(world=sequential_digest(0x40)))),
        ("wider viewport", render(b"frame-input", frame_input_digest(width=VIEWPORT[0] + 1))),
        ("taller viewport", render(b"frame-input", frame_input_digest(height=VIEWPORT[1] + 1))),
        # The declared viewport is square, so a transposition control needs a non-square pair.
        ("landscape viewport", render(b"frame-input", frame_input_digest(width=801, height=603))),
        ("portrait viewport", render(b"frame-input", frame_input_digest(width=603, height=801))),
        (
            "other renderer profile",
            render(b"frame-input", frame_input_digest(renderer_profile=sequential_digest(0x60))),
        ),
        (
            "other overlay input",
            render(b"frame-input", frame_input_digest(overlay_input=sequential_digest(0x80))),
        ),
        # The renderer-profile and overlay-input lanes are both `policy` tags, so the Rust
        # constructor's kind check accepts either in either position. Only their order in the
        # payload separates them, which makes this swap the control that matters.
        (
            "profile and overlay swapped",
            render(
                b"frame-input",
                frame_input_digest(
                    renderer_profile=decode(CROSSHAIR_OVERLAY_INPUT),
                    overlay_input=decode(RENDERER_PROFILE),
                ),
            ),
        ),
    ]
    print("\nframe input")
    require_distinct("one changed frame-input field at a time", input_variants)

    other_frame = frame_digest(*FRAME_SIZE, (FRAME_WORDS[0] ^ 1,))
    receipt_variants = [
        ("root", render(b"frame-receipt", root)),
        ("linked", render(b"frame-receipt", linked)),
        ("root, later tick", render(b"frame-receipt", frame_receipt_digest(None, 5, frame_input, frame))),
        (
            "linked to an all-zero parent",
            render(b"frame-receipt", frame_receipt_digest(bytes(32), 4, frame_input, frame)),
        ),
        (
            "other input",
            render(
                b"frame-receipt",
                frame_receipt_digest(None, 4, frame_input_digest(width=VIEWPORT[0] + 1), frame),
            ),
        ),
        (
            "one flipped pixel bit",
            render(b"frame-receipt", frame_receipt_digest(None, 4, frame_input, other_frame)),
        ),
    ]
    print("\nframe receipt v1")
    require_distinct("parent, tick, input and frame varied independently", receipt_variants)
    absent = encode_frame_receipt(None, 4, frame_input, frame)
    zero_parent = encode_frame_receipt(bytes(32), 4, frame_input, frame)
    if absent[1:] != zero_parent[1:]:
        raise SystemExit("absent parent padding is not 32 zero bytes")
    print("  an absent parent and an all-zero parent differ only in the present marker")


if __name__ == "__main__":
    main()
