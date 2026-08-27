#!/usr/bin/env python3
"""Independent stdlib vectors for the Bitblock DisplayReceipt v1 payload.

This file intentionally imports no Bitblock/Rust code. It reconstructs the canonical 312-byte
payload and independently applies the content-tag wrapper.

A display receipt binds one screen-capture attempt to the CPU frame that was submitted: the
already-published frame receipt, the exact expected framebuffer, the client rectangle it was
compared over, the declared capture policy, and the complete facts of the attempt. The
byte-distinct v2 object at `tools/display_receipt_v2_vectors.py` replaces the unqualified frame
receipt with a complete typed one; the 175-byte capture-facts block is shared, so the two suites
must keep agreeing on it.

Failure is a first-class result here rather than an absent one, so the vectors carry a matching
capture, a mismatching capture, and an attempt that never reached readback. All three are the
same schema and the same length; only the fact bytes separate them.
"""

from __future__ import annotations

import base64
import hashlib
import struct

TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
DISPLAY_RECEIPT_SCHEMA = b"bitblock/display-receipt/v1"
DISPLAY_POLICY_SCHEMA = b"bitblock/display-capture-policy/gdi-screen-dc/v1"
CANONICAL_FRAME_DOMAIN = b"bitblock/frame/xrgb8888-row-major/v1"
# The fixture schema crates/core/src/display.rs uses for stand-in nested objects, shared with
# tools/display_receipt_v2_vectors.py so both suites name the same frame receipt.
DISPLAY_TEST_SCHEMA = b"bitblock/display-test/v1"

DISPLAY_RECEIPT_PAYLOAD_LEN = 312
DISPLAY_POLICY_PAYLOAD_LEN = 13
CAPTURE_FACTS_LEN = 175
SRCCOPY_CAPTUREBLT = 0x40CC0020
RGB_MASK = 0x00FFFFFF

OUTCOME_MATCH, OUTCOME_MISMATCH, OUTCOME_UNAVAILABLE = 0, 1, 2
STAGE_COMPLETE, STAGE_WINDOW_HANDLE = 0, 1

EXPECTED_WORDS = (0x00112233, 0x00445566)
CAPTURED_WORDS = (0x00112233, 0x00445567)
VIEWPORT = (2, 1)
CLIENT_RECT = (-20, 30, -18, 31)
DPI = 144
QPC = (100, 120, 10_000_000)

EXPECTED_POLICY = "#bb.policy.sha256.vwu2yb5l6tcqw3foxladgwir4umvpzwdec7dim4at6eethztrksa"
EXPECTED_FRAME = "#bb.frame.sha256.wg6ev5ighrfukd3xzyrx3ejpn55kd2ol4p5sio7au3oww2oac54a"
# The v1 known answer crates/core/src/display.rs has asserted since the audit path was added.
EXPECTED_MATCHED = (
    "#bb.display-receipt.sha256.rsbrsj6aui5mk57qc63uadfylkrj3aritdziqhlhvhxwclj4hz7q"
)
EXPECTED_MISMATCHED = (
    "#bb.display-receipt.sha256.ro5bwfyygtmzp5hqeiripljfpia67fc5mcheebnqg27elugpq7fq"
)
EXPECTED_UNAVAILABLE = (
    "#bb.display-receipt.sha256.e7db43nscarqcmnbzec6w44vxm4fepcevsb2yws3th6okh6ihnea"
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


def optional_digest(value: bytes | None) -> bytes:
    if value is None:
        return b"\x00" + bytes(32)
    if len(value) != 32:
        raise ValueError("optional link must be a full digest")
    return b"\x01" + value


def frame_digest(width: int, height: int, words: tuple[int, ...]) -> bytes:
    payload = (
        field(CANONICAL_FRAME_DOMAIN)
        + struct.pack(">IIQ", width, height, len(words))
        + b"".join(struct.pack(">I", word) for word in words)
    )
    return content_tag_digest(b"frame", CANONICAL_FRAME_DOMAIN, payload)


def display_policy_digest() -> bytes:
    payload = (
        struct.pack(">H", 1)  # policy revision
        + b"\x00"  # GDI screen DC, not a window rerender
        + b"\x00"  # top-down 32-bit BI_RGB DIB
        + struct.pack(">I", SRCCOPY_CAPTUREBLT)
        + bytes(5)  # high-byte mask, complete-slice compare, geometry, QPC, DwmFlush
    )
    if len(payload) != DISPLAY_POLICY_PAYLOAD_LEN:
        raise ValueError(f"display policy payload is {len(payload)} bytes")
    return content_tag_digest(b"policy", DISPLAY_POLICY_SCHEMA, payload)


def frame_facts(words: tuple[int, ...]) -> tuple[int, ...]:
    rgb = [word & RGB_MASK for word in words]
    return (
        len(words),
        sum(value != 0 for value in rgb),
        sum(word & ~RGB_MASK != 0 for word in words),
        sum(value.bit_count() for value in rgb),
        sum((value >> 16) & 0xFF for value in rgb),
        sum((value >> 8) & 0xFF for value in rgb),
        sum(value & 0xFF for value in rgb),
    )


def encode_capture_facts(
    outcome: int,
    stage: int,
    error_code: int,
    flushes: tuple[bool, bool, bool],
    rect_before: tuple[int, int, int, int],
    rect_after: tuple[int, int, int, int],
    dpi: int,
    qpc: tuple[int, int, int],
    captured: tuple[bytes, tuple[int, ...]] | None,
    mismatch_count: int,
    first_mismatch: int | None,
) -> bytes:
    payload = bytes([outcome, stage]) + struct.pack(">I", error_code)
    payload += bytes(int(flag) for flag in flushes)
    payload += struct.pack(">iiii", *rect_before) + struct.pack(">iiii", *rect_after)
    payload += struct.pack(">Iqqq", dpi, *qpc)
    if captured is None:
        payload += b"\x00" + bytes(32) + b"".join(struct.pack(">Q", 0) for _ in range(7))
    else:
        digest, facts = captured
        payload += b"\x01" + digest + b"".join(struct.pack(">Q", value) for value in facts)
    payload += struct.pack(">Q", mismatch_count)
    if first_mismatch is None:
        payload += b"\x00" + struct.pack(">Q", 0)
    else:
        payload += b"\x01" + struct.pack(">Q", first_mismatch)
    if len(payload) != CAPTURE_FACTS_LEN:
        raise ValueError(f"capture facts block is {len(payload)} bytes")
    return payload


def encode_display_receipt(
    parent: bytes | None,
    expected_receipt: bytes,
    expected_frame: bytes,
    width: int,
    height: int,
    policy: bytes,
    facts: bytes,
) -> bytes:
    payload = (
        optional_digest(parent)
        + expected_receipt
        + expected_frame
        + struct.pack(">II", width, height)
        + policy
        + facts
    )
    if len(payload) != DISPLAY_RECEIPT_PAYLOAD_LEN:
        raise ValueError(f"display receipt payload is {len(payload)} bytes")
    return payload


def matched_facts(frame: bytes) -> bytes:
    """A complete capture whose every word equalled the expected frame."""
    return encode_capture_facts(
        OUTCOME_MATCH,
        STAGE_COMPLETE,
        0,
        (True, True, True),
        CLIENT_RECT,
        CLIENT_RECT,
        DPI,
        QPC,
        (frame, frame_facts(EXPECTED_WORDS)),
        0,
        None,
    )


def mismatched_facts() -> bytes:
    """The same complete capture, with the second word off by one."""
    return encode_capture_facts(
        OUTCOME_MISMATCH,
        STAGE_COMPLETE,
        0,
        (True, True, True),
        CLIENT_RECT,
        CLIENT_RECT,
        DPI,
        QPC,
        (frame_digest(*VIEWPORT, CAPTURED_WORDS), frame_facts(CAPTURED_WORDS)),
        1,
        1,
    )


def unavailable_facts(error_code: int) -> bytes:
    """An attempt refused at the window handle: no flush, no geometry, no clock, no pixels."""
    return encode_capture_facts(
        OUTCOME_UNAVAILABLE,
        STAGE_WINDOW_HANDLE,
        error_code,
        (False, False, False),
        (0, 0, 0, 0),
        (0, 0, 0, 0),
        0,
        (0, 0, 0),
        None,
        0,
        None,
    )


def receipt_digest(**changes: object) -> bytes:
    frame = frame_digest(*VIEWPORT, EXPECTED_WORDS)
    base: dict[str, object] = {
        "parent": None,
        "expected_receipt": content_tag_digest(
            b"frame-receipt", DISPLAY_TEST_SCHEMA, b"submitted"
        ),
        "expected_frame": frame,
        "width": VIEWPORT[0],
        "height": VIEWPORT[1],
        "policy": display_policy_digest(),
        "facts": matched_facts(frame),
    }
    base.update(changes)
    return content_tag_digest(
        b"display-receipt",
        DISPLAY_RECEIPT_SCHEMA,
        encode_display_receipt(**base),  # type: ignore[arg-type]
    )


def require_distinct(label: str, tags: list[tuple[str, str]]) -> None:
    seen: dict[str, str] = {}
    for name, value in tags:
        if value in seen:
            raise SystemExit(f"{label}: {name} collided with {seen[value]}")
        seen[value] = name
    print(f"  {len(tags)} distinct tags across {label}")


def main() -> None:
    policy = display_policy_digest()
    frame = frame_digest(*VIEWPORT, EXPECTED_WORDS)
    matched = receipt_digest()
    mismatched = receipt_digest(parent=matched, facts=mismatched_facts())
    unavailable = receipt_digest(facts=unavailable_facts(5))

    print("display receipt v1")
    print(f"  capture policy     {render(b'policy', policy)}")
    print(f"  expected frame     {render(b'frame', frame)}")
    print(f"  matched, root      {render(b'display-receipt', matched)}")
    print(f"  mismatched, linked {render(b'display-receipt', mismatched)}")
    print(f"  unavailable, root  {render(b'display-receipt', unavailable)}")

    for label, actual, expected in (
        ("capture policy", render(b"policy", policy), EXPECTED_POLICY),
        ("expected frame", render(b"frame", frame), EXPECTED_FRAME),
        ("matched receipt", render(b"display-receipt", matched), EXPECTED_MATCHED),
        ("mismatched receipt", render(b"display-receipt", mismatched), EXPECTED_MISMATCHED),
        ("unavailable receipt", render(b"display-receipt", unavailable), EXPECTED_UNAVAILABLE),
    ):
        if actual != expected:
            raise SystemExit(f"{label} tag moved: {actual}")

    variants = [
        ("matched, root", render(b"display-receipt", matched)),
        ("mismatched, linked", render(b"display-receipt", mismatched)),
        ("unavailable, root", render(b"display-receipt", unavailable)),
        (
            "mismatched, unlinked",
            render(b"display-receipt", receipt_digest(facts=mismatched_facts())),
        ),
        (
            "linked to an all-zero parent",
            render(b"display-receipt", receipt_digest(parent=bytes(32))),
        ),
        (
            "other submitted receipt",
            render(
                b"display-receipt",
                receipt_digest(
                    expected_receipt=content_tag_digest(
                        b"frame-receipt", DISPLAY_TEST_SCHEMA, b"resubmitted"
                    )
                ),
            ),
        ),
        (
            "other expected frame",
            render(
                b"display-receipt",
                receipt_digest(expected_frame=frame_digest(*VIEWPORT, CAPTURED_WORDS)),
            ),
        ),
        ("wider client", render(b"display-receipt", receipt_digest(width=VIEWPORT[0] + 1))),
        ("taller client", render(b"display-receipt", receipt_digest(height=VIEWPORT[1] + 1))),
        (
            "other capture policy",
            render(b"display-receipt", receipt_digest(policy=bytes(range(0x40, 0x60)))),
        ),
        (
            "unavailable at another error code",
            render(b"display-receipt", receipt_digest(facts=unavailable_facts(6))),
        ),
    ]
    print()
    require_distinct("one changed display-receipt field at a time", variants)

    # An unavailable attempt zero-fills the frame-facts block rather than omitting it, so the
    # present marker is the only thing separating "no pixels" from "an all-zero capture".
    absent = unavailable_facts(5)
    if absent[102:158] != bytes(56):
        raise SystemExit("absent capture facts are not zero-filled")
    if absent[70:102] != bytes(32):
        raise SystemExit("absent capture digest padding is not 32 zero bytes")
    print("  an absent capture zero-fills its digest and every frame-fact lane")


if __name__ == "__main__":
    main()
