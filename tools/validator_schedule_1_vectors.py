#!/usr/bin/env python3
"""Independent stdlib Python vectors for VALIDATOR-SCHEDULE-1 (PS-055).

Imports no Rust/Bitblock code. Reuses DomainPatriciaPy from auth_commit_1_vectors
and independently reconstructs both Wave 15C canonical schemas:
  * bitblock/validator-schedule-1/schedule-root/v1
  * bitblock/validator-schedule-1/epoch-schedule/v1
"""

from __future__ import annotations

import hashlib
import struct
import sys

from auth_commit_1_vectors import (
    DOM_ECONOMIC_STAKE,
    DOM_GOVERNANCE,
    DOM_VALIDATOR_SCHEDULE,
    DomainPatriciaPy,
)
from crypto_d1_vectors import content_tag
from economic_state_1_vectors import encode_econ_root, encode_val_record, u128_be
from governance_1_vectors import encode_gov_root, param_trie_key
from identity_1_vectors import pk

SCHED_ROOT_SCHEMA = b"bitblock/validator-schedule-1/schedule-root/v1"
SCHED_EPOCH_SCHEMA = b"bitblock/validator-schedule-1/epoch-schedule/v1"

EXPECTED_GEN_ROOT_TAG = (
    "#bb.receipt.sha256.3odndsylyhbqu3zinnag6r5zc7nveqfui2nmhcl6544elyfv56yq"
)
EXPECTED_EP2_SCHED_TAG = (
    "#bb.receipt.sha256.jpxuitjyubnexfutl57v2ypfehtlyynd2r27zflm7otqih6hxmca"
)
EXPECTED_POST_ROOT_TAG = (
    "#bb.receipt.sha256.3ht2hivyqt4fijsvojhzyadzy3lzfajb3xlyy53jhboq4l7mbkta"
)


def epoch_trie_key(effective_epoch: int) -> bytes:
    return hashlib.sha256(b"bb.vs1.epoch_key.v1" + struct.pack(">Q", effective_epoch)).digest()


def encode_epoch_schedule(
    effective_epoch: int,
    source_epoch: int,
    econ_sha: bytes,
    gov_sha: bytes,
    max_bps: int,
    validators: list[tuple[bytes, bytes, bytes, int]],
) -> bytes:
    total = sum(w for _, _, _, w in validators)
    out = (
        struct.pack(">HQQ", 1, effective_epoch, source_epoch)
        + econ_sha
        + gov_sha
        + u128_be(total)
        + struct.pack(">HH", max_bps, len(validators))
    )
    for vid, vk, prk, w in validators:
        out += vid + vk + prk + u128_be(w)
    assert len(out) == 102 + len(validators) * 112
    return out


def encode_schedule_root(
    profile_id: bytes,
    current_epoch: int,
    latest_epoch: int,
    delay: int,
    count: int,
    active_count: int,
    max_bps: int,
    trie_root: bytes,
) -> bytes:
    out = (
        struct.pack(">H", 1)
        + profile_id
        + struct.pack(">QQQIHH", current_epoch, latest_epoch, delay, count, active_count, max_bps)
        + trie_root
    )
    assert len(out) == 98
    return out


def main() -> int:
    profile_id = bytes([0x77]) * 32
    stake = 100_000_000_000
    v = [bytes([i]) * 32 for i in (1, 2, 3, 4, 5)]

    # Governance genesis root sha
    gov_t0 = DomainPatriciaPy(DOM_GOVERNANCE, {param_trie_key(1): struct.pack(">Q", 50_000_000)})
    gov_r0 = encode_gov_root(profile_id, 0, 0, 0, 0, 0, bytes(32), 0, 1, gov_t0.root())
    gov_sha = hashlib.sha256(gov_r0).digest()

    # Economic genesis root sha (4 validators)
    econ_t0 = DomainPatriciaPy(
        DOM_ECONOMIC_STAKE,
        {v[i]: encode_val_record(stake, 0, 0, False, 0, []) for i in range(4)},
    )
    econ_r0 = encode_econ_root(
        profile_id, 0, 4, 4, 4 * stake, 0, 0, 0, 0, 4 * stake, 0, 0, econ_t0.root()
    )
    econ_sha0 = hashlib.sha256(econ_r0).digest()

    vals4 = [(v[i], pk(11 + i), pk(21 + i), stake) for i in range(4)]
    ep0_bytes = encode_epoch_schedule(0, 0, econ_sha0, gov_sha, 2_500, vals4)
    ep1_bytes = encode_epoch_schedule(1, 0, econ_sha0, gov_sha, 2_500, vals4)

    sched_t0 = DomainPatriciaPy(
        DOM_VALIDATOR_SCHEDULE,
        {epoch_trie_key(0): ep0_bytes, epoch_trie_key(1): ep1_bytes},
    )
    gen_root_bytes = encode_schedule_root(profile_id, 0, 1, 2, 2, 4, 2_500, sched_t0.root())
    gen_root_tag, _ = content_tag("receipt", SCHED_ROOT_SCHEMA, gen_root_bytes)

    # Economic post-V5-bond root sha (5 validators at epoch 0)
    econ_t1 = DomainPatriciaPy(
        DOM_ECONOMIC_STAKE,
        {v[i]: encode_val_record(stake, 0, 0, False, 0, []) for i in range(5)},
    )
    econ_r1 = encode_econ_root(
        profile_id, 0, 5, 5, 5 * stake, 0, 0, 0, 0, 4 * stake, stake, 0, econ_t1.root()
    )
    econ_sha1 = hashlib.sha256(econ_r1).digest()
    vals5 = [(v[i], pk(11 + i), pk(21 + i), stake) for i in range(5)]
    ep2_bytes = encode_epoch_schedule(2, 0, econ_sha1, gov_sha, 2_000, vals5)
    ep2_tag, _ = content_tag("receipt", SCHED_EPOCH_SCHEMA, ep2_bytes)

    sched_t1 = DomainPatriciaPy(
        DOM_VALIDATOR_SCHEDULE,
        {
            epoch_trie_key(0): ep0_bytes,
            epoch_trie_key(1): ep1_bytes,
            epoch_trie_key(2): ep2_bytes,
        },
    )
    post_root_bytes = encode_schedule_root(profile_id, 2, 2, 2, 3, 5, 2_000, sched_t1.root())
    post_root_tag, _ = content_tag("receipt", SCHED_ROOT_SCHEMA, post_root_bytes)

    if "--print" in sys.argv:
        print("GEN_ROOT: ", gen_root_tag)
        print("EP2_SCHED:", ep2_tag)
        print("POST_ROOT:", post_root_tag)
        return 0

    assert gen_root_tag == EXPECTED_GEN_ROOT_TAG, f"{gen_root_tag} != {EXPECTED_GEN_ROOT_TAG}"
    assert ep2_tag == EXPECTED_EP2_SCHED_TAG, f"{ep2_tag} != {EXPECTED_EP2_SCHED_TAG}"
    assert post_root_tag == EXPECTED_POST_ROOT_TAG, f"{post_root_tag} != {EXPECTED_POST_ROOT_TAG}"

    print(
        "validator_schedule_1_vectors PASS: 3 golden tags verified across 2 schemas "
        "(ValidatorScheduleRootV1, EpochValidatorScheduleV1)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
