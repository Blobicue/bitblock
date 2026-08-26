#!/usr/bin/env python3
"""Independent stdlib KAT for PresentationOutputArtifact v1.

The outer payload is reconstructed byte-for-byte from the independent Python
vectors for its four nested canonical objects.  No Rust code, FFI, generated
binding, or serialized Rust output is imported.
"""

from __future__ import annotations

import base64
from dataclasses import dataclass
import hashlib
import struct

import composition_timing_vectors as timing_vectors
import display_capture_artifact_vectors as display_vectors
import output_topology_vectors as topology_vectors


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
KIND = b"presentation-output-artifact"
SCHEMA = b"bitblock/presentation-output-artifact/v1"

OTHER_KIND = b"presentation-artifact"
OTHER_SCHEMA = b"bitblock/presentation-output-artifact/v2"

LENGTH_BYTES = 8
TIMING_BYTES = 948
FIXED_OUTER_BYTES = LENGTH_BYTES + TIMING_BYTES + LENGTH_BYTES + LENGTH_BYTES
MAX_PAYLOAD_BYTES = 8 * 1024 * 1024


# These constants freeze the independently built nested fixtures and final outer
# object.  Any byte drift changes at least one digest and fails the KAT.
EXPECTED_NESTED = {
    "display": (
        457,
        "c9c210c9ec7de6389911c2eb76546a6759d9a58e1ed308a73990b65e535c3e05",
        "#bb.display-artifact.sha256.fbd5hyvucb3zycqdqletiyrasrflufmnkjuo4zkdkd6iddprjs6q",
    ),
    "timing": (
        948,
        "4584d7ac7f6dedf69b2772be9d01c7e0276236f693c62a144299a34be5971108",
        "#bb.presentation-artifact.sha256.jexny7eocz3dnu2mqseiz5suabch7r7rn4rw6wew6etuvsb7xrdq",
    ),
    "pre-topology": (
        475,
        "784ba31b22acb62f5834344292120643a156b51fef2093bfa1fe07ae7af75c3a",
        "#bb.output-topology.sha256.jbfwsd34kns7uwofvkj6w3u7tq7nlsbqdfw5b725pxpcg44c2h2q",
    ),
    "post-topology": (
        83,
        "7aa02ee2e90872e140bc20186d84b43af2ff840e9c0b484560b96c2935f921cd",
        "#bb.output-topology.sha256.fw2l66z7jd2eu6suedannr77x5yrugb6sot7y3ojduamdiuemtpq",
    ),
}

EXPECTED_OUTER_LEN = 1_987
EXPECTED_OUTER_SHA256 = (
    "cfb2758c2e2f710772876ad2965cfe6a7785a6f29535359656af659db4765c76"
)
EXPECTED_OUTER_TAG = (
    "#bb.presentation-output-artifact.sha256."
    "fhc42zlstxtkh5mzyhpyjtlpuwo5dna5ryklwjithph577okripa"
)
EXPECTED_DOMAIN_TAGS = {
    "schema": (
        "#bb.presentation-output-artifact.sha256."
        "cppzwtee3rtf77v55go6bgaig2y4cetpcdf2jqf3e2wjxnjop7dq"
    ),
    "kind": (
        "#bb.presentation-artifact.sha256."
        "27kchuy4xlv3577dtkc4ijlbzadykd2fkrsun5nraxq6oco7cvta"
    ),
    "payload": (
        "#bb.presentation-output-artifact.sha256."
        "xcnumgh74vwmnahvweqnox7ghhvd3siifjoyz6nfis37huxiwv6a"
    ),
}


def u64(value: int) -> bytes:
    return struct.pack(">Q", value)


def framed(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def canonical_preimage(kind: bytes, schema: bytes, payload: bytes) -> bytes:
    return (
        framed(TAG_DOMAIN)
        + framed(kind)
        + framed(HASH_SUITE)
        + framed(schema)
        + u64(len(payload))
        + payload
    )


def tag_digest(kind: bytes, schema: bytes, payload: bytes) -> bytes:
    return hashlib.sha256(canonical_preimage(kind, schema, payload)).digest()


def content_tag(kind: bytes, schema: bytes, payload: bytes) -> str:
    encoded = base64.b32encode(tag_digest(kind, schema, payload)).decode("ascii")
    return f"#bb.{kind.decode('ascii')}.sha256.{encoded.lower().rstrip('=')}"


class Reader:
    def __init__(self, data: bytes):
        self.data = data
        self.cursor = 0

    def take(self, length: int, field: str) -> bytes:
        if length < 0 or length > len(self.data) - self.cursor:
            raise ValueError(f"{field}: truncated")
        end = self.cursor + length
        value = self.data[self.cursor:end]
        self.cursor = end
        return value

    def unsigned(self, width: int, field: str) -> int:
        return int.from_bytes(self.take(width, field), "big")

    def signed(self, width: int, field: str) -> int:
        return int.from_bytes(self.take(width, field), "big", signed=True)

    def boolean(self, field: str) -> bool:
        value = self.unsigned(1, field)
        if value not in (0, 1):
            raise ValueError(f"{field}: unknown boolean marker")
        return bool(value)

    def finish(self) -> None:
        if self.cursor != len(self.data):
            raise ValueError(f"trailing bytes: {len(self.data) - self.cursor}")


def build_timing_payload(
    frame_receipt: bytes,
    *,
    frequency: int = 10_000_000,
    post_after: int = 19,
) -> bytes:
    """Build all 948 timing bytes around the supplied canonical receipt."""

    if len(frame_receipt) != timing_vectors.FRAME_RECEIPT_V2_LEN:
        raise ValueError("wrong FrameReceiptV2 length")
    timing_policy = timing_vectors.tag_digest(
        b"policy",
        timing_vectors.TIMING_POLICY_SCHEMA,
        timing_vectors.timing_policy_payload(),
    )
    presenter_policy = timing_vectors.tag_digest(
        b"policy",
        timing_vectors.WINDOWS_PRESENT_POLICY_SCHEMA,
        timing_vectors.windows_present_policy_payload(),
    )
    parts = (
        timing_policy,
        frame_receipt,
        timing_vectors.presenter_facts(presenter_policy),
        struct.pack(">BqI", 1, frequency, 0),
        timing_vectors.dwm_timing_call(
            10,
            11,
            timing_vectors.encode_snapshot(timing_vectors.snapshot_fixture(0)),
        ),
        timing_vectors.qpc_bracket(12, 13),
        timing_vectors.gdi_flush_call(14, 15),
        timing_vectors.dwm_flush_call(16, 17),
        timing_vectors.dwm_timing_call(
            18,
            post_after,
            timing_vectors.encode_snapshot(timing_vectors.snapshot_fixture(1)),
        ),
    )
    payload = b"".join(parts)
    if tuple(map(len, parts)) != (32, 130, 52, 13, 319, 26, 27, 30, 319):
        raise AssertionError("composition timing field layout drifted")
    if len(payload) != TIMING_BYTES:
        raise AssertionError("composition timing envelope is not exactly 948 bytes")
    return payload


def decode_qpc(reader: Reader, field: str) -> tuple[bool, int]:
    succeeded = reader.boolean(f"{field} success")
    value = reader.signed(8, f"{field} value")
    error = reader.unsigned(4, f"{field} error")
    if (succeeded and error != 0) or (not succeeded and value != 0):
        raise ValueError(f"{field}: inconsistent QPC read")
    return succeeded, value


def decode_qpc_bracket(reader: Reader, field: str) -> None:
    decode_qpc(reader, f"{field} before")
    decode_qpc(reader, f"{field} after")


def decode_dwm_timing_call(reader: Reader, field: str) -> tuple[bool, int]:
    decode_qpc(reader, f"{field} before")
    hresult = reader.signed(4, f"{field} HRESULT")
    present = reader.boolean(f"{field} snapshot presence")
    snapshot = reader.take(timing_vectors.DWM_SNAPSHOT_LEN, f"{field} snapshot")
    if present != (hresult == 0):
        raise ValueError(f"{field}: snapshot presence disagrees with HRESULT")
    if not present and any(snapshot):
        raise ValueError(f"{field}: nonzero absent-snapshot padding")
    return decode_qpc(reader, f"{field} after")


def decode_timing_payload(
    payload: bytes,
) -> tuple[bytes, int | None, int | None]:
    """Validate the envelope and return receipt, frequency, and post-call QPC."""

    if len(payload) != TIMING_BYTES:
        raise ValueError("wrong composition timing envelope length")
    reader = Reader(payload)
    expected_policy = timing_vectors.tag_digest(
        b"policy",
        timing_vectors.TIMING_POLICY_SCHEMA,
        timing_vectors.timing_policy_payload(),
    )
    if reader.take(32, "timing policy") != expected_policy:
        raise ValueError("composition timing policy mismatch")

    receipt = reader.take(timing_vectors.FRAME_RECEIPT_V2_LEN, "frame receipt")
    receipt_fields = display_vectors.decode_frame_receipt(receipt)
    if receipt_fields["stream"] != 1:
        raise ValueError("composition timing receipt is not Composite")

    reader.take(32, "presenter policy")
    requested = reader.unsigned(4, "requested scanlines")
    returned = reader.signed(4, "returned scanlines")
    call_error = reader.unsigned(4, "presenter error")
    reader.signed(4, "ReleaseDC result")
    reader.unsigned(4, "ReleaseDC error")
    if (
        requested == 0
        or requested > 0x7FFF_FFFF
        or returned != requested
        or call_error != 0
    ):
        raise ValueError("incomplete presenter submission")

    frequency_succeeded = reader.boolean("QPC frequency success")
    frequency = reader.signed(8, "QPC frequency")
    frequency_error = reader.unsigned(4, "QPC frequency error")
    if (
        frequency_succeeded
        and (frequency <= 0 or frequency_error != 0)
    ) or (not frequency_succeeded and frequency != 0):
        raise ValueError("inconsistent QPC frequency")

    decode_dwm_timing_call(reader, "pre DWM timing")
    decode_qpc_bracket(reader, "checked presenter")
    decode_qpc(reader, "GdiFlush before")
    reader.boolean("GdiFlush success")
    decode_qpc(reader, "GdiFlush after")
    decode_qpc(reader, "DwmFlush before")
    reader.signed(4, "DwmFlush HRESULT")
    decode_qpc(reader, "DwmFlush after")
    post_succeeded, post_value = decode_dwm_timing_call(reader, "post DWM timing")
    reader.finish()
    return (
        receipt,
        frequency if frequency_succeeded else None,
        post_value if post_succeeded else None,
    )


@dataclass(frozen=True)
class NestedParts:
    display: bytes
    timing: bytes
    pre_topology: bytes
    post_topology: bytes


@dataclass(frozen=True)
class OuterLayout:
    display_start: int
    timing_start: int
    pre_length_offset: int
    pre_start: int
    post_length_offset: int
    post_start: int


def build_nested_parts() -> NestedParts:
    display = display_vectors.build_match_reuse()
    display_fields = display_vectors.decode_artifact(display)
    receipt = display_vectors.encode_frame_receipt(
        display_fields["receipt"]["expected_receipt"]
    )
    timing = build_timing_payload(receipt)
    pre = topology_vectors.encode_topology(topology_vectors.available_clone())
    post = topology_vectors.encode_topology(topology_vectors.unavailable())
    if pre.hex() != topology_vectors.EXPECTED_PAYLOAD_HEX["available-clone"]:
        raise AssertionError("available topology full-byte golden drifted")
    if post.hex() != topology_vectors.EXPECTED_PAYLOAD_HEX["unavailable"]:
        raise AssertionError("unavailable topology full-byte golden drifted")
    return NestedParts(display, timing, pre, post)


def encode_outer(parts: NestedParts) -> bytes:
    if len(parts.timing) != TIMING_BYTES:
        raise ValueError("wrong fixed timing length")
    payload = (
        u64(len(parts.display))
        + parts.display
        + parts.timing
        + u64(len(parts.pre_topology))
        + parts.pre_topology
        + u64(len(parts.post_topology))
        + parts.post_topology
    )
    if len(payload) > MAX_PAYLOAD_BYTES:
        raise ValueError("presentation-output artifact exceeds 8 MiB")
    return payload


def decode_outer(payload: bytes) -> tuple[NestedParts, OuterLayout]:
    if len(payload) > MAX_PAYLOAD_BYTES:
        raise ValueError("presentation-output artifact exceeds 8 MiB")
    reader = Reader(payload)

    display_len = reader.unsigned(LENGTH_BYTES, "display length")
    display_start = reader.cursor
    display = reader.take(display_len, "display artifact")
    timing_start = reader.cursor
    timing = reader.take(TIMING_BYTES, "composition timing envelope")
    pre_length_offset = reader.cursor
    pre_len = reader.unsigned(LENGTH_BYTES, "pre-topology length")
    pre_start = reader.cursor
    pre = reader.take(pre_len, "pre-topology")
    post_length_offset = reader.cursor
    post_len = reader.unsigned(LENGTH_BYTES, "post-topology length")
    post_start = reader.cursor
    post = reader.take(post_len, "post-topology")
    reader.finish()

    display_fields = display_vectors.decode_artifact(display)
    timing_receipt, timing_frequency, timing_post = decode_timing_payload(timing)
    topology_vectors.decode_topology(pre)
    topology_vectors.decode_topology(post)
    display_receipt = display_vectors.encode_frame_receipt(
        display_fields["receipt"]["expected_receipt"]
    )
    if display_receipt != timing_receipt:
        raise ValueError("display and timing FrameReceiptV2 values differ")
    capture_facts = display_fields["receipt"]["facts"]
    capture_frequency = capture_facts["qpc_frequency"]
    if timing_frequency is not None and capture_frequency > 0:
        if timing_frequency != capture_frequency:
            raise ValueError("display and timing QPC frequencies differ")
        if timing_post is not None and capture_facts["qpc_start"] < timing_post:
            raise ValueError("display capture precedes timing post-call QPC")

    parts = NestedParts(display, timing, pre, post)
    if encode_outer(parts) != payload:
        raise ValueError("noncanonical presentation-output encoding")
    layout = OuterLayout(
        display_start,
        timing_start,
        pre_length_offset,
        pre_start,
        post_length_offset,
        post_start,
    )
    return parts, layout


def nested_tag(name: str, payload: bytes) -> str:
    if name == "display":
        return content_tag(
            display_vectors.ARTIFACT_KIND,
            display_vectors.ARTIFACT_SCHEMA,
            payload,
        )
    if name == "timing":
        return content_tag(b"presentation-artifact", timing_vectors.ENVELOPE_SCHEMA, payload)
    return content_tag(topology_vectors.KIND, topology_vectors.SCHEMA, payload)


def assert_nested_goldens(parts: NestedParts) -> None:
    values = {
        "display": parts.display,
        "timing": parts.timing,
        "pre-topology": parts.pre_topology,
        "post-topology": parts.post_topology,
    }
    for name, payload in values.items():
        expected_len, expected_hash, expected_tag = EXPECTED_NESTED[name]
        actual_hash = hashlib.sha256(payload).hexdigest()
        actual_tag = nested_tag(name, payload)
        if len(payload) != expected_len:
            raise AssertionError(f"{name}: length drifted to {len(payload)}")
        if expected_hash and actual_hash != expected_hash:
            raise AssertionError(f"{name}: payload bytes drifted")
        if expected_tag and actual_tag != expected_tag:
            raise AssertionError(f"{name}: content tag drifted")


def expect_reject(name: str, payload: bytes) -> None:
    try:
        decode_outer(payload)
    except (IndexError, struct.error, ValueError):
        return
    raise AssertionError(f"negative control accepted: {name}")


def replace_u64(payload: bytes, offset: int, value: int) -> bytes:
    return payload[:offset] + u64(value) + payload[offset + LENGTH_BYTES :]


def main() -> None:
    if FIXED_OUTER_BYTES != 972:
        raise AssertionError("fixed outer overhead is not 972 bytes")

    built = build_nested_parts()
    assert_nested_goldens(built)
    payload = encode_outer(built)
    decoded, layout = decode_outer(payload)
    if decoded != built:
        raise AssertionError("outer decode changed a nested canonical payload")

    # Every outer byte is one of three length lanes or one exact nested slice.
    if payload[:LENGTH_BYTES] != u64(len(built.display)):
        raise AssertionError("display length lane drifted")
    if payload[layout.display_start : layout.timing_start] != built.display:
        raise AssertionError("display nested slice drifted")
    if payload[layout.timing_start : layout.pre_length_offset] != built.timing:
        raise AssertionError("timing nested slice drifted")
    if payload[layout.pre_length_offset : layout.pre_start] != u64(
        len(built.pre_topology)
    ):
        raise AssertionError("pre-topology length lane drifted")
    if payload[layout.pre_start : layout.post_length_offset] != built.pre_topology:
        raise AssertionError("pre-topology nested slice drifted")
    if payload[layout.post_length_offset : layout.post_start] != u64(
        len(built.post_topology)
    ):
        raise AssertionError("post-topology length lane drifted")
    if payload[layout.post_start :] != built.post_topology:
        raise AssertionError("post-topology nested slice drifted")

    fixed = (
        len(payload)
        - len(built.display)
        - len(built.pre_topology)
        - len(built.post_topology)
    )
    if fixed != FIXED_OUTER_BYTES:
        raise AssertionError(f"fixed outer overhead drifted to {fixed}")
    if len(payload) - sum(map(len, built.__dict__.values())) != 3 * LENGTH_BYTES:
        raise AssertionError("outer object contains bytes beyond three length lanes")

    display_fields = display_vectors.decode_artifact(built.display)
    if display_fields["form"] != display_vectors.REUSE_EXPECTED:
        raise AssertionError("display fixture no longer reuses its expected frame")
    _, _, captured_start, captured_end = display_vectors.layout(built.display)
    if captured_start != captured_end:
        raise AssertionError("display fixture gained a second encoded pixel lane")
    pixel_lane = b"".join(display_vectors.u32(pixel) for pixel in display_vectors.EXPECTED_PIXELS)
    if built.display.count(pixel_lane) != 1 or payload.count(pixel_lane) != 1:
        raise AssertionError("expected exactly one encoded display pixel lane")

    display_receipt = display_vectors.encode_frame_receipt(
        display_fields["receipt"]["expected_receipt"]
    )
    timing_receipt = built.timing[32 : 32 + timing_vectors.FRAME_RECEIPT_V2_LEN]
    if display_receipt != timing_receipt:
        raise AssertionError("display/timing receipt equality drifted")

    outer_hash = hashlib.sha256(payload).hexdigest()
    outer_tag = content_tag(KIND, SCHEMA, payload)
    if len(payload) != EXPECTED_OUTER_LEN:
        raise AssertionError(f"outer length drifted to {len(payload)}")
    if EXPECTED_OUTER_SHA256 and outer_hash != EXPECTED_OUTER_SHA256:
        raise AssertionError("outer payload bytes drifted")
    if EXPECTED_OUTER_TAG and outer_tag != EXPECTED_OUTER_TAG:
        raise AssertionError("outer content tag drifted")

    payload_mutant = payload[:-1] + bytes((payload[-1] ^ 1,))
    # The last byte is an unavailable-topology error-code lane, so this remains
    # a valid canonical outer object while changing only payload content.
    decode_outer(payload_mutant)
    baseline_preimage = canonical_preimage(KIND, SCHEMA, payload)
    schema_preimage = canonical_preimage(KIND, OTHER_SCHEMA, payload)
    kind_preimage = canonical_preimage(OTHER_KIND, SCHEMA, payload)
    payload_preimage = canonical_preimage(KIND, SCHEMA, payload_mutant)
    baseline_header = baseline_preimage[: -len(payload)]
    if not (
        schema_preimage[-len(payload) :] == payload
        and kind_preimage[-len(payload) :] == payload
        and payload_preimage[-len(payload_mutant) :] == payload_mutant
        and schema_preimage[: -len(payload)] != baseline_header
        and kind_preimage[: -len(payload)] != baseline_header
        and payload_preimage[: -len(payload_mutant)] == baseline_header
    ):
        raise AssertionError("schema/kind/payload preimage separation failed")
    domain_tags = {
        "schema": content_tag(KIND, OTHER_SCHEMA, payload),
        "kind": content_tag(OTHER_KIND, SCHEMA, payload),
        "payload": content_tag(KIND, SCHEMA, payload_mutant),
    }
    if len({outer_tag, *domain_tags.values()}) != 4:
        raise AssertionError("schema/kind/payload mutation did not separate tags")
    for name, actual in domain_tags.items():
        expected = EXPECTED_DOMAIN_TAGS[name]
        if expected and actual != expected:
            raise AssertionError(f"{name} domain-separation tag drifted")

    # Powered fail-closed controls for each structural and nested boundary.
    expect_reject(
        "display length lane",
        replace_u64(payload, 0, len(built.display) + 1),
    )
    expect_reject(
        "pre-topology length lane",
        replace_u64(
            payload,
            layout.pre_length_offset,
            len(built.pre_topology) + 1,
        ),
    )
    expect_reject(
        "post-topology length lane",
        replace_u64(
            payload,
            layout.post_length_offset,
            len(built.post_topology) + 1,
        ),
    )
    expect_reject("trailing byte", payload + b"\x00")
    expect_reject("truncated timing", payload[: layout.pre_length_offset - 1])

    display_mutant = bytearray(payload)
    display_mutant[layout.display_start] = 2
    expect_reject("nested display marker", bytes(display_mutant))
    timing_mutant = bytearray(payload)
    timing_mutant[layout.timing_start] ^= 1
    expect_reject("nested timing policy", bytes(timing_mutant))
    pre_mutant = bytearray(payload)
    pre_mutant[layout.pre_start] = 0xFF
    expect_reject("nested pre-topology marker", bytes(pre_mutant))
    post_mutant = bytearray(payload)
    post_mutant[layout.post_start] = 0xFF
    expect_reject("nested post-topology marker", bytes(post_mutant))

    mismatched_timing = build_timing_payload(timing_vectors.frame_receipt_v2_payload())
    expect_reject(
        "display/timing receipt mismatch",
        encode_outer(
            NestedParts(
                built.display,
                mismatched_timing,
                built.pre_topology,
                built.post_topology,
            )
        ),
    )

    display_fields = display_vectors.decode_artifact(built.display)
    display_receipt = display_vectors.encode_frame_receipt(
        display_fields["receipt"]["expected_receipt"]
    )
    expect_reject(
        "display/timing QPC frequency mismatch",
        encode_outer(
            NestedParts(
                built.display,
                build_timing_payload(display_receipt, frequency=9_999_999),
                built.pre_topology,
                built.post_topology,
            )
        ),
    )
    expect_reject(
        "display capture precedes timing post-call QPC",
        encode_outer(
            NestedParts(
                built.display,
                build_timing_payload(display_receipt, post_after=101),
                built.pre_topology,
                built.post_topology,
            )
        ),
    )

    nested_values = (
        ("display", built.display),
        ("timing", built.timing),
        ("pre-topology", built.pre_topology),
        ("post-topology", built.post_topology),
    )
    for name, nested in nested_values:
        print(
            f"{name}\t{len(nested)}\t{hashlib.sha256(nested).hexdigest()}"
            f"\t{nested_tag(name, nested)}"
        )
    print(f"presentation-output\t{len(payload)}\t{outer_hash}\t{outer_tag}")
    print("layout\tfixed-overhead=972\tlength-lanes=24\tpixel-lanes=1\treceipts=equal")
    print(
        "domain-separation"
        f"\tschema={domain_tags['schema']}"
        f"\tkind={domain_tags['kind']}"
        f"\tpayload={domain_tags['payload']}"
    )
    print("negative-controls\t12\trejected")
    print(f"outer-tag\t{outer_tag}")


if __name__ == "__main__":
    main()
