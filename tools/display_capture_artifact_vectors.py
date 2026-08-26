#!/usr/bin/env python3
"""Independent stdlib vectors for Bitblock DisplayCaptureArtifact v1.

This tool imports no Bitblock/Rust code. It reconstructs the embedded typed
DisplayReceipt v2, canonical expected/captured frames, artifact payload, and
the readable display-artifact content tag. Its decoder also rechecks the
receipt observations and exact comparison against the carried pixels.
"""

from __future__ import annotations

import base64
import hashlib
import struct


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
ARTIFACT_KIND = b"display-artifact"
ARTIFACT_SCHEMA = b"bitblock/display-capture-artifact/v1"
FRAME_KIND = b"frame"
FRAME_SCHEMA = b"bitblock/frame/xrgb8888-row-major/v1"
FRAME_RECEIPT_KIND = b"frame-receipt"
FRAME_RECEIPT_SCHEMA = b"bitblock/frame-receipt/v2"
DISPLAY_RECEIPT_KIND = b"display-receipt"
DISPLAY_RECEIPT_SCHEMA = b"bitblock/display-receipt/v2"
DISPLAY_POLICY_SCHEMA = b"bitblock/display-capture-policy/gdi-screen-dc/v1"
TEST_OBJECT_SCHEMA = b"bitblock/display-artifact-test/v1"

FRAME_RECEIPT_LEN = 130
DISPLAY_FACTS_LEN = 175
DISPLAY_RECEIPT_LEN = 378
FRAME_FIXED_LEN = 2 + len(FRAME_SCHEMA) + 4 + 4 + 8

ABSENT = 0
REUSE_EXPECTED = 1
INLINE = 2


def framed(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def u32(value: int) -> bytes:
    return struct.pack(">I", value)


def i32(value: int) -> bytes:
    return struct.pack(">i", value)


def u64(value: int) -> bytes:
    return struct.pack(">Q", value)


def i64(value: int) -> bytes:
    return struct.pack(">q", value)


def canonical_preimage(kind: bytes, schema: bytes, payload: bytes) -> bytes:
    return (
        framed(TAG_DOMAIN)
        + framed(kind)
        + framed(HASH_SUITE)
        + framed(schema)
        + u64(len(payload))
        + payload
    )


def digest(kind: bytes, schema: bytes, payload: bytes) -> bytes:
    return hashlib.sha256(canonical_preimage(kind, schema, payload)).digest()


def readable_tag(kind: bytes, value: bytes) -> str:
    encoded = base64.b32encode(value).decode("ascii").lower().rstrip("=")
    return f"#bb.{kind.decode('ascii')}.sha256.{encoded}"


def content_tag(kind: bytes, schema: bytes, payload: bytes) -> str:
    return readable_tag(kind, digest(kind, schema, payload))


def optional_digest(value: bytes | None) -> bytes:
    return b"\x00" + bytes(32) if value is None else b"\x01" + value


def decode_optional(raw: bytes, field: str) -> bytes | None:
    if len(raw) != 33:
        raise ValueError(f"{field}: wrong optional-digest length")
    if raw[0] == 0:
        if raw[1:] != bytes(32):
            raise ValueError(f"{field}: nonzero absent padding")
        return None
    if raw[0] == 1:
        return raw[1:]
    raise ValueError(f"{field}: unknown marker {raw[0]}")


def encode_frame(width: int, height: int, pixels: tuple[int, ...]) -> bytes:
    if width * height != len(pixels):
        raise ValueError("frame shape does not match its pixels")
    return (
        framed(FRAME_SCHEMA)
        + u32(width)
        + u32(height)
        + u64(len(pixels))
        + b"".join(u32(pixel) for pixel in pixels)
    )


def decode_frame(raw: bytes, field: str) -> dict:
    if len(raw) < FRAME_FIXED_LEN:
        raise ValueError(f"{field}: truncated canonical frame")
    domain_len = struct.unpack(">H", raw[:2])[0]
    if domain_len != len(FRAME_SCHEMA):
        raise ValueError(f"{field}: wrong frame domain length")
    if raw[2 : 2 + domain_len] != FRAME_SCHEMA:
        raise ValueError(f"{field}: wrong frame domain")
    cursor = 2 + domain_len
    width, height = struct.unpack(">II", raw[cursor : cursor + 8])
    cursor += 8
    count = struct.unpack(">Q", raw[cursor : cursor + 8])[0]
    cursor += 8
    if count != width * height:
        raise ValueError(f"{field}: pixel count does not match dimensions")
    if count > (len(raw) - cursor) // 4:
        raise ValueError(f"{field}: truncated pixel words")
    expected_len = cursor + count * 4
    if len(raw) != expected_len:
        raise ValueError(f"{field}: wrong canonical frame length")
    pixels = tuple(
        struct.unpack(">I", raw[index : index + 4])[0]
        for index in range(cursor, len(raw), 4)
    )
    return {"width": width, "height": height, "pixels": pixels}


def frame_facts(pixels: tuple[int, ...]) -> tuple[int, ...]:
    rgb = tuple(pixel & 0x00FF_FFFF for pixel in pixels)
    return (
        len(pixels),
        sum(value != 0 for value in rgb),
        sum(pixel & 0xFF00_0000 != 0 for pixel in pixels),
        sum(value.bit_count() for value in rgb),
        sum((value >> 16) & 0xFF for value in rgb),
        sum((value >> 8) & 0xFF for value in rgb),
        sum(value & 0xFF for value in rgb),
    )


def observe_frame(frame: dict) -> dict:
    raw = encode_frame(frame["width"], frame["height"], frame["pixels"])
    return {
        "tag": digest(FRAME_KIND, FRAME_SCHEMA, raw),
        "facts": frame_facts(frame["pixels"]),
    }


def encode_frame_facts(facts: tuple[int, ...]) -> bytes:
    if len(facts) != 7:
        raise ValueError("frame facts require seven lanes")
    return b"".join(u64(value) for value in facts)


def decode_frame_facts(raw: bytes) -> tuple[int, ...]:
    if len(raw) != 56:
        raise ValueError("wrong frame-facts length")
    return tuple(
        struct.unpack(">Q", raw[index : index + 8])[0]
        for index in range(0, len(raw), 8)
    )


def encode_frame_receipt(fields: dict) -> bytes:
    raw = (
        optional_digest(fields["parent"])
        + bytes((fields["stream"],))
        + fields["outcome"]
        + fields["input"]
        + fields["frame"]
    )
    if len(raw) != FRAME_RECEIPT_LEN:
        raise AssertionError("frame-receipt-v2 length drifted")
    return raw


def decode_frame_receipt(raw: bytes) -> dict:
    if len(raw) != FRAME_RECEIPT_LEN:
        raise ValueError("wrong frame-receipt-v2 length")
    parent = decode_optional(raw[:33], "frame receipt parent")
    stream = raw[33]
    if stream not in (0, 1):
        raise ValueError("unknown frame stream")
    fields = {
        "parent": parent,
        "stream": stream,
        "outcome": raw[34:66],
        "input": raw[66:98],
        "frame": raw[98:130],
    }
    if encode_frame_receipt(fields) != raw:
        raise ValueError("noncanonical frame receipt")
    return fields


def rect_bytes(rect: tuple[int, int, int, int]) -> bytes:
    return b"".join(i32(value) for value in rect)


def rect_extent(rect: tuple[int, int, int, int]) -> tuple[int, int] | None:
    width = rect[2] - rect[0]
    height = rect[3] - rect[1]
    if not (0 <= width <= 0x7FFF_FFFF and 0 <= height <= 0x7FFF_FFFF):
        return None
    return width, height


def encode_display_facts(facts: dict) -> bytes:
    captured = facts["captured"]
    captured_bytes = (
        b"\x00" + bytes(88)
        if captured is None
        else b"\x01" + captured["tag"] + encode_frame_facts(captured["facts"])
    )
    first = facts["first_mismatch"]
    raw = (
        bytes((facts["outcome"], facts["stage"]))
        + u32(facts["error_code"])
        + bytes(
            (
                int(facts["flush_before"]),
                int(facts["compositor_flush"]),
                int(facts["flush_after"]),
            )
        )
        + rect_bytes(facts["rect_before"])
        + rect_bytes(facts["rect_after"])
        + u32(facts["dpi"])
        + i64(facts["qpc_start"])
        + i64(facts["qpc_end"])
        + i64(facts["qpc_frequency"])
        + captured_bytes
        + u64(facts["mismatch_count"])
        + (b"\x00" + bytes(8) if first is None else b"\x01" + u64(first))
    )
    if len(raw) != DISPLAY_FACTS_LEN:
        raise AssertionError("display-facts length drifted")
    return raw


def decode_bool(value: int, field: str) -> bool:
    if value not in (0, 1):
        raise ValueError(f"{field}: unknown boolean")
    return bool(value)


def decode_display_facts(raw: bytes) -> dict:
    if len(raw) != DISPLAY_FACTS_LEN:
        raise ValueError("wrong display-facts length")
    if raw[0] not in (0, 1, 2):
        raise ValueError("unknown display outcome")
    if raw[1] > 16:
        raise ValueError("unknown capture stage")
    captured_marker = raw[69]
    if captured_marker == 0:
        if raw[70:158] != bytes(88):
            raise ValueError("captured observation: nonzero absent padding")
        captured = None
    elif captured_marker == 1:
        captured = {
            "tag": raw[70:102],
            "facts": decode_frame_facts(raw[102:158]),
        }
    else:
        raise ValueError("captured observation: unknown marker")
    first_marker = raw[166]
    if first_marker == 0:
        if raw[167:175] != bytes(8):
            raise ValueError("first mismatch: nonzero absent padding")
        first = None
    elif first_marker == 1:
        first = struct.unpack(">Q", raw[167:175])[0]
    else:
        raise ValueError("first mismatch: unknown marker")
    return {
        "outcome": raw[0],
        "stage": raw[1],
        "error_code": struct.unpack(">I", raw[2:6])[0],
        "flush_before": decode_bool(raw[6], "flush before"),
        "compositor_flush": decode_bool(raw[7], "compositor flush"),
        "flush_after": decode_bool(raw[8], "flush after"),
        "rect_before": struct.unpack(">iiii", raw[9:25]),
        "rect_after": struct.unpack(">iiii", raw[25:41]),
        "dpi": struct.unpack(">I", raw[41:45])[0],
        "qpc_start": struct.unpack(">q", raw[45:53])[0],
        "qpc_end": struct.unpack(">q", raw[53:61])[0],
        "qpc_frequency": struct.unpack(">q", raw[61:69])[0],
        "captured": captured,
        "mismatch_count": struct.unpack(">Q", raw[158:166])[0],
        "first_mismatch": first,
    }


def validate_display_facts(
    expected_frame: bytes, width: int, height: int, facts: dict
) -> None:
    captured = facts["captured"]
    if captured is not None and (
        captured["facts"][0] != width * height or captured["facts"][2] != 0
    ):
        raise ValueError("captured observation facts are inconsistent")
    complete_geometry = (
        facts["rect_before"] == facts["rect_after"]
        and rect_extent(facts["rect_before"]) == (width, height)
    )
    complete_barriers = (
        facts["flush_before"]
        and facts["compositor_flush"]
        and facts["flush_after"]
        and facts["qpc_frequency"] > 0
        and facts["qpc_end"] >= facts["qpc_start"]
    )
    if facts["outcome"] == 0:
        valid = (
            facts["stage"] == 0
            and facts["error_code"] == 0
            and width > 0
            and height > 0
            and facts["dpi"] > 0
            and captured is not None
            and captured["tag"] == expected_frame
            and facts["mismatch_count"] == 0
            and facts["first_mismatch"] is None
            and complete_geometry
            and complete_barriers
        )
    elif facts["outcome"] == 1:
        valid = (
            facts["stage"] == 0
            and facts["error_code"] == 0
            and width > 0
            and height > 0
            and facts["dpi"] > 0
            and captured is not None
            and facts["mismatch_count"] > 0
            and facts["first_mismatch"] is not None
            and facts["first_mismatch"] < width * height
            and complete_geometry
            and complete_barriers
        )
    else:
        no_flush = not (
            facts["flush_before"]
            or facts["compositor_flush"]
            or facts["flush_after"]
        )
        before_readback = (
            facts["flush_before"]
            and facts["compositor_flush"]
            and not facts["flush_after"]
        )
        after_readback = (
            facts["flush_before"]
            and facts["compositor_flush"]
            and facts["flush_after"]
        )
        untouched_geometry = (
            facts["rect_before"] == (0, 0, 0, 0)
            and facts["rect_after"] == (0, 0, 0, 0)
            and facts["dpi"] == 0
        )
        before_shape_clock = (
            facts["qpc_start"] == 0
            and facts["qpc_end"] == 0
            and facts["qpc_frequency"] == 0
        )
        expected_geometry = (
            width > 0
            and height > 0
            and rect_extent(facts["rect_before"]) == (width, height)
            and facts["dpi"] > 0
        )
        stage = facts["stage"]
        if stage in (1, 2) and captured is None:
            stage_valid = no_flush and untouched_geometry and before_shape_clock
        elif stage == 16 and captured is None:
            stage_valid = (
                facts["flush_before"]
                and not facts["compositor_flush"]
                and not facts["flush_after"]
                and untouched_geometry
                and before_shape_clock
            )
        elif stage in (3, 4) and captured is None:
            stage_valid = before_readback and untouched_geometry and before_shape_clock
        elif stage in (5, 6) and captured is None:
            stage_valid = (
                before_readback
                and facts["rect_after"] == (0, 0, 0, 0)
                and facts["dpi"] == 0
                and before_shape_clock
            )
        elif stage == 7 and captured is None:
            stage_valid = (
                before_readback
                and facts["rect_after"] == (0, 0, 0, 0)
                and facts["dpi"] > 0
                and before_shape_clock
            )
        elif stage == 8 and captured is None:
            stage_valid = (
                before_readback
                and expected_geometry
                and facts["rect_after"] == (0, 0, 0, 0)
            )
        elif stage in (9, 10, 11, 12, 13, 14) and captured is None:
            stage_valid = (
                before_readback
                and expected_geometry
                and facts["rect_after"] == (0, 0, 0, 0)
                and facts["qpc_frequency"] > 0
            )
        elif stage == 8 and captured is not None:
            stage_valid = (
                after_readback
                and expected_geometry
                and facts["rect_after"] == (0, 0, 0, 0)
                and facts["qpc_frequency"] > 0
            )
        elif stage == 15 and captured is not None:
            stage_valid = (
                after_readback
                and expected_geometry
                and facts["qpc_frequency"] > 0
                and facts["qpc_end"] >= facts["qpc_start"]
            )
        elif stage == 5 and captured is not None:
            stage_valid = (
                after_readback
                and expected_geometry
                and facts["rect_after"] == facts["rect_before"]
                and facts["qpc_frequency"] > 0
                and facts["qpc_end"] >= facts["qpc_start"]
            )
        else:
            stage_valid = False
        valid = (
            stage != 0
            and facts["mismatch_count"] == 0
            and facts["first_mismatch"] is None
            and stage_valid
        )
    if not valid:
        raise ValueError("inconsistent display receipt facts")


def encode_display_receipt(fields: dict) -> bytes:
    raw = (
        optional_digest(fields["parent"])
        + encode_frame_receipt(fields["expected_receipt"])
        + u32(fields["width"])
        + u32(fields["height"])
        + fields["policy"]
        + encode_display_facts(fields["facts"])
    )
    if len(raw) != DISPLAY_RECEIPT_LEN:
        raise AssertionError("display-receipt-v2 length drifted")
    return raw


def decode_display_receipt(raw: bytes) -> dict:
    if len(raw) != DISPLAY_RECEIPT_LEN:
        raise ValueError("wrong display-receipt-v2 length")
    fields = {
        "parent": decode_optional(raw[:33], "display receipt parent"),
        "expected_receipt": decode_frame_receipt(raw[33:163]),
        "width": struct.unpack(">I", raw[163:167])[0],
        "height": struct.unpack(">I", raw[167:171])[0],
        "policy": raw[171:203],
        "facts": decode_display_facts(raw[203:378]),
    }
    if fields["expected_receipt"]["stream"] != 1:
        raise ValueError("display receipt requires the composite stream")
    validate_display_facts(
        fields["expected_receipt"]["frame"],
        fields["width"],
        fields["height"],
        fields["facts"],
    )
    if encode_display_receipt(fields) != raw:
        raise ValueError("noncanonical display receipt")
    return fields


def encode_artifact(fields: dict) -> bytes:
    expected = encode_frame(
        fields["expected"]["width"],
        fields["expected"]["height"],
        fields["expected"]["pixels"],
    )
    inline = b""
    if fields["form"] == INLINE:
        captured = fields["captured"]
        inline = encode_frame(
            captured["width"], captured["height"], captured["pixels"]
        )
    return (
        encode_display_receipt(fields["receipt"])
        + u64(len(expected))
        + expected
        + bytes((fields["form"],))
        + u64(len(inline))
        + inline
    )


def decode_artifact(payload: bytes) -> dict:
    if len(payload) < DISPLAY_RECEIPT_LEN + 8:
        raise ValueError("artifact truncated before expected-frame length")
    receipt = decode_display_receipt(payload[:DISPLAY_RECEIPT_LEN])
    cursor = DISPLAY_RECEIPT_LEN
    expected_len = struct.unpack(">Q", payload[cursor : cursor + 8])[0]
    cursor += 8
    if expected_len > len(payload) - cursor:
        raise ValueError("artifact truncated in expected frame")
    expected_end = cursor + expected_len
    expected = decode_frame(payload[cursor:expected_end], "expected frame")
    cursor = expected_end
    if len(payload) - cursor < 9:
        raise ValueError("artifact truncated in capture metadata")
    form = payload[cursor]
    cursor += 1
    if form not in (ABSENT, REUSE_EXPECTED, INLINE):
        raise ValueError("unknown captured form")
    captured_len = struct.unpack(">Q", payload[cursor : cursor + 8])[0]
    cursor += 8
    if captured_len > len(payload) - cursor:
        raise ValueError("artifact truncated in captured frame")
    captured_end = cursor + captured_len
    if captured_end != len(payload):
        raise ValueError("artifact has trailing bytes")
    if form in (ABSENT, REUSE_EXPECTED) and captured_len != 0:
        raise ValueError("noncanonical captured form length")
    if form == INLINE and captured_len == 0:
        raise ValueError("empty inline captured frame")
    if form == ABSENT:
        captured = None
    elif form == REUSE_EXPECTED:
        captured = expected
    else:
        captured = decode_frame(payload[cursor:captured_end], "captured frame")

    viewport = (receipt["width"], receipt["height"])
    if (expected["width"], expected["height"]) != viewport:
        raise ValueError("expected-frame viewport mismatch")
    expected_observation = observe_frame(expected)
    if expected_observation["facts"][2] != 0:
        raise ValueError("expected frame has non-XRGB high bytes")
    if expected_observation["tag"] != receipt["expected_receipt"]["frame"]:
        raise ValueError("expected frame tag is stale")

    stored_observation = receipt["facts"]["captured"]
    if (stored_observation is None) != (captured is None):
        raise ValueError("captured presence disagrees with receipt")
    if captured is not None:
        if (captured["width"], captured["height"]) != viewport:
            raise ValueError("captured-frame viewport mismatch")
        captured_observation = observe_frame(captured)
        if captured_observation["facts"][2] != 0:
            raise ValueError("captured frame has non-XRGB high bytes")
        if captured_observation != stored_observation:
            raise ValueError("captured observation is stale")
    if form == INLINE and captured["pixels"] == expected["pixels"]:
        raise ValueError("duplicate inline expected frame")

    facts = receipt["facts"]
    if facts["stage"] == 0:
        if captured is None:
            raise ValueError("complete capture has no pixels")
        mismatches = tuple(
            index
            for index, pair in enumerate(zip(expected["pixels"], captured["pixels"]))
            if pair[0] != pair[1]
        )
        actual = (0 if not mismatches else 1, len(mismatches), mismatches[0] if mismatches else None)
        declared = (
            facts["outcome"],
            facts["mismatch_count"],
            facts["first_mismatch"],
        )
        if actual != declared:
            raise ValueError("comparison facts are stale")
    elif facts["outcome"] != 2:
        raise ValueError("incomplete capture is not unavailable")

    fields = {
        "receipt": receipt,
        "expected": expected,
        "form": form,
        "captured": captured,
    }
    if encode_artifact(fields) != payload:
        raise ValueError("noncanonical display artifact")
    return fields


EXPECTED_PIXELS = (0x0011_2233, 0x0044_5566)
MISMATCH_PIXELS = (0x0011_2233, 0x0044_5567)
CAPTURE_RECT = (-20, 30, -18, 31)


def display_policy() -> bytes:
    payload = struct.pack(">H", 1) + b"\x00\x00" + u32(0x40CC_0020) + bytes(5)
    if len(payload) != 13:
        raise AssertionError("display policy length drifted")
    return digest(b"policy", DISPLAY_POLICY_SCHEMA, payload)


def expected_frame() -> dict:
    return {"width": 2, "height": 1, "pixels": EXPECTED_PIXELS}


def complete_facts(captured_pixels: tuple[int, ...]) -> dict:
    captured = {"width": 2, "height": 1, "pixels": captured_pixels}
    mismatch_indexes = tuple(
        index
        for index, pair in enumerate(zip(EXPECTED_PIXELS, captured_pixels))
        if pair[0] != pair[1]
    )
    return {
        "outcome": 0 if not mismatch_indexes else 1,
        "stage": 0,
        "error_code": 0,
        "flush_before": True,
        "compositor_flush": True,
        "flush_after": True,
        "rect_before": CAPTURE_RECT,
        "rect_after": CAPTURE_RECT,
        "dpi": 144,
        "qpc_start": 100,
        "qpc_end": 120,
        "qpc_frequency": 10_000_000,
        "captured": observe_frame(captured),
        "mismatch_count": len(mismatch_indexes),
        "first_mismatch": mismatch_indexes[0] if mismatch_indexes else None,
    }


def unavailable_facts() -> dict:
    return {
        "outcome": 2,
        "stage": 2,
        "error_code": 5,
        "flush_before": False,
        "compositor_flush": False,
        "flush_after": False,
        "rect_before": (0, 0, 0, 0),
        "rect_after": (0, 0, 0, 0),
        "dpi": 0,
        "qpc_start": 0,
        "qpc_end": 0,
        "qpc_frequency": 0,
        "captured": None,
        "mismatch_count": 0,
        "first_mismatch": None,
    }


def build_receipt(facts: dict) -> dict:
    expected = expected_frame()
    frame_payload = encode_frame(
        expected["width"], expected["height"], expected["pixels"]
    )
    frame_receipt = {
        "parent": None,
        "stream": 1,
        "outcome": digest(b"tick-outcome", TEST_OBJECT_SCHEMA, b"outcome"),
        "input": digest(b"frame-input", TEST_OBJECT_SCHEMA, b"input"),
        "frame": digest(FRAME_KIND, FRAME_SCHEMA, frame_payload),
    }
    return {
        "parent": None,
        "expected_receipt": frame_receipt,
        "width": 2,
        "height": 1,
        "policy": display_policy(),
        "facts": facts,
    }


def build_match_reuse() -> bytes:
    fields = {
        "receipt": build_receipt(complete_facts(EXPECTED_PIXELS)),
        "expected": expected_frame(),
        "form": REUSE_EXPECTED,
        "captured": expected_frame(),
    }
    return encode_artifact(fields)


def build_mismatch_inline() -> bytes:
    fields = {
        "receipt": build_receipt(complete_facts(MISMATCH_PIXELS)),
        "expected": expected_frame(),
        "form": INLINE,
        "captured": {"width": 2, "height": 1, "pixels": MISMATCH_PIXELS},
    }
    return encode_artifact(fields)


def build_unavailable_absent() -> bytes:
    fields = {
        "receipt": build_receipt(unavailable_facts()),
        "expected": expected_frame(),
        "form": ABSENT,
        "captured": None,
    }
    return encode_artifact(fields)


VECTORS = {
    "match-reuse": (
        build_match_reuse,
        "c9c210c9ec7de6389911c2eb76546a6759d9a58e1ed308a73990b65e535c3e05",
        "#bb.display-artifact.sha256.fbd5hyvucb3zycqdqletiyrasrflufmnkjuo4zkdkd6iddprjs6q",
    ),
    "mismatch-inline": (
        build_mismatch_inline,
        "4d59d1bf4fdf6822b7dc54332f899fce690ba3da8f8d72f8cca235bb5586f8fb",
        "#bb.display-artifact.sha256.6seho25bidubdwpxr3s5jwku2jm6k252qiibuuqdz7x7inkxt42q",
    ),
    "unavailable-absent": (
        build_unavailable_absent,
        "27cd4a270b1ab1b3a36e7b7eb2b731d70f0cf546005d2305b7ee0a35157fbc10",
        "#bb.display-artifact.sha256.prtxaadxkb7lfpawawthysg2jutx2dqikujmp6osg6tvpodkkzmq",
    ),
}


def layout(payload: bytes) -> tuple[int, int, int, int]:
    expected_start = DISPLAY_RECEIPT_LEN + 8
    expected_len = struct.unpack(
        ">Q", payload[DISPLAY_RECEIPT_LEN:expected_start]
    )[0]
    expected_end = expected_start + expected_len
    captured_start = expected_end + 9
    captured_len = struct.unpack(">Q", payload[expected_end + 1 : captured_start])[0]
    return expected_start, expected_end, captured_start, captured_start + captured_len


def expect_reject(name: str, payload: bytes) -> None:
    try:
        decode_artifact(payload)
    except (IndexError, struct.error, ValueError):
        return
    raise SystemExit(f"negative control accepted: {name}")


def negative_controls() -> None:
    match = build_match_reuse()
    mismatch = build_mismatch_inline()
    _, match_expected_end, _, _ = layout(match)
    match_expected_start = DISPLAY_RECEIPT_LEN + 8

    expect_reject("truncated receipt", match[: DISPLAY_RECEIPT_LEN - 1])
    expect_reject("truncated expected length", match[: DISPLAY_RECEIPT_LEN + 4])
    expect_reject("truncated expected frame", match[: match_expected_end - 1])
    expect_reject("truncated capture metadata", match[: match_expected_end + 5])
    expect_reject("truncated inline frame", mismatch[:-1])

    mutant = bytearray(match)
    mutant[match_expected_end] = 3
    expect_reject("unknown captured form", bytes(mutant))

    expected_raw = match[match_expected_start:match_expected_end]
    duplicate_inline = (
        match[:match_expected_end]
        + bytes((INLINE,))
        + u64(len(expected_raw))
        + expected_raw
    )
    expect_reject("duplicate inline expected", duplicate_inline)

    mutant = bytearray(match)
    mutant[33 + 98] ^= 1
    expect_reject("stale receipt expected frame", bytes(mutant))

    mutant = bytearray(mismatch)
    mutant[203 + 70] ^= 1
    expect_reject("stale captured observation", bytes(mutant))

    mutant = bytearray(mismatch)
    mutant[203 + 158 : 203 + 166] = u64(2)
    expect_reject("stale comparison facts", bytes(mutant))

    expect_reject("trailing bytes", match + b"\x00")


def main() -> None:
    for name, (build, expected_payload_hash, expected_tag) in VECTORS.items():
        payload = build()
        decode_artifact(payload)
        payload_hash = hashlib.sha256(payload).hexdigest()
        preimage = canonical_preimage(ARTIFACT_KIND, ARTIFACT_SCHEMA, payload)
        actual_tag = content_tag(ARTIFACT_KIND, ARTIFACT_SCHEMA, payload)
        print(
            f"{name}\t{len(payload)}\t{len(preimage)}\t{payload_hash}\t{actual_tag}"
        )
        if expected_payload_hash and payload_hash != expected_payload_hash:
            raise SystemExit(f"{name}: payload hash mismatch: {payload_hash}")
        if expected_tag and actual_tag != expected_tag:
            raise SystemExit(f"{name}: tag mismatch: {actual_tag}")
    negative_controls()
    print("negative-controls\t11\trejected")


if __name__ == "__main__":
    main()
