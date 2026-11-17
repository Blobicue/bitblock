#!/usr/bin/env python3
"""Independent known-answer checks for CoordinateEpisodeEvidence v1.

This stdlib-only script reconstructs the named policy and one frozen canonical
episode payload without importing or launching Bitblock. It checks framing,
state markers, tag preimages, and simple structural mutations. It does not
replay Rust history, establish ownership/scarcity, or provide cryptocurrency,
consensus, Proof-of-History, Proof-of-Stake, custody, or Layer-1 evidence.
"""

from __future__ import annotations

import base64
import hashlib
import struct


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
POLICY_KIND = b"policy"
RECEIPT_KIND = b"receipt"
POLICY_SCHEMA = b"bitblock/coordinate-episode-derivation-policy/v1"
EVIDENCE_SCHEMA = b"bitblock/coordinate-episode-evidence/v1"
ACTIVE = 0
CLOSED = 1
EXPECTED = {
    "policy": "#bb.policy.sha256.qjswilslbl4opnwl7npgkycn3lapzdlovmqphgj3cvlwugkr7rbq",
    "evidence": "#bb.receipt.sha256.os4co4gqd6jed4ttwmbkjozviqpb6ly2xtczuuizobzcc7fcsplq",
}

PAYLOAD_HEX = (
    "8265642e4b0af8e7b6cbfb5e65604ddac0fc8d6eab20f3993b15576a1951fc43"
    "b5dc131097f1abb55d661a191369e521b76d1fc7ebe28e9f1ab2196788a185d5"
    "abf6806c97c3c48406ca02a8897f369d4c5c25f90cebde2e9682788eb9704d11"
    "427856465326653882c15c8437831351ad5e3096b15a715bb3ea8d51d9f1cd49"
    "00000000000000030000000000000001fffffffffffffffe0000000000000003"
    "a90a5b28e45ceeae3a04bcc0c0c3c018d4c6cd0f9595352fd03ffb3c5ef4bf59"
    "01f18488bb9c6296e730eb674b60b49998495da25c17eb7586ad3e0919e0c97f"
    "490000000000000001fffffffffffffffe0000000000000003a1e863d0daa64d"
    "b3c701812b37b2215b01f6fa96712ea1f11863254233480f0501e42cfb20df0c"
    "f626a2fd525a993969b62982612424ddf9538557daca09ffead7ffffffffffff"
    "fff9000000000000000400000000000000091aac409873690a15a8c8c53aea03"
    "5e7ad98d58301dd73f51f82a784a58627df900"
)


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


def parse_payload(payload: bytes) -> tuple[bytes, bytes, bytes, bytes, list[tuple[tuple[int, int, int], bytes, bytes | None]]]:
    if len(payload) < 136:
        raise ValueError("payload has no complete header")
    policy, genesis, head, world = (
        payload[0:32],
        payload[32:64],
        payload[64:96],
        payload[96:128],
    )
    expected_policy = content_tag(POLICY_KIND, POLICY_SCHEMA, b"")
    if policy != expected_policy:
        raise ValueError("payload derivation policy differs from v1 policy")
    count = struct.unpack(">Q", payload[128:136])[0]
    cursor = 136
    episodes = []
    seen_events = set()
    for index in range(count):
        if cursor + 57 > len(payload):
            raise ValueError(f"episode {index} is truncated")
        coord = struct.unpack(">qqq", payload[cursor : cursor + 24])
        cursor += 24
        mint = payload[cursor : cursor + 32]
        cursor += 32
        if mint in seen_events:
            raise ValueError(f"episode {index} reuses event tag")
        seen_events.add(mint)
        marker = payload[cursor]
        cursor += 1
        if marker == ACTIVE:
            burn = None
        elif marker == CLOSED:
            if cursor + 32 > len(payload):
                raise ValueError(f"closed episode {index} lacks burn tag")
            burn = payload[cursor : cursor + 32]
            cursor += 32
            if burn in seen_events:
                raise ValueError(f"episode {index} reuses event tag")
            seen_events.add(burn)
        else:
            raise ValueError(f"episode {index} has state marker {marker}")
        episodes.append((coord, mint, burn))
    if cursor != len(payload):
        raise ValueError(f"payload has {len(payload) - cursor} trailing bytes")
    return policy, genesis, head, world, episodes


def main() -> None:
    payload = bytes.fromhex(PAYLOAD_HEX)
    policy = content_tag(POLICY_KIND, POLICY_SCHEMA, b"")
    if text(POLICY_KIND, policy) != EXPECTED["policy"]:
        raise AssertionError("policy tag drift")

    policy_field, genesis, head, world, episodes = parse_payload(payload)
    if policy_field != policy:
        raise AssertionError("payload policy digest differs from derivation policy")
    if len(episodes) != 3:
        raise AssertionError("fixture must contain three episodes")
    if [episode[0] for episode in episodes] != [(1, -2, 3), (1, -2, 3), (-7, 4, 9)]:
        raise AssertionError("episode coordinate order drift")
    if [episode[2] is None for episode in episodes] != [False, False, True]:
        raise AssertionError("episode active/closed states drift")
    if episodes[0][1] == episodes[1][1]:
        raise AssertionError("re-mint must have a distinct mint tag")
    if len({episode[1] for episode in episodes}) != len(episodes):
        raise AssertionError("mint tags must be unique in this fixture")

    evidence = content_tag(RECEIPT_KIND, EVIDENCE_SCHEMA, payload)
    if text(RECEIPT_KIND, evidence) != EXPECTED["evidence"]:
        raise AssertionError("evidence tag drift")

    for label, offset in {
        "policy": 0,
        "genesis": 32,
        "history-head": 64,
        "final-world": 96,
        "first-mint": 136 + 24,
        "first-burn": 136 + 24 + 32 + 1,
    }.items():
        mutant = bytearray(payload)
        mutant[offset] ^= 1
        if content_tag(RECEIPT_KIND, EVIDENCE_SCHEMA, bytes(mutant)) == evidence:
            raise AssertionError(f"{label} mutation did not alter the content tag")

    malformed = bytearray(payload)
    malformed[136 + 24 + 32] = 9
    try:
        parse_payload(bytes(malformed))
    except ValueError as error:
        if "state marker 9" not in str(error):
            raise
    else:
        raise AssertionError("unknown episode state was accepted")

    wrong_policy = bytearray(payload)
    wrong_policy[0] ^= 1
    try:
        parse_payload(bytes(wrong_policy))
    except ValueError as error:
        if "derivation policy" not in str(error):
            raise
    else:
        raise AssertionError("wrong derivation policy was accepted")

    duplicate_event = bytearray(payload)
    mint_start = 136 + 24
    burn_start = mint_start + 32 + 1
    duplicate_event[burn_start : burn_start + 32] = duplicate_event[
        mint_start : mint_start + 32
    ]
    try:
        parse_payload(bytes(duplicate_event))
    except ValueError as error:
        if "reuses event tag" not in str(error):
            raise
    else:
        raise AssertionError("duplicate episode event was accepted")

    print(
        "coordinate-episode-vectors PASS"
        f" bytes={len(payload)} episodes={len(episodes)} evidence={text(RECEIPT_KIND, evidence)}"
    )


if __name__ == "__main__":
    main()
