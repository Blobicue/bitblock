#!/usr/bin/env python3
"""Independent known-answer checks for LocalBoundsProfile v1.

Reconstructs the canonical local bounds profile payload, tag preimages,
checked preallocation arithmetic, and field sensitivity controls without
importing or launching Bitblock.
"""

from __future__ import annotations

import base64
import hashlib
import struct
import sys

TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
POLICY_KIND = b"policy"
LOCAL_BOUNDS_PROFILE_SCHEMA = b"bitblock/local-bounds-profile/v1"

DEFAULT_MAX_RECORD_BYTES = 16 * 1024 * 1024
DEFAULT_MAX_RECORDS_PER_COMMIT = 65_536
DEFAULT_MAX_STAGED_WRITE_BYTES = 64 * 1024 * 1024
DEFAULT_MAX_RETAINED_HISTORY_RECORDS = 262_144
DEFAULT_MAX_RESTART_SCAN_BYTES = 512 * 1024 * 1024
DEFAULT_MAX_BACKUP_RESTORE_BYTES = 1024 * 1024 * 1024
DEFAULT_MAX_PRUNING_BATCH_RECORDS = 4_096

EXPECTED_DEFAULT_TAG = (
    "#bb.policy.sha256.gvc7zlvkyaeidfzsivb4xdlx32wc5q7kmfmlk274cd6szzhgfx7a"
)


def field(val: bytes) -> bytes:
    return struct.pack(">H", len(val)) + val


def content_tag_render(kind: bytes, schema: bytes, payload: bytes) -> str:
    preimage = (
        field(TAG_DOMAIN)
        + field(kind)
        + field(HASH_SUITE)
        + field(schema)
        + struct.pack(">Q", len(payload))
        + payload
    )
    digest = hashlib.sha256(preimage).digest()
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.{kind.decode('ascii')}.sha256.{encoded}"


def encode_profile(
    max_record_bytes: int,
    max_records_per_commit: int,
    max_staged_write_bytes: int,
    max_retained_history_records: int,
    max_restart_scan_bytes: int,
    max_backup_restore_bytes: int,
    max_pruning_batch_records: int,
) -> bytes:
    return struct.pack(
        ">7Q",
        max_record_bytes,
        max_records_per_commit,
        max_staged_write_bytes,
        max_retained_history_records,
        max_restart_scan_bytes,
        max_backup_restore_bytes,
        max_pruning_batch_records,
    )


def check_preallocation_staged(
    profile_staged_limit: int, count: int, bytes_per_record: int
) -> int:
    total = count * bytes_per_record
    if total > 0xFFFFFFFFFFFFFFFF:
        raise OverflowError("integer overflow in preallocation")
    if total > profile_staged_limit:
        raise ValueError("staged write allocation exceeds profile limit")
    return total


def main() -> int:
    # 1. Baseline default profile reconstruction
    default_payload = encode_profile(
        DEFAULT_MAX_RECORD_BYTES,
        DEFAULT_MAX_RECORDS_PER_COMMIT,
        DEFAULT_MAX_STAGED_WRITE_BYTES,
        DEFAULT_MAX_RETAINED_HISTORY_RECORDS,
        DEFAULT_MAX_RESTART_SCAN_BYTES,
        DEFAULT_MAX_BACKUP_RESTORE_BYTES,
        DEFAULT_MAX_PRUNING_BATCH_RECORDS,
    )
    assert len(default_payload) == 56, f"expected 56 bytes, got {len(default_payload)}"

    default_tag = content_tag_render(
        POLICY_KIND, LOCAL_BOUNDS_PROFILE_SCHEMA, default_payload
    )
    assert (
        default_tag == EXPECTED_DEFAULT_TAG
    ), f"expected {EXPECTED_DEFAULT_TAG}, got {default_tag}"

    # 2. Per-lane sensitivity: each lane moved by +1 must change the tag
    baseline_fields = [
        DEFAULT_MAX_RECORD_BYTES,
        DEFAULT_MAX_RECORDS_PER_COMMIT,
        DEFAULT_MAX_STAGED_WRITE_BYTES,
        DEFAULT_MAX_RETAINED_HISTORY_RECORDS,
        DEFAULT_MAX_RESTART_SCAN_BYTES,
        DEFAULT_MAX_BACKUP_RESTORE_BYTES,
        DEFAULT_MAX_PRUNING_BATCH_RECORDS,
    ]

    seen_tags = {default_tag}
    for i in range(len(baseline_fields)):
        mutated_fields = list(baseline_fields)
        mutated_fields[i] += 1
        mutated_payload = encode_profile(*mutated_fields)
        tag = content_tag_render(
            POLICY_KIND, LOCAL_BOUNDS_PROFILE_SCHEMA, mutated_payload
        )
        assert tag not in seen_tags, f"collision on lane {i} mutation"
        seen_tags.add(tag)

    assert len(seen_tags) == 8, f"expected 8 distinct tags, got {len(seen_tags)}"

    # 3. Lane swap sensitivity: swap field 0 and 2
    swapped_fields = list(baseline_fields)
    swapped_fields[0], swapped_fields[2] = swapped_fields[2], swapped_fields[0]
    swapped_payload = encode_profile(*swapped_fields)
    swapped_tag = content_tag_render(
        POLICY_KIND, LOCAL_BOUNDS_PROFILE_SCHEMA, swapped_payload
    )
    assert (
        swapped_tag != default_tag
    ), "swapping max_record_bytes and max_staged_write_bytes must move tag"

    # 4. Checked preallocation arithmetic
    # Normal within limit
    res = check_preallocation_staged(
        DEFAULT_MAX_STAGED_WRITE_BYTES, 100, 1024
    )
    assert res == 102400

    # Exceeding limit
    try:
        check_preallocation_staged(
            DEFAULT_MAX_STAGED_WRITE_BYTES, 100_000, 1024
        )
        raise AssertionError("should have failed exceeding limit")
    except ValueError:
        pass

    # Overflow
    try:
        check_preallocation_staged(
            DEFAULT_MAX_STAGED_WRITE_BYTES, 0xFFFFFFFF, 0xFFFFFFFF
        )
        raise AssertionError("should have failed on overflow")
    except (OverflowError, ValueError):
        pass

    return 0


if __name__ == "__main__":
    sys.exit(main())
