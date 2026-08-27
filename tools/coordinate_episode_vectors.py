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
    "evidence": "#bb.receipt.sha256.6feyyp2ttluxp572qxdyfdzmwkgp4i3ad72usqvm32p3lc6ozo5a",
}

PAYLOAD_HEX = (
    "8265642e4b0af8e7b6cbfb5e65604ddac0fc8d6eab20f3993b15576a1951fc43"
    "5a15db8f70dc341f7c5d00864bc532fca141afd02fb3f448e22da1de7a6ce5fb"
    "ba83a2e3db2444357a7797ad9b9af451ea5a8413e46f80a5f5e5c2f797d019f"
    "853a74f35f91e4d3bbd32c5223e250c5fa3fa73f8eed7358b7f9218a88a47a89"
    "100000000000000030000000000000001fffffffffffffffe0000000000000003"
    "30bcbfb47fc0c4beb7ce43addf512b4fe30fc207983c14b9520f4b180e46b302"
    "01b5c85128686b7262178fd07cb695b8c125e7e9d885350e4b7691c107e25b8a"
    "380000000000000001fffffffffffffffe00000000000000030e500ca8077a78f"
    "4f00890e1000172e1d5407d73060c8796df5b4e7840646f9901ea42017ee5b9d"
    "adad8f7cde63dfece4d94a15a04a530a0e6344f4922b3874937fffffffffffffff"
    "9000000000000000400000000000000095e8113540f2eb826d270ae43c51979ea"
    "66960c3df7fc9a58226c7f99b26a6e9e00"
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
