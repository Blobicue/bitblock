#!/usr/bin/env python3
"""Independent canonical vectors for the physical-sensor observation codec.

This stdlib-only script reconstructs the v1 policy, declared-profile, ordered
challenge, and observation bytes without importing or launching Bitblock.
It validates content-tag framing and the neutral/report states only. It does
not possess an external instrument, resolve a presentation artifact, compare a
real sensor sample, establish calibration, or make any scanout/light claim.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import struct


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
POLICY_KIND = b"policy"
OBSERVATION_KIND = b"physical-sensor-observation"
POLICY_SCHEMA = b"bitblock/physical-sensor-policy/v1"
PROFILE_SCHEMA = b"bitblock/declared-sensor-profile/v1"
CHALLENGE_SCHEMA = b"bitblock/sensor-rgb-challenge/v1"
OBSERVATION_SCHEMA = b"bitblock/physical-sensor-observation/v1"

NOT_APPLICABLE = 0
REPORTED_SAMPLES = 2
COLORIMETER = 2
XRGB8_EXPANDED_RGB16 = 1

# Filled from the independently encoded payloads below.  They intentionally
# bind no hardware fact: these are canonical-codec known-answer tests only.
EXPECTED = {
    "policy": "#bb.policy.sha256.o5lo24retmb5tnqnsjxh2defm2za6txgcntog5tpzxeannjzd4dq",
    "not-applicable": "#bb.physical-sensor-observation.sha256.ojrmsd5622e42zrvqwlcelaia5kzrvl3v4b6yglg2s27dmzm4uvq",
    "profile": "#bb.policy.sha256.wkxyvnyqua22wdm6cbv4nkt3lttnazsbwrq4jse5zrgnyynqob5q",
    "challenge": "#bb.policy.sha256.5a5ldpfnswugam6677eqhtmu7a34cawh5fhh52chadkudd5apnpq",
    "reported-samples": "#bb.physical-sensor-observation.sha256.cbwsp5lz3nl4ux3spzakh57ag46takzn4m6yv42tjdmtl45nv2ta",
}


def field(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def content_tag(kind: bytes, schema: bytes, payload: bytes) -> bytes:
    preimage = (
        field(TAG_DOMAIN)
        + field(kind)
        + field(HASH_SUITE)
        + field(schema)
        + struct.pack(">Q", len(payload))
        + payload
    )
    return hashlib.sha256(preimage).digest()


def text(kind: bytes, digest: bytes) -> str:
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.{kind.decode('ascii')}.sha256.{encoded}"


def profile_payload() -> bytes:
    return bytes((COLORIMETER, XRGB8_EXPANDED_RGB16)) + struct.pack(">HHH", 0, 2, 4)


def challenge_points() -> tuple[tuple[int, int, int], ...]:
    return (
        (0, 0, 0x0011_2233),
        (1, 0, 0x0044_5566),
    )


def challenge_payload(points: tuple[tuple[int, int, int], ...]) -> bytes:
    if not points:
        raise ValueError("challenge must not be empty")
    if tuple(sorted((y, x) for x, y, _ in points)) != tuple((y, x) for x, y, _ in points):
        raise ValueError("challenge must be in strictly row-major order")
    if len({(x, y) for x, y, _ in points}) != len(points):
        raise ValueError("challenge coordinates must be unique")
    if any(word & 0xFF00_0000 for _, _, word in points):
        raise ValueError("challenge words must be XRGB")
    return struct.pack(">H", len(points)) + b"".join(
        struct.pack(">III", x, y, word) for x, y, word in points
    )


def observation_payload(
    state: int,
    policy_digest: bytes,
    parent_digest: bytes | None,
    profile: bytes | None,
    points: tuple[tuple[int, int, int], ...] | None,
    samples: tuple[tuple[int, int, int], ...],
    raw: bytes,
) -> bytes:
    if len(policy_digest) != 32:
        raise ValueError("policy must be a SHA-256 digest")
    payload = bytes((state,)) + policy_digest
    payload += b"\x00" if parent_digest is None else b"\x01" + parent_digest
    payload += b"\x00" if profile is None else b"\x01" + profile
    payload += b"\x00" if points is None else b"\x01" + challenge_payload(points)
    payload += struct.pack(">H", len(samples))
    payload += b"".join(struct.pack(">HHH", *sample) for sample in samples)
    payload += struct.pack(">I", len(raw)) + raw
    return payload


def vector_tags() -> dict[str, str]:
    policy = content_tag(POLICY_KIND, POLICY_SCHEMA, b"")
    profile = profile_payload()
    points = challenge_points()
    not_applicable = observation_payload(
        NOT_APPLICABLE, policy, None, None, None, (), b""
    )
    reported = observation_payload(
        REPORTED_SAMPLES,
        policy,
        hashlib.sha256(b"bitblock/physical-sensor-vector-parent/v1").digest(),
        profile,
        points,
        ((0x1111, 0x2222, 0x3333), (0x4444, 0x5555, 0x6666)),
        b"vector-raw",
    )
    return {
        "policy": text(POLICY_KIND, policy),
        "not-applicable": text(
            OBSERVATION_KIND,
            content_tag(OBSERVATION_KIND, OBSERVATION_SCHEMA, not_applicable),
        ),
        "profile": text(POLICY_KIND, content_tag(POLICY_KIND, PROFILE_SCHEMA, profile)),
        "challenge": text(
            POLICY_KIND,
            content_tag(POLICY_KIND, CHALLENGE_SCHEMA, challenge_payload(points)),
        ),
        "reported-samples": text(
            OBSERVATION_KIND,
            content_tag(OBSERVATION_KIND, OBSERVATION_SCHEMA, reported),
        ),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--emit", action="store_true")
    args = parser.parse_args()
    actual = vector_tags()
    if args.emit:
        for name, value in actual.items():
            print(f"{name}\t{value}")
        return 0
    if any(not value for value in EXPECTED.values()):
        raise AssertionError("frozen expected physical-sensor vectors are not populated")
    if actual != EXPECTED:
        raise AssertionError(f"physical-sensor vector mismatch:\nactual={actual}\nexpected={EXPECTED}")
    print(f"physical-sensor-vectors\tPASS\tcases={len(actual)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
