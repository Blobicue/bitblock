#!/usr/bin/env python3
"""Independent stdlib KAT for Bitblock's Windows composition timing envelope.

This program imports no Bitblock implementation code. It reconstructs both policy
payloads, the nested FrameReceiptV2 payload, the complete DWM_TIMING_INFO field
sequence, the fixed envelope layout, and the content-tag framing from constants.
"""

from __future__ import annotations

import base64
import hashlib
import struct


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"

WINDOWS_PRESENT_POLICY_SCHEMA = (
    b"bitblock/windows-present-policy/set-dibits-to-device/v1"
)
TIMING_POLICY_SCHEMA = (
    b"bitblock/composition-timing-policy/windows-dwm-null-hwnd/v1"
)
ENVELOPE_SCHEMA = b"bitblock/composition-timing-envelope/v1"

FRAME_RECEIPT_V2_LEN = 130
PRESENTER_FACTS_LEN = 52
QPC_READ_LEN = 13
QPC_BRACKET_LEN = 26
QPC_FREQUENCY_LEN = 13
DWM_SNAPSHOT_LEN = 288
DWM_TIMING_CALL_LEN = 319
GDI_FLUSH_CALL_LEN = 27
DWM_FLUSH_CALL_LEN = 30
ENVELOPE_LEN = 948

EXPECTED_WINDOWS_PRESENT_POLICY = (
    "#bb.policy.sha256.opgpybj32tw72pep2xusit4lzzgsiozs5f5reppq67iiejueetvq"
)
EXPECTED_TIMING_POLICY = (
    "#bb.policy.sha256.t2zuewerksnmvhhjyaaplmlmfb2poptubqxpc6czwvmuwufpanrq"
)
EXPECTED_ENVELOPE = (
    "#bb.presentation-artifact.sha256.sw2i52veohfmc3lqe74tzd72n6ppwxxshejxmazclwikgniqx2sq"
)


def framed(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def tag_digest(kind: bytes, schema: bytes, payload: bytes) -> bytes:
    preimage = (
        framed(TAG_DOMAIN)
        + framed(kind)
        + framed(HASH_SUITE)
        + framed(schema)
        + struct.pack(">Q", len(payload))
        + payload
    )
    return hashlib.sha256(preimage).digest()


def content_tag(kind: bytes, schema: bytes, payload: bytes) -> str:
    encoded = (
        base64.b32encode(tag_digest(kind, schema, payload))
        .decode("ascii")
        .lower()
        .rstrip("=")
    )
    return f"#bb.{kind.decode('ascii')}.sha256.{encoded}"


def windows_present_policy_payload() -> bytes:
    payload = (
        struct.pack(">H", 1)  # policy revision
        + bytes([0])  # SetDIBitsToDevice into a window DC
        + struct.pack(">I", 0)  # BI_RGB
        + struct.pack(">H", 32)  # 32-bit XRGB pixels
        + bytes([1])  # top-down DIB
        + struct.pack(">I", 0)  # DIB_RGB_COLORS
        + bytes([1, 1, 1])  # full rows; per-call DC; retain release result
    )
    assert len(payload) == 17
    return payload


def timing_policy_payload() -> bytes:
    payload = struct.pack(">H", 1) + bytes(
        [
            0,  # Windows desktop-composition session
            1,  # DwmGetCompositionTimingInfo uses the required null HWND
            1,  # every observation is QPC bracketed
            1,  # retain checked-presenter facts
            1,  # retain GdiFlush result
            1,  # retain DwmFlush HRESULT
            1,  # retain the complete DWM timing structure
            1,  # retain before and after timing calls
            1,  # progress assessment is derived, never encoded
            1,  # frame attribution remains Unknown
            0,  # no scanout claim
            0,  # no physical-light claim
        ]
    )
    assert len(payload) == 14
    return payload


def sequence(start: int) -> bytes:
    return bytes((start + index) & 0xFF for index in range(32))


def frame_receipt_v2_payload() -> bytes:
    payload = (
        bytes([0])  # absent parent
        + bytes(32)  # canonical absent-parent padding
        + bytes([1])  # Composite stream
        + sequence(0x10)  # TickOutcome digest
        + sequence(0x40)  # FrameInput digest
        + sequence(0x70)  # final Frame digest
    )
    assert len(payload) == FRAME_RECEIPT_V2_LEN
    assert payload[0] == 0 and payload[1:33] == bytes(32)
    assert payload[33] == 1
    return payload


# The exact DWM_TIMING_INFO order after omitting cbSize. "I" is a big-endian
# u32 lane and "Q" is a big-endian u64 lane. QPC lanes are never signed.
DWM_SNAPSHOT_LAYOUT = (
    ("rate_refresh_numerator", "I"),
    ("rate_refresh_denominator", "I"),
    ("qpc_refresh_period", "Q"),
    ("rate_compose_numerator", "I"),
    ("rate_compose_denominator", "I"),
    ("qpc_vblank", "Q"),
    ("c_refresh", "Q"),
    ("c_dx_refresh", "I"),
    ("qpc_compose", "Q"),
    ("c_frame", "Q"),
    ("c_dx_present", "I"),
    ("c_refresh_frame", "Q"),
    ("c_frame_submitted", "Q"),
    ("c_dx_present_submitted", "I"),
    ("c_frame_confirmed", "Q"),
    ("c_dx_present_confirmed", "I"),
    ("c_refresh_confirmed", "Q"),
    ("c_dx_refresh_confirmed", "I"),
    ("c_frames_late", "Q"),
    ("c_frames_outstanding", "I"),
    ("c_frame_displayed", "Q"),
    ("qpc_frame_displayed", "Q"),
    ("c_refresh_frame_displayed", "Q"),
    ("c_frame_complete", "Q"),
    ("qpc_frame_complete", "Q"),
    ("c_frame_pending", "Q"),
    ("qpc_frame_pending", "Q"),
    ("c_frames_displayed", "Q"),
    ("c_frames_complete", "Q"),
    ("c_frames_pending", "Q"),
    ("c_frames_available", "Q"),
    ("c_frames_dropped", "Q"),
    ("c_frames_missed", "Q"),
    ("c_refresh_next_displayed", "Q"),
    ("c_refresh_next_presented", "Q"),
    ("c_refreshes_displayed", "Q"),
    ("c_refreshes_presented", "Q"),
    ("c_refresh_started", "Q"),
    ("c_pixels_received", "Q"),
    ("c_pixels_drawn", "Q"),
    ("c_buffers_empty", "Q"),
)


def snapshot_fixture(epoch: int) -> dict[str, int]:
    values: dict[str, int] = {}
    for index, (name, lane) in enumerate(DWM_SNAPSHOT_LAYOUT):
        if name in ("rate_refresh_numerator", "rate_compose_numerator"):
            values[name] = 60
        elif name in ("rate_refresh_denominator", "rate_compose_denominator"):
            values[name] = 1
        elif name == "qpc_refresh_period":
            values[name] = 166_667
        elif name.startswith("qpc_"):
            values[name] = 900_000 + epoch * 10_000 + index * 17
        elif lane == "I":
            values[name] = 1_000 + epoch * 100 + index
        else:
            values[name] = 5_000 + epoch * 100 + index
    return values


def encode_snapshot(values: dict[str, int]) -> bytes:
    assert set(values) == {name for name, _ in DWM_SNAPSHOT_LAYOUT}
    payload = b"".join(
        struct.pack(">" + lane, values[name])
        for name, lane in DWM_SNAPSHOT_LAYOUT
    )
    assert struct.calcsize(">" + "".join(lane for _, lane in DWM_SNAPSHOT_LAYOUT)) == 288
    assert len(payload) == DWM_SNAPSHOT_LEN
    return payload


def qpc_read(value: int) -> bytes:
    payload = struct.pack(">BQI", 1, value, 0)
    assert len(payload) == QPC_READ_LEN
    return payload


def qpc_bracket(before: int, after: int) -> bytes:
    assert before <= after
    payload = qpc_read(before) + qpc_read(after)
    assert len(payload) == QPC_BRACKET_LEN
    return payload


def dwm_timing_call(before: int, after: int, snapshot: bytes) -> bytes:
    assert len(snapshot) == DWM_SNAPSHOT_LEN
    payload = (
        qpc_read(before)
        + struct.pack(">i", 0)  # S_OK
        + bytes([1])  # snapshot-present marker
        + snapshot
        + qpc_read(after)
    )
    assert len(payload) == DWM_TIMING_CALL_LEN
    return payload


def presenter_facts(present_policy_digest: bytes) -> bytes:
    assert len(present_policy_digest) == 32
    payload = (
        present_policy_digest
        + struct.pack(">I", 720)  # requested scanlines
        + struct.pack(">i", 720)  # returned scanlines
        + struct.pack(">I", 0)  # SetDIBitsToDevice error code
        + struct.pack(">i", 1)  # ReleaseDC success
        + struct.pack(">I", 0)  # ReleaseDC error code
    )
    assert len(payload) == PRESENTER_FACTS_LEN
    return payload


def gdi_flush_call(before: int, after: int) -> bytes:
    payload = qpc_read(before) + bytes([1]) + qpc_read(after)
    assert len(payload) == GDI_FLUSH_CALL_LEN
    return payload


def dwm_flush_call(before: int, after: int) -> bytes:
    payload = qpc_read(before) + struct.pack(">i", 0) + qpc_read(after)
    assert len(payload) == DWM_FLUSH_CALL_LEN
    return payload


def envelope_payload(timing_policy_digest: bytes, present_policy_digest: bytes) -> bytes:
    parts = (
        ("policy", timing_policy_digest),
        ("frame_receipt_v2", frame_receipt_v2_payload()),
        ("presenter", presenter_facts(present_policy_digest)),
        ("qpc_frequency", struct.pack(">BQI", 1, 10_000_000, 0)),
        (
            "pre_dwm",
            dwm_timing_call(1_000_000, 1_000_100, encode_snapshot(snapshot_fixture(0))),
        ),
        ("presenter_bracket", qpc_bracket(1_000_200, 1_000_400)),
        ("gdi_flush", gdi_flush_call(1_000_500, 1_000_600)),
        ("dwm_flush", dwm_flush_call(1_000_700, 1_000_800)),
        (
            "post_dwm",
            dwm_timing_call(1_000_900, 1_001_000, encode_snapshot(snapshot_fixture(1))),
        ),
    )
    expected_offsets = {
        "policy": (0, 32),
        "frame_receipt_v2": (32, 162),
        "presenter": (162, 214),
        "qpc_frequency": (214, 227),
        "pre_dwm": (227, 546),
        "presenter_bracket": (546, 572),
        "gdi_flush": (572, 599),
        "dwm_flush": (599, 629),
        "post_dwm": (629, 948),
    }
    offsets: dict[str, tuple[int, int]] = {}
    cursor = 0
    encoded_parts: list[bytes] = []
    for name, part in parts:
        start = cursor
        cursor += len(part)
        offsets[name] = (start, cursor)
        encoded_parts.append(part)
    assert offsets == expected_offsets
    assert cursor == ENVELOPE_LEN
    payload = b"".join(encoded_parts)
    assert len(payload) == ENVELOPE_LEN
    assert payload[214:227] == struct.pack(">BQI", 1, 10_000_000, 0)
    return payload


def main() -> None:
    present_payload = windows_present_policy_payload()
    present_tag = content_tag(b"policy", WINDOWS_PRESENT_POLICY_SCHEMA, present_payload)
    assert present_tag == EXPECTED_WINDOWS_PRESENT_POLICY
    present_digest = tag_digest(b"policy", WINDOWS_PRESENT_POLICY_SCHEMA, present_payload)

    policy_payload = timing_policy_payload()
    policy_tag = content_tag(b"policy", TIMING_POLICY_SCHEMA, policy_payload)
    policy_digest = tag_digest(b"policy", TIMING_POLICY_SCHEMA, policy_payload)

    payload = envelope_payload(policy_digest, present_digest)
    envelope_tag = content_tag(b"presentation-artifact", ENVELOPE_SCHEMA, payload)

    if policy_tag != EXPECTED_TIMING_POLICY:
        raise SystemExit(f"timing policy drift: {policy_tag} != {EXPECTED_TIMING_POLICY}")
    if envelope_tag != EXPECTED_ENVELOPE:
        raise SystemExit(f"envelope drift: {envelope_tag} != {EXPECTED_ENVELOPE}")

    payload_mutant = bytearray(payload)
    payload_mutant[-1] ^= 1
    assert (
        content_tag(b"presentation-artifact", ENVELOPE_SCHEMA, bytes(payload_mutant))
        != envelope_tag
    )
    assert (
        content_tag(b"presentation-artifact", ENVELOPE_SCHEMA + b"-mutant", payload)
        != envelope_tag
    )

    print(policy_tag)
    print(envelope_tag)
    print("mutation-controls\t2\tchanged")


if __name__ == "__main__":
    main()
