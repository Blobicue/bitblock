#!/usr/bin/env python3
"""Independent known-answer reconstruction for optimization-equivalence v3."""

from __future__ import annotations

import base64
import hashlib
import struct


TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
KIND = b"receipt"
OUTPUT_SCHEMA = b"bitblock/optimization-equivalence/output/v3"
RECEIPT_SCHEMA = b"bitblock/optimization-equivalence/bounded-corpus/v3"

FIELDS = {
    "final_world": "#bb.world.sha256.6a2uby2p3md7hhmmepbl67rnefxvteb3znxsxnvozxoy4lbztqxq",
    "history": "#bb.receipt.sha256.awdyznfll2hsuap4u24kcrzg3tmuxy3bjox5wd2wumgvihial2fa",
    "frame_input": "#bb.frame-input.sha256.iu5e3slyn3txqtd5adk4ca7im2e3tcgo5q4qy5dhmvivzvdgg4wq",
    "game_frame": "#bb.frame.sha256.s2qnppnpaon4brmeosrufpurexk7mfctbiuy2rep6cckv5gxos7a",
    "game_evidence": "#bb.receipt.sha256.yr4vgqk3uz4slvbekyrsfmgf2qaspq252kcy2olqfqd6yporhhkq",
    "composite_input": "#bb.frame-input.sha256.yfmxsloxa2tg5voxvetd6ygyfflquqfpmqidz6qcpszjgesf4mca",
    "composite_frame": "#bb.frame.sha256.isqpvp4w6kox3bcrmr53hvkfqqnnsvxgdy6yb77dwqwxjdwd2wha",
    "composite_evidence": "#bb.receipt.sha256.n5y3jp4ahvhadzw7qbjxm2sxbnh7lwu4cmydyhkhvij367fshjfa",
    "display_receipt": "#bb.display-receipt.sha256.pduqk6qosatdhgbw4mlbztq4re2ixwgyzawssj3cye5miulixzuq",
    "case": "#bb.registry.sha256.6d5golms4a3vk7vqvbmvhjksrw4vaihnmnkzjg3er7fekeky67aa",
    "initial_world": "#bb.world.sha256.ziy2sdalw4i2k5uszp32ldvf57moe44kv2q2fcs6dkxwavoertca",
    "reference_impl": "#bb.registry.sha256.f26hjlfme5czij7hgkwel2dyusknugmnaqk6v2rkt24lhd4uut3a",
    "reference_policy": "#bb.policy.sha256.rkozy4jf4fvgvlu2xysp5ryoppcs5c6i6yegb2whrhiethcse7ua",
    "candidate_impl": "#bb.registry.sha256.wo3abxyl7d6vt6wmsqxscexoyfttry2vxjnp3ikqrkah2w4jmxja",
    "candidate_policy": "#bb.policy.sha256.75nzikonkjpqffay47bxldzunhxxw4s7pdmgtbs3e2lujadumbea",
}

REJECTED_V2_FIELDS = {
    **FIELDS,
    "game_evidence": "#bb.receipt.sha256.4ek6h3w4pzrxqy4xyq346eak7yjdqebncr52u57q76wzxt76agwq",
    "composite_input": "#bb.frame-input.sha256.fkf3ssmqb3jwku2dnhpsuedymkq6ehzvudynclkyrtueek7j7wia",
    "composite_evidence": "#bb.receipt.sha256.2dcwjak2vw437crjudmbywsjspl5qkfqmx7j7ncoaakwtflv67ia",
    "display_receipt": "#bb.display-receipt.sha256.zo52e2varem4pr4hz3sajgkweouvrhnfg3fmdby3esuvxjixnunq",
}

EXPECTED_OUTPUT = "#bb.receipt.sha256.ta633j5g7y5zhrxht5qml6sejmbjcdrsvd37whc4ykn3hgwgbs5q"
EXPECTED_RECEIPT = "#bb.receipt.sha256.haywsy6jofj3k6gehasulniebg2kvzpdzoxwpys7fejucsaujmpa"
REJECTED_V2_OUTPUT = "#bb.receipt.sha256.yclrpejhfo4pop4ldvowaomk4ywsfbodxj7436iil5amq7n7mkyq"
REJECTED_V2_RECEIPT = "#bb.receipt.sha256.wxmct7gif5ygiueryxymilcsfdlwoncaya5ucxqj65tuvxqr5zta"


def be16(value: bytes) -> bytes:
    return struct.pack(">H", len(value)) + value


def digest(tag: str) -> bytes:
    encoded = tag.rsplit(".", 1)[1]
    result = base64.b32decode(encoded.upper() + "=" * ((8 - len(encoded) % 8) % 8))
    if len(result) != 32:
        raise AssertionError(f"non-SHA-256 tag: {tag}")
    return result


def content_tag(schema: bytes, payload: bytes) -> str:
    preimage = (
        be16(TAG_DOMAIN)
        + be16(KIND)
        + be16(HASH_SUITE)
        + be16(schema)
        + struct.pack(">Q", len(payload))
        + payload
    )
    encoded = base64.b32encode(hashlib.sha256(preimage).digest()).decode("ascii")
    return "#bb.receipt.sha256." + encoded.lower().rstrip("=")


def output_payload(fields: dict[str, str] = FIELDS) -> bytes:
    payload = b"".join(
        digest(fields[name])
        for name in (
            "final_world",
            "history",
            "frame_input",
            "game_frame",
            "game_evidence",
        )
    )
    payload += b"\x01"
    payload += b"".join(
        digest(fields[name])
        for name in ("composite_input", "composite_frame", "composite_evidence")
    )
    payload += b"\x01" + digest(fields["display_receipt"])
    assert len(payload) == 290
    return payload


def receipt_payload(output: str) -> bytes:
    payload = b"".join(
        digest(FIELDS[name])
        for name in (
            "case",
            "initial_world",
            "reference_impl",
            "reference_policy",
            "candidate_impl",
            "candidate_policy",
        )
    )
    payload += digest(output) + digest(output)
    assert len(payload) == 256
    return payload


def main() -> None:
    output_bytes = output_payload()
    output = content_tag(OUTPUT_SCHEMA, output_bytes)
    assert output == EXPECTED_OUTPUT
    receipt_bytes = receipt_payload(output)
    receipt = content_tag(RECEIPT_SCHEMA, receipt_bytes)
    assert receipt == EXPECTED_RECEIPT

    # Powered domain/field controls: each must move the admitted tag.
    assert content_tag(b"bitblock/optimization-equivalence/output/v1", output_bytes) != output
    rejected_v2_output_bytes = output_payload(REJECTED_V2_FIELDS)
    assert (
        content_tag(
            b"bitblock/optimization-equivalence/output/v2", rejected_v2_output_bytes
        )
        == REJECTED_V2_OUTPUT
    )
    assert content_tag(
        b"bitblock/optimization-equivalence/bounded-corpus/v1", receipt_bytes
    ) != receipt
    rejected_v2_receipt_bytes = receipt_payload(REJECTED_V2_OUTPUT)
    assert (
        content_tag(
            b"bitblock/optimization-equivalence/bounded-corpus/v2",
            rejected_v2_receipt_bytes,
        )
        == REJECTED_V2_RECEIPT
    )
    no_composite = output_bytes[:160] + b"\x00" + bytes(96) + output_bytes[257:]
    assert len(no_composite) == 290 and content_tag(OUTPUT_SCHEMA, no_composite) != output
    swapped = bytearray(output_bytes)
    swapped[128:160], swapped[225:257] = swapped[225:257], swapped[128:160]
    assert content_tag(OUTPUT_SCHEMA, bytes(swapped)) != output

    print(
        "output\t290\t"
        f"{hashlib.sha256(output_bytes).hexdigest()}\t{output}"
    )
    print(
        "receipt\t256\t"
        f"{hashlib.sha256(receipt_bytes).hexdigest()}\t{receipt}"
    )
    print("negative-controls\t6")


if __name__ == "__main__":
    main()
