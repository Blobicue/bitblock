#!/usr/bin/env python3
"""Independent stdlib KATs for Bitblock's output-topology observation codec.

This script imports no Bitblock code and never invokes Rust.  It independently reconstructs the
canonical payload, strict decoder, stable-topology projection, and content-tag preimage.
"""

from __future__ import annotations

import base64
import copy
import hashlib
import math
import struct
from typing import Any


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
KIND = b"output-topology"
SCHEMA = b"bitblock/output-topology-observation/v1"
MUTATED_SCHEMA = b"bitblock/output-topology-observation/v2"

AVAILABLE = 0
UNAVAILABLE = 1
MAX_PAYLOAD_BYTES = 65_536
MAX_UTF16_UNITS = 8_192
MAX_ROUTES = 256
MAX_ATTEMPTS = 256
MIN_ROUTE_BYTES = 87

ATTEMPT_FIELDS = (
    "operation_code_raw",
    "profile_code_raw",
    "request_flags_raw",
    "retry_ordinal",
    "outcome_code_raw",
    "error_code",
    "reported_path_capacity",
    "reported_mode_capacity",
)

# These collector-local join keys deliberately have no canonical lane.  connector_instance is a
# retained target-name fact and therefore is intentionally not in this set.
TRANSIENT_WINDOWS_IDS = frozenset(
    {
        "hmonitor",
        "adapter_luid",
        "source_id",
        "target_id",
        "source_mode_index",
        "target_mode_index",
    }
)


def u8(value: int) -> bytes:
    return struct.pack(">B", value)


def u16(value: int) -> bytes:
    return struct.pack(">H", value)


def u32(value: int) -> bytes:
    return struct.pack(">I", value)


def i32(value: int) -> bytes:
    return struct.pack(">i", value)


def u64(value: int) -> bytes:
    return struct.pack(">Q", value)


def boolean(value: bool) -> bytes:
    if not isinstance(value, bool):
        raise ValueError("boolean lane requires bool")
    return u8(int(value))


def framed(value: bytes) -> bytes:
    if len(value) > 0xFFFF:
        raise ValueError("framed field exceeds u16")
    return u16(len(value)) + value


def encode_utf16(units: tuple[int, ...]) -> bytes:
    if not units:
        raise ValueError("UTF-16 text is empty")
    if len(units) > min(MAX_UTF16_UNITS, 0xFFFF):
        raise ValueError("UTF-16 text is too long")
    if 0 in units:
        raise ValueError("UTF-16 text contains NUL")
    return u16(len(units)) + b"".join(u16(unit) for unit in units)


def encode_rect(rect: tuple[int, int, int, int]) -> bytes:
    return b"".join(i32(value) for value in rect)


def encode_size(size: tuple[int, int]) -> bytes:
    return u32(size[0]) + u32(size[1])


def encode_rational(value: tuple[int, int]) -> bytes:
    return u32(value[0]) + u32(value[1])


def rational_key(value: tuple[int, int]) -> tuple[int, int, int, int, int]:
    numerator, denominator = value
    if denominator == 0:
        normalized = (1, numerator, 0)
    else:
        divisor = math.gcd(numerator, denominator) or 1
        normalized = (0, numerator // divisor, denominator // divisor)
    return normalized + (numerator, denominator)


def option_key(value: Any, present_key: tuple[Any, ...]) -> tuple[Any, ...]:
    return (0,) if value is None else (1,) + present_key


def source_key(source: dict[str, Any]) -> tuple[Any, ...]:
    mode = source["mode"]
    return (
        tuple(source["gdi_device_name"]),
        tuple(source["client_rect"]),
        tuple(source["monitor_rect"]),
        tuple(source["work_rect"]),
        source["monitor_flags_raw"],
        mode["position_x"],
        mode["position_y"],
        mode["width"],
        mode["height"],
        mode["pixel_format_raw"],
        source["status_flags_raw"],
    )


def identity_stable_key(identity: dict[str, Any]) -> tuple[Any, ...]:
    if identity["outcome"] == "available":
        edid = identity["edid"]
        return (
            AVAILABLE,
            tuple(identity["device_path"]),
            identity["name_flags_raw"],
            option_key(edid, tuple(edid or ())),
            identity["connector_instance"],
            identity["reported_output_technology_raw"],
        )
    if identity["outcome"] == "unavailable":
        return (UNAVAILABLE, identity["reason_code_raw"], identity["error_code"])
    raise ValueError("unknown target identity outcome")


def identity_annotation_key(identity: dict[str, Any]) -> tuple[Any, ...]:
    if identity["outcome"] != "available":
        return ()
    annotation = identity["friendly_annotation"]
    return option_key(annotation, tuple(annotation or ()))


def desktop_key(desktop: dict[str, Any] | None) -> tuple[Any, ...]:
    if desktop is None:
        return (0,)
    return (
        1,
        tuple(desktop["path_source_size"]),
        tuple(desktop["desktop_image_region"]),
        tuple(desktop["desktop_image_clip"]),
    )


def stable_route_key(route: dict[str, Any]) -> tuple[Any, ...]:
    signal = route["signal"]
    return (
        identity_stable_key(route["identity"]),
        route["output_technology_raw"],
        route["target_available"],
        route["status_flags_raw"],
        route["rotation_raw"],
        route["scaling_raw"],
        rational_key(route["refresh_rate"]),
        route["path_flags_raw"],
        signal["pixel_rate"],
        rational_key(signal["h_sync"]),
        rational_key(signal["v_sync"]),
        tuple(signal["active_size"]),
        tuple(signal["total_size"]),
        signal["video_standard_raw"],
        signal["scanline_ordering_raw"],
        desktop_key(route["desktop_image"]),
    )


def canonical_route_key(route: dict[str, Any]) -> tuple[Any, ...]:
    return stable_route_key(route), identity_annotation_key(route["identity"])


def encode_acquisition(acquisition: dict[str, Any]) -> bytes:
    attempts = acquisition["attempts"]
    if not 1 <= len(attempts) <= MAX_ATTEMPTS:
        raise ValueError("noncanonical acquisition attempt count")
    result = bytearray()
    result += u32(acquisition["profile_code_raw"])
    result += u32(acquisition["request_flags_raw"])
    result += u16(len(attempts))
    for index, attempt in enumerate(attempts):
        if attempt["retry_ordinal"] != index:
            raise ValueError("noncanonical retry ordinal")
        for field in ATTEMPT_FIELDS:
            result += u32(attempt[field])
    return bytes(result)


def encode_source(source: dict[str, Any]) -> bytes:
    mode = source["mode"]
    return (
        encode_utf16(tuple(source["gdi_device_name"]))
        + encode_rect(tuple(source["client_rect"]))
        + encode_rect(tuple(source["monitor_rect"]))
        + encode_rect(tuple(source["work_rect"]))
        + u32(source["monitor_flags_raw"])
        + i32(mode["position_x"])
        + i32(mode["position_y"])
        + u32(mode["width"])
        + u32(mode["height"])
        + u32(mode["pixel_format_raw"])
        + u32(source["status_flags_raw"])
    )


def encode_identity(identity: dict[str, Any]) -> bytes:
    if identity["outcome"] == "unavailable":
        return (
            u8(UNAVAILABLE)
            + u32(identity["reason_code_raw"])
            + u32(identity["error_code"])
        )
    if identity["outcome"] != "available":
        raise ValueError("unknown target identity outcome")
    annotation = identity["friendly_annotation"]
    edid = identity["edid"]
    result = bytearray(u8(AVAILABLE))
    result += encode_utf16(tuple(identity["device_path"]))
    result += boolean(annotation is not None)
    if annotation is not None:
        result += encode_utf16(tuple(annotation))
    result += u32(identity["name_flags_raw"])
    result += boolean(edid is not None)
    if edid is not None:
        result += u16(edid[0]) + u16(edid[1])
    result += u32(identity["connector_instance"])
    result += u32(identity["reported_output_technology_raw"])
    return bytes(result)


def encode_signal(signal: dict[str, Any]) -> bytes:
    return (
        u64(signal["pixel_rate"])
        + encode_rational(tuple(signal["h_sync"]))
        + encode_rational(tuple(signal["v_sync"]))
        + encode_size(tuple(signal["active_size"]))
        + encode_size(tuple(signal["total_size"]))
        + u32(signal["video_standard_raw"])
        + u32(signal["scanline_ordering_raw"])
    )


def encode_desktop(desktop: dict[str, Any] | None) -> bytes:
    if desktop is None:
        return boolean(False)
    return (
        boolean(True)
        + encode_size(tuple(desktop["path_source_size"]))
        + encode_rect(tuple(desktop["desktop_image_region"]))
        + encode_rect(tuple(desktop["desktop_image_clip"]))
    )


def encode_route(route: dict[str, Any]) -> bytes:
    encoded = (
        encode_identity(route["identity"])
        + u32(route["output_technology_raw"])
        + boolean(route["target_available"])
        + u32(route["status_flags_raw"])
        + u32(route["rotation_raw"])
        + u32(route["scaling_raw"])
        + encode_rational(tuple(route["refresh_rate"]))
        + u32(route["path_flags_raw"])
        + encode_signal(route["signal"])
        + encode_desktop(route["desktop_image"])
    )
    if len(encoded) < MIN_ROUTE_BYTES:
        raise ValueError("route is shorter than its fixed minimum")
    return encoded


def encode_topology(facts: dict[str, Any]) -> bytes:
    if facts["outcome"] == "unavailable":
        payload = (
            u8(UNAVAILABLE)
            + encode_acquisition(facts["acquisition"])
            + u32(facts["reason_code_raw"])
            + u32(facts["error_code"])
        )
    elif facts["outcome"] == "available":
        routes = sorted(facts["routes"], key=canonical_route_key)
        if not 1 <= len(routes) <= MAX_ROUTES:
            raise ValueError("noncanonical route count")
        if any(
            stable_route_key(left) == stable_route_key(right)
            for left, right in zip(routes, routes[1:])
        ):
            raise ValueError("duplicate route")
        payload_buffer = bytearray(u8(AVAILABLE))
        payload_buffer += encode_acquisition(facts["acquisition"])
        payload_buffer += encode_source(facts["source"])
        payload_buffer += u16(len(routes))
        for route in routes:
            encoded_route = encode_route(route)
            payload_buffer += u32(len(encoded_route)) + encoded_route
        payload = bytes(payload_buffer)
    else:
        raise ValueError("unknown observation outcome")
    if not 1 <= len(payload) <= MAX_PAYLOAD_BYTES:
        raise ValueError("output-topology payload length is out of bounds")
    return payload


class Reader:
    def __init__(self, data: bytes):
        self.data = data
        self.cursor = 0

    def take(self, length: int) -> bytes:
        end = self.cursor + length
        if end > len(self.data):
            raise ValueError("unexpected end")
        result = self.data[self.cursor:end]
        self.cursor = end
        return result

    def unsigned(self, width: int) -> int:
        return int.from_bytes(self.take(width), "big")

    def signed_i32(self) -> int:
        return int.from_bytes(self.take(4), "big", signed=True)

    def marker(self) -> bool:
        value = self.unsigned(1)
        if value not in (0, 1):
            raise ValueError("unknown boolean marker")
        return bool(value)

    def finish(self) -> None:
        if self.cursor != len(self.data):
            raise ValueError("trailing bytes")


def decode_utf16(reader: Reader) -> tuple[int, ...]:
    count = reader.unsigned(2)
    if not 1 <= count <= MAX_UTF16_UNITS:
        raise ValueError("noncanonical UTF-16 count")
    units = tuple(reader.unsigned(2) for _ in range(count))
    if 0 in units:
        raise ValueError("UTF-16 text contains NUL")
    return units


def decode_rect(reader: Reader) -> tuple[int, int, int, int]:
    return tuple(reader.signed_i32() for _ in range(4))  # type: ignore[return-value]


def decode_size(reader: Reader) -> tuple[int, int]:
    return reader.unsigned(4), reader.unsigned(4)


def decode_rational(reader: Reader) -> tuple[int, int]:
    return reader.unsigned(4), reader.unsigned(4)


def decode_acquisition(reader: Reader) -> dict[str, Any]:
    result = {
        "profile_code_raw": reader.unsigned(4),
        "request_flags_raw": reader.unsigned(4),
    }
    count = reader.unsigned(2)
    if not 1 <= count <= MAX_ATTEMPTS:
        raise ValueError("noncanonical acquisition attempt count")
    attempts = []
    for index in range(count):
        attempt = {field: reader.unsigned(4) for field in ATTEMPT_FIELDS}
        if attempt["retry_ordinal"] != index:
            raise ValueError("noncanonical retry ordinal")
        attempts.append(attempt)
    result["attempts"] = attempts
    return result


def decode_source(reader: Reader) -> dict[str, Any]:
    source = {
        "gdi_device_name": decode_utf16(reader),
        "client_rect": decode_rect(reader),
        "monitor_rect": decode_rect(reader),
        "work_rect": decode_rect(reader),
        "monitor_flags_raw": reader.unsigned(4),
        "mode": {
            "position_x": reader.signed_i32(),
            "position_y": reader.signed_i32(),
            "width": reader.unsigned(4),
            "height": reader.unsigned(4),
            "pixel_format_raw": reader.unsigned(4),
        },
        "status_flags_raw": reader.unsigned(4),
    }
    return source


def decode_identity(reader: Reader) -> dict[str, Any]:
    outcome = reader.unsigned(1)
    if outcome == UNAVAILABLE:
        return {
            "outcome": "unavailable",
            "reason_code_raw": reader.unsigned(4),
            "error_code": reader.unsigned(4),
        }
    if outcome != AVAILABLE:
        raise ValueError("unknown target identity outcome")
    device_path = decode_utf16(reader)
    annotation = decode_utf16(reader) if reader.marker() else None
    name_flags_raw = reader.unsigned(4)
    edid = (reader.unsigned(2), reader.unsigned(2)) if reader.marker() else None
    return {
        "outcome": "available",
        "device_path": device_path,
        "friendly_annotation": annotation,
        "name_flags_raw": name_flags_raw,
        "edid": edid,
        "connector_instance": reader.unsigned(4),
        "reported_output_technology_raw": reader.unsigned(4),
    }


def decode_signal(reader: Reader) -> dict[str, Any]:
    return {
        "pixel_rate": reader.unsigned(8),
        "h_sync": decode_rational(reader),
        "v_sync": decode_rational(reader),
        "active_size": decode_size(reader),
        "total_size": decode_size(reader),
        "video_standard_raw": reader.unsigned(4),
        "scanline_ordering_raw": reader.unsigned(4),
    }


def decode_route(reader: Reader) -> dict[str, Any]:
    route = {
        "identity": decode_identity(reader),
        "output_technology_raw": reader.unsigned(4),
        "target_available": reader.marker(),
        "status_flags_raw": reader.unsigned(4),
        "rotation_raw": reader.unsigned(4),
        "scaling_raw": reader.unsigned(4),
        "refresh_rate": decode_rational(reader),
        "path_flags_raw": reader.unsigned(4),
        "signal": decode_signal(reader),
    }
    if reader.marker():
        route["desktop_image"] = {
            "path_source_size": decode_size(reader),
            "desktop_image_region": decode_rect(reader),
            "desktop_image_clip": decode_rect(reader),
        }
    else:
        route["desktop_image"] = None
    return route


def decode_topology(payload: bytes) -> dict[str, Any]:
    if not 1 <= len(payload) <= MAX_PAYLOAD_BYTES:
        raise ValueError("output-topology payload length is out of bounds")
    reader = Reader(payload)
    outcome = reader.unsigned(1)
    if outcome == UNAVAILABLE:
        result = {
            "outcome": "unavailable",
            "acquisition": decode_acquisition(reader),
            "reason_code_raw": reader.unsigned(4),
            "error_code": reader.unsigned(4),
        }
    elif outcome == AVAILABLE:
        acquisition = decode_acquisition(reader)
        source = decode_source(reader)
        count = reader.unsigned(2)
        if not 1 <= count <= MAX_ROUTES:
            raise ValueError("noncanonical route count")
        routes = []
        for _ in range(count):
            route_length = reader.unsigned(4)
            if not MIN_ROUTE_BYTES <= route_length <= MAX_PAYLOAD_BYTES:
                raise ValueError("noncanonical route length")
            route_reader = Reader(reader.take(route_length))
            route = decode_route(route_reader)
            route_reader.finish()
            routes.append(route)
        keys = [canonical_route_key(route) for route in routes]
        if any(left >= right for left, right in zip(keys, keys[1:])):
            raise ValueError("noncanonical route order")
        stable_keys = [stable_route_key(route) for route in routes]
        if any(left == right for left, right in zip(stable_keys, stable_keys[1:])):
            raise ValueError("duplicate material route")
        result = {
            "outcome": "available",
            "acquisition": acquisition,
            "source": source,
            "routes": routes,
        }
    else:
        raise ValueError("unknown observation outcome")
    reader.finish()
    if encode_topology(result) != payload:
        raise ValueError("noncanonical encoding")
    return result


def stable_projection(facts: dict[str, Any]) -> tuple[Any, ...] | None:
    """Mirror stable_topology_eq: Available source/routes, no acquisition or annotation."""

    if facts["outcome"] != "available":
        return None
    routes = sorted(facts["routes"], key=canonical_route_key)
    return source_key(facts["source"]), tuple(stable_route_key(route) for route in routes)


def canonical_preimage(payload: bytes, schema: bytes = SCHEMA) -> bytes:
    return (
        framed(TAG_DOMAIN)
        + framed(KIND)
        + framed(HASH_SUITE)
        + framed(schema)
        + u64(len(payload))
        + payload
    )


def content_tag(payload: bytes, schema: bytes = SCHEMA) -> str:
    digest = hashlib.sha256(canonical_preimage(payload, schema)).digest()
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.output-topology.sha256.{encoded}"


def acquisition(seed: int) -> dict[str, Any]:
    return {
        "profile_code_raw": 0xA000_0000 | seed,
        "request_flags_raw": 0xF000_0000 | seed,
        "attempts": [
            {
                "operation_code_raw": 1,
                "profile_code_raw": 2,
                "request_flags_raw": 0x8000_0000,
                "retry_ordinal": 0,
                "outcome_code_raw": 3,
                "error_code": 122,
                "reported_path_capacity": 1,
                "reported_mode_capacity": 2,
            },
            {
                "operation_code_raw": 4,
                "profile_code_raw": 2,
                "request_flags_raw": 0x8000_0000,
                "retry_ordinal": 1,
                "outcome_code_raw": 5,
                "error_code": 0,
                "reported_path_capacity": 2,
                "reported_mode_capacity": 5,
            },
        ],
    }


def source() -> dict[str, Any]:
    return {
        "gdi_device_name": (ord("\\"), ord("\\"), ord("."), ord("\\"), ord("D")),
        "client_rect": (10, 20, 810, 620),
        "monitor_rect": (0, 0, 1920, 1080),
        "work_rect": (0, 0, 1920, 1040),
        "monitor_flags_raw": 0x8000_0001,
        "mode": {
            "position_x": 0,
            "position_y": 0,
            "width": 1920,
            "height": 1080,
            "pixel_format_raw": 0xFFFF_0001,
        },
        "status_flags_raw": 0x8000_0042,
    }


def route(name: int, refresh: tuple[int, int]) -> dict[str, Any]:
    return {
        "identity": {
            "outcome": "available",
            "device_path": (0xD800, ord("P"), name),
            "friendly_annotation": (ord("M"), name),
            "name_flags_raw": 0xF0F0_00F0,
            "edid": (0x1234, name),
            "connector_instance": name,
            "reported_output_technology_raw": 0xFFFF_FF01,
        },
        "output_technology_raw": 0xFFFF_FF02,
        "target_available": True,
        "status_flags_raw": 0x8000_0001,
        "rotation_raw": 0xFFFF_0002,
        "scaling_raw": 0xFFFF_0003,
        "refresh_rate": refresh,
        "path_flags_raw": 0xDEAD_BEEF,
        "signal": {
            "pixel_rate": 148_500_000,
            "h_sync": (67_500, 1),
            "v_sync": refresh,
            "active_size": (1920, 1080),
            "total_size": (2200, 1125),
            "video_standard_raw": 0xFFFF_0010,
            "scanline_ordering_raw": 0xFFFF_0020,
        },
        "desktop_image": {
            "path_source_size": (1920, 1080),
            "desktop_image_region": (0, 0, 1920, 1080),
            "desktop_image_clip": (0, 0, 1920, 1080),
        },
    }


def available_clone() -> dict[str, Any]:
    # Deliberately supplied in reverse path order: the encoder must canonicalize the two routes.
    return {
        "outcome": "available",
        "acquisition": acquisition(1),
        "source": source(),
        "routes": [route(2, (60, 1)), route(1, (120, 2))],
    }


def unavailable() -> dict[str, Any]:
    return {
        "outcome": "unavailable",
        "acquisition": acquisition(2),
        "reason_code_raw": 7,
        "error_code": 87,
    }


def collect_clone_with_join_keys(join_keys: dict[str, int]) -> dict[str, Any]:
    if set(join_keys) != TRANSIENT_WINDOWS_IDS:
        raise ValueError("transient join-key fixture is incomplete")
    # The join is already complete at this boundary.  No collector-local key enters the facts.
    return available_clone()


def field_names(value: Any) -> set[str]:
    if isinstance(value, dict):
        names = set(value)
        for child in value.values():
            names.update(field_names(child))
        return names
    if isinstance(value, (list, tuple)):
        names: set[str] = set()
        for child in value:
            names.update(field_names(child))
        return names
    return set()


# Full canonical payload goldens, frozen from the independent construction below.
EXPECTED_PAYLOAD_HEX = {
    "available-clone": (
        "00a0000001f0000001000200000001000000028000000000000000000000030000007a0000000100"
        "00000200000004000000028000000000000001000000050000000000000002000000050005005c00"
        "5c002e005c00440000000a000000140000032a0000026c0000000000000000000007800000043800"
        "0000000000000000000780000004108000000100000000000000000000078000000438ffff000180"
        "000042000200000097000003d80000500001010002004d0001f0f000f0011234000100000001ffff"
        "ff01ffffff020180000001ffff0002ffff00030000007800000002deadbeef0000000008d9ee2000"
        "0107ac00000001000000780000000200000780000004380000089800000465ffff0010ffff002001"
        "00000780000004380000000000000000000007800000043800000000000000000000078000000438"
        "00000097000003d80000500002010002004d0002f0f000f0011234000200000002ffffff01ffffff"
        "020180000001ffff0002ffff00030000003c00000001deadbeef0000000008d9ee20000107ac0000"
        "00010000003c0000000100000780000004380000089800000465ffff0010ffff0020010000078000"
        "0004380000000000000000000007800000043800000000000000000000078000000438"
    ),
    "unavailable": (
        "01a0000002f0000002000200000001000000028000000000000000000000030000007a"
        "000000010000000200000004000000028000000000000001000000050000000000000002"
        "000000050000000700000057"
    ),
}

EXPECTED_TAGS = {
    "available-clone": (
        "#bb.output-topology.sha256."
        "jbfwsd34kns7uwofvkj6w3u7tq7nlsbqdfw5b725pxpcg44c2h2q"
    ),
    "unavailable": (
        "#bb.output-topology.sha256."
        "fw2l66z7jd2eu6suedannr77x5yrugb6sot7y3ojduamdiuemtpq"
    ),
    "acquisition-mutant": (
        "#bb.output-topology.sha256."
        "f575cglxnwk5tvgp3b5br5ptoudgh2eelaotxm2lnknlya2syvva"
    ),
    "annotation-mutant": (
        "#bb.output-topology.sha256."
        "ggxt4un2ono4r777cadzhm7yug7ynkc4hd6bp3jhdnljv6vklobq"
    ),
    "schema-mutant": (
        "#bb.output-topology.sha256."
        "xuzdu7nfzwkqxmubkawxwuimlc4gehqurydupsa4jbybhasy56la"
    ),
    "payload-mutant": (
        "#bb.output-topology.sha256."
        "ii6kcblgdmfptlxyoab67xlawzifkwe5ewoja2cfxbejdn5cpbeq"
    ),
}

EXPECTED_EMPTY_PAYLOAD_PREIMAGE_HEX = (
    "0017626974626c6f636b2f636f6e74656e742d7461672f7631000f6f75747075742d"
    "746f706f6c6f677900067368613235360027626974626c6f636b2f6f75747075742d"
    "746f706f6c6f67792d6f62736572766174696f6e2f76310000000000000000"
)


def assert_golden(name: str, payload: bytes) -> None:
    if payload.hex() != EXPECTED_PAYLOAD_HEX[name]:
        raise AssertionError(f"{name}: payload golden drift")
    if content_tag(payload) != EXPECTED_TAGS[name]:
        raise AssertionError(f"{name}: tag golden drift")


def expect_decode_reject(name: str, payload: bytes) -> None:
    try:
        decode_topology(payload)
    except ValueError:
        return
    raise AssertionError(f"negative control accepted: {name}")


def expect_encode_reject(name: str, facts: dict[str, Any]) -> None:
    try:
        encode_topology(facts)
    except ValueError:
        return
    raise AssertionError(f"negative encode control accepted: {name}")


def main() -> None:
    if canonical_preimage(b"").hex() != EXPECTED_EMPTY_PAYLOAD_PREIMAGE_HEX:
        raise AssertionError("content-tag preimage framing drift")

    available_facts = available_clone()
    unavailable_facts = unavailable()
    available_payload = encode_topology(available_facts)
    unavailable_payload = encode_topology(unavailable_facts)

    vectors = (
        ("available-clone", available_payload),
        ("unavailable", unavailable_payload),
    )
    for name, payload in vectors:
        decoded = decode_topology(payload)
        if encode_topology(decoded) != payload:
            raise AssertionError(f"{name}: strict decode/re-encode mismatch")
        assert_golden(name, payload)
        print(
            f"{name}\t{len(payload)}\t{len(canonical_preimage(payload))}"
            f"\t{hashlib.sha256(payload).hexdigest()}\t{content_tag(payload)}"
        )

    acquisition_mutant = copy.deepcopy(available_facts)
    acquisition_mutant["acquisition"] = acquisition(99)
    annotation_mutant = copy.deepcopy(available_facts)
    annotation_mutant["routes"][0]["identity"]["friendly_annotation"] = (
        ord("N"),
        2,
    )
    acquisition_payload = encode_topology(acquisition_mutant)
    annotation_payload = encode_topology(annotation_mutant)
    baseline_projection = stable_projection(available_facts)
    if not (
        baseline_projection == stable_projection(acquisition_mutant)
        and baseline_projection == stable_projection(annotation_mutant)
    ):
        raise AssertionError("stable projection included acquisition or annotation")
    if content_tag(acquisition_payload) == content_tag(available_payload):
        raise AssertionError("acquisition mutation did not alter the full tag")
    if content_tag(annotation_payload) == content_tag(available_payload):
        raise AssertionError("annotation mutation did not alter the full tag")
    for name, payload in (
        ("acquisition-mutant", acquisition_payload),
        ("annotation-mutant", annotation_payload),
    ):
        if content_tag(payload) != EXPECTED_TAGS[name]:
            raise AssertionError(f"{name}: tag golden drift")
    print(
        "stable-projection\tacquisition-tag-changed=true"
        "\tannotation-tag-changed=true\tprojection-unchanged=true"
    )

    low_ids = {
        "hmonitor": 1,
        "adapter_luid": 2,
        "source_id": 3,
        "target_id": 4,
        "source_mode_index": 5,
        "target_mode_index": 6,
    }
    high_ids = {
        "hmonitor": 0xFFFF_FFFF_FFFF_FFFF,
        "adapter_luid": 0xFEDC_BA98_7654_3210,
        "source_id": 0xFFFF_FF61,
        "target_id": 0xFFFF_FF62,
        "source_mode_index": 0xFFFF_FF63,
        "target_mode_index": 0xFFFF_FF64,
    }
    low_payload = encode_topology(collect_clone_with_join_keys(low_ids))
    high_payload = encode_topology(collect_clone_with_join_keys(high_ids))
    decoded_surface = field_names(decode_topology(available_payload))
    if not TRANSIENT_WINDOWS_IDS.isdisjoint(decoded_surface):
        raise AssertionError("a transient Windows ID leaked into the canonical surface")
    if low_payload != high_payload or content_tag(low_payload) != content_tag(high_payload):
        raise AssertionError("transient Windows IDs altered canonical bytes")
    print(f"transient-windows-ids\t{len(TRANSIENT_WINDOWS_IDS)}\tabsent")

    payload_mutant = copy.deepcopy(available_facts)
    payload_mutant["routes"][0]["path_flags_raw"] ^= 1
    mutated_payload = encode_topology(payload_mutant)
    baseline_preimage = canonical_preimage(available_payload)
    schema_preimage = canonical_preimage(available_payload, MUTATED_SCHEMA)
    payload_preimage = canonical_preimage(mutated_payload)
    baseline_header = baseline_preimage[: -len(available_payload)]
    schema_header = schema_preimage[: -len(available_payload)]
    payload_header = payload_preimage[: -len(mutated_payload)]
    if not (
        schema_preimage[-len(available_payload) :] == available_payload
        and schema_header != baseline_header
        and payload_header == baseline_header
        and mutated_payload != available_payload
    ):
        raise AssertionError("schema/payload mutation separation failed")
    schema_tag = content_tag(available_payload, MUTATED_SCHEMA)
    payload_tag = content_tag(mutated_payload)
    if schema_tag == content_tag(available_payload) or payload_tag == content_tag(
        available_payload
    ):
        raise AssertionError("mutation did not alter tag")
    if schema_tag != EXPECTED_TAGS["schema-mutant"]:
        raise AssertionError("schema-mutant: tag golden drift")
    if payload_tag != EXPECTED_TAGS["payload-mutant"]:
        raise AssertionError("payload-mutant: tag golden drift")
    print(f"mutation-separation\tschema={schema_tag}\tpayload={payload_tag}")

    expect_decode_reject("truncated payload", available_payload[:-1])
    bad_marker = bytearray(unavailable_payload)
    bad_marker[0] = 2
    expect_decode_reject("unknown observation marker", bytes(bad_marker))
    first_route_offset = 1 + len(encode_acquisition(acquisition(1))) + len(encode_source(source())) + 2
    first_route_length = int.from_bytes(
        available_payload[first_route_offset : first_route_offset + 4], "big"
    )
    first_route_end = first_route_offset + 4 + first_route_length
    second_route_length = int.from_bytes(
        available_payload[first_route_end : first_route_end + 4], "big"
    )
    second_route_end = first_route_end + 4 + second_route_length
    swapped_routes = (
        available_payload[:first_route_offset]
        + available_payload[first_route_end:second_route_end]
        + available_payload[first_route_offset:first_route_end]
    )
    expect_decode_reject("noncanonical route order", swapped_routes)

    first_duplicate = route(1, (60, 1))
    relabeled_duplicate = copy.deepcopy(first_duplicate)
    relabeled_duplicate["identity"]["friendly_annotation"] = (
        ord("O"),
        ord("t"),
        ord("h"),
    )
    duplicate_facts = {
        "outcome": "available",
        "acquisition": acquisition(1),
        "source": source(),
        "routes": [first_duplicate, relabeled_duplicate],
    }
    expect_encode_reject("annotation-distinguished material duplicate", duplicate_facts)
    duplicate_routes = sorted(duplicate_facts["routes"], key=canonical_route_key)
    duplicate_payload_buffer = bytearray(u8(AVAILABLE))
    duplicate_payload_buffer += encode_acquisition(duplicate_facts["acquisition"])
    duplicate_payload_buffer += encode_source(duplicate_facts["source"])
    duplicate_payload_buffer += u16(len(duplicate_routes))
    for duplicate_route in duplicate_routes:
        encoded_duplicate = encode_route(duplicate_route)
        duplicate_payload_buffer += u32(len(encoded_duplicate)) + encoded_duplicate
    expect_decode_reject(
        "encoded annotation-distinguished material duplicate",
        bytes(duplicate_payload_buffer),
    )
    print("negative-controls\t5\trejected")


if __name__ == "__main__":
    main()
