#!/usr/bin/env python3
"""Independent stdlib vectors for Bitblock's renderer profile, crosshair policy and overlay input.

This file intentionally imports no Bitblock/Rust code. It reconstructs each canonical payload
from the declared constants and independently applies the content-tag wrapper.

These three objects are one chain. The renderer profile names every constant and ordering rule a
frame's pixels may depend on; the crosshair policy names the overlay geometry; the overlay input
is the policy-plus-optional-payload object a `FrameInput` actually binds. All three hash under
tag kind `policy` -- the schema, not the kind, is what separates them.

Three of the twelve declared palette slots hold the same word (`0x00ffffff`), so equality of a
colour proves nothing about position. The vectors below therefore move one slot at a time and
require the tag to move each time.
"""

from __future__ import annotations

import base64
import hashlib
import struct

TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
RENDERER_PROFILE_SCHEMA = b"bitblock/renderer-profile/host-f64/v1"
CROSSHAIR_POLICY_SCHEMA = b"bitblock/overlay-policy/crosshair/v1"
OVERLAY_INPUT_SCHEMA = b"bitblock/overlay-input/v1"

RENDERER_PROFILE_PAYLOAD_LEN = 106
CROSSHAIR_POLICY_PAYLOAD_LEN = 16
OVERLAY_INPUT_PAYLOAD_LEN = 65

# crates/core/src/render.rs, in declaration order.
PROFILE_REVISIONS = (1, 1, 1, 1)
PROFILE_ARITHMETIC = 0  # current host Rust f64 arithmetic
PROFILE_DEPTH_RULE = 0  # total_cmp depth; equal depth chooses lower RGB
PROFILE_SCALARS = (
    1.0e-7,  # NEAR_PLANE
    -1.0e-10,  # BARYCENTRIC_EPSILON
    1.0e-9,  # ONE_PIXEL_EPSILON, from crates/core/src/camera.rs
    1.0,  # SMALL_BIT_EXTENT
    0.5,  # PIXEL_CENTER_OFFSET
    2.0**-52,  # f64::EPSILON
)
PROFILE_COLORS = (
    0x00FFFFFF,  # RGB_MASK
    0x00000000,  # BACKGROUND
    0x00FFFFFF,  # AGGREGATE_BIT
    0x00FF9F1C,  # BEDROCK_PIXEL
    0x00FFC15A,  # BEDROCK_TOP
    0x00C86F0A,  # BEDROCK_SIDE
    0x00E8890C,  # BEDROCK_FRONT
    0x00A95508,  # BEDROCK_BACK
    0x00FFFFFF,  # ORDINARY_TOP
    0x00BDBDBD,  # ORDINARY_SIDE
    0x00D8D8D8,  # ORDINARY_BACK
    0x00989898,  # ORDINARY_FRONT
)
CROSSHAIR_REVISION = 1
CROSSHAIR_CENTER_RULE = 0  # integer-divided physical-pixel center
CROSSHAIR_GAP_RADIUS = 2
CROSSHAIR_ARM_LENGTH = 3
CROSSHAIR_COLOR = 0x00FFFFFF
CROSSHAIR_DEPTH_RULE = 0  # the overlay never writes depth

# The three tags this suite must keep reproducing. Their Rust counterparts are asserted in
# crates/core/src/render.rs; if these two sides ever disagree, exactly one of them moved.
EXPECTED_RENDERER_PROFILE = (
    "#bb.policy.sha256.tsxgj54fonmrrlkoygcn5dtv65iu6neymvyee3x6hartgevwyynq"
)
EXPECTED_CROSSHAIR_POLICY = (
    "#bb.policy.sha256.pyj2osdlm4onqqyrhhlvy2qjfxxkfpfsg7flmjxeodoho55i4hta"
)
EXPECTED_CROSSHAIR_OVERLAY_INPUT = (
    "#bb.policy.sha256.3cjxsg2du7fhxwr6ihi7hno4sdwd7xbojoidnpvsrq5ubbaa7wfa"
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


def encode_renderer_profile(
    revisions: tuple[int, ...] = PROFILE_REVISIONS,
    arithmetic: int = PROFILE_ARITHMETIC,
    depth_rule: int = PROFILE_DEPTH_RULE,
    scalars: tuple[float, ...] = PROFILE_SCALARS,
    colors: tuple[int, ...] = PROFILE_COLORS,
) -> bytes:
    if len(revisions) != 4 or len(scalars) != 6 or len(colors) != 12:
        raise ValueError("renderer profile has four revisions, six scalars and twelve colours")
    payload = b"".join(struct.pack(">H", revision) for revision in revisions)
    payload += bytes([arithmetic, depth_rule])
    payload += b"".join(struct.pack(">d", scalar) for scalar in scalars)
    payload += b"".join(struct.pack(">I", color) for color in colors)
    if len(payload) != RENDERER_PROFILE_PAYLOAD_LEN:
        raise ValueError(f"renderer profile payload is {len(payload)} bytes")
    return payload


def encode_crosshair_policy(
    revision: int = CROSSHAIR_REVISION,
    center_rule: int = CROSSHAIR_CENTER_RULE,
    gap_radius: int = CROSSHAIR_GAP_RADIUS,
    arm_length: int = CROSSHAIR_ARM_LENGTH,
    color: int = CROSSHAIR_COLOR,
    depth_rule: int = CROSSHAIR_DEPTH_RULE,
) -> bytes:
    payload = (
        struct.pack(">H", revision)
        + bytes([center_rule])
        + struct.pack(">III", gap_radius, arm_length, color)
        + bytes([depth_rule])
    )
    if len(payload) != CROSSHAIR_POLICY_PAYLOAD_LEN:
        raise ValueError(f"crosshair policy payload is {len(payload)} bytes")
    return payload


def encode_overlay_input(policy: bytes, payload: bytes | None) -> bytes:
    """policy digest || present marker || payload digest, zero-padded when absent."""
    if len(policy) != 32:
        raise ValueError("overlay policy link must be a full digest")
    if payload is None:
        body = policy + b"\x00" + bytes(32)
    else:
        if len(payload) != 32:
            raise ValueError("overlay payload link must be a full digest")
        body = policy + b"\x01" + payload
    if len(body) != OVERLAY_INPUT_PAYLOAD_LEN:
        raise ValueError(f"overlay input payload is {len(body)} bytes")
    return body


def profile_digest(**changes: object) -> bytes:
    return content_tag_digest(
        b"policy", RENDERER_PROFILE_SCHEMA, encode_renderer_profile(**changes)
    )


def crosshair_digest(**changes: int) -> bytes:
    return content_tag_digest(
        b"policy", CROSSHAIR_POLICY_SCHEMA, encode_crosshair_policy(**changes)
    )


def overlay_digest(policy: bytes, payload: bytes | None) -> bytes:
    return content_tag_digest(
        b"policy", OVERLAY_INPUT_SCHEMA, encode_overlay_input(policy, payload)
    )


def require_distinct(label: str, tags: list[tuple[str, str]]) -> None:
    seen: dict[str, str] = {}
    for name, value in tags:
        if value in seen:
            raise SystemExit(f"{label}: {name} collided with {seen[value]}")
        seen[value] = name
    print(f"  {len(tags)} distinct tags across {label}")


def main() -> None:
    profile = profile_digest()
    crosshair = crosshair_digest()
    overlay = overlay_digest(crosshair, None)

    print("declared renderer and overlay identities")
    print(f"  renderer profile   {render(b'policy', profile)}")
    print(f"  crosshair policy   {render(b'policy', crosshair)}")
    print(f"  crosshair overlay  {render(b'policy', overlay)}")

    for label, actual, expected in (
        ("renderer profile", render(b"policy", profile), EXPECTED_RENDERER_PROFILE),
        ("crosshair policy", render(b"policy", crosshair), EXPECTED_CROSSHAIR_POLICY),
        ("crosshair overlay", render(b"policy", overlay), EXPECTED_CROSSHAIR_OVERLAY_INPUT),
    ):
        if actual != expected:
            raise SystemExit(f"{label} tag moved: {actual}")

    # Every renderer-profile lane is separately bound. ORDINARY_TOP (slot 8) carries the same
    # word as RGB_MASK (slot 0) and AGGREGATE_BIT (slot 2), so only position separates them.
    profile_variants = [("declared", render(b"policy", profile))]
    for index in range(len(PROFILE_REVISIONS)):
        revisions = list(PROFILE_REVISIONS)
        revisions[index] += 1
        profile_variants.append(
            (f"revision lane {index}", render(b"policy", profile_digest(revisions=tuple(revisions))))
        )
    for index in range(len(PROFILE_SCALARS)):
        scalars = list(PROFILE_SCALARS)
        scalars[index] = -scalars[index]
        profile_variants.append(
            (f"scalar lane {index}", render(b"policy", profile_digest(scalars=tuple(scalars))))
        )
    for index in range(len(PROFILE_COLORS)):
        colors = list(PROFILE_COLORS)
        colors[index] ^= 0x01
        profile_variants.append(
            (f"colour slot {index}", render(b"policy", profile_digest(colors=tuple(colors))))
        )
    profile_variants.append(
        ("arithmetic rule", render(b"policy", profile_digest(arithmetic=1)))
    )
    profile_variants.append(("depth rule", render(b"policy", profile_digest(depth_rule=1))))
    print("\nrenderer profile")
    require_distinct("one changed renderer-profile lane at a time", profile_variants)

    # Gap radius and arm length are adjacent u32 lanes carrying different values, so swapping
    # them is the mutation a positional encoding must notice.
    crosshair_variants = [
        ("declared", render(b"policy", crosshair)),
        ("wider gap", render(b"policy", crosshair_digest(gap_radius=CROSSHAIR_GAP_RADIUS + 1))),
        ("longer arm", render(b"policy", crosshair_digest(arm_length=CROSSHAIR_ARM_LENGTH + 1))),
        (
            "gap and arm swapped",
            render(
                b"policy",
                crosshair_digest(
                    gap_radius=CROSSHAIR_ARM_LENGTH, arm_length=CROSSHAIR_GAP_RADIUS
                ),
            ),
        ),
        ("other colour", render(b"policy", crosshair_digest(color=CROSSHAIR_COLOR ^ 0x01))),
        ("other centre rule", render(b"policy", crosshair_digest(center_rule=1))),
        ("overlay writes depth", render(b"policy", crosshair_digest(depth_rule=1))),
        ("next revision", render(b"policy", crosshair_digest(revision=CROSSHAIR_REVISION + 1))),
    ]
    print("\ncrosshair policy")
    require_distinct("one changed crosshair lane at a time", crosshair_variants)

    # An absent payload pads with 32 zero bytes, so `None` and `Some(all-zero digest)` differ
    # only in the marker byte. If the marker were dropped they would alias.
    zero_payload = overlay_digest(crosshair, bytes(32))
    overlay_variants = [
        ("crosshair, no payload", render(b"policy", overlay)),
        ("crosshair, zero payload", render(b"policy", zero_payload)),
        (
            "crosshair, real payload",
            render(b"policy", overlay_digest(crosshair, bytes(range(32)))),
        ),
        ("other policy, no payload", render(b"policy", overlay_digest(profile, None))),
    ]
    print("\noverlay input")
    require_distinct("policy and payload varied independently", overlay_variants)
    if encode_overlay_input(crosshair, None)[33:] != encode_overlay_input(crosshair, bytes(32))[33:]:
        raise SystemExit("absent overlay padding is not 32 zero bytes")
    print("  absent and zero payloads differ only in the present marker")


if __name__ == "__main__":
    main()
