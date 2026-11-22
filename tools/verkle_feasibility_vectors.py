#!/usr/bin/env python3
"""Independent stdlib vectors for VERKLE-R1 feasibility schemas.

Imports no Bitblock/Rust code. Independently reconstructs:
  * bitblock/verkle-r1/coord-key-32b/v1
  * bitblock/verkle-r1/vector-commitment-profile/v1
and verifies bit-exact ContentTag parity, stem/modulus non-aliasing headroom,
and fail-closed adversarial negative controls.
"""

from __future__ import annotations

import base64
import hashlib
import struct
import sys

TAG_DOMAIN = b"bitblock/content-tag/v1"
HASH_SUITE = b"sha256"
SIGN_BIAS = 1 << 63

VERKLE_COORDINATE_KEY_V1_SCHEMA = b"bitblock/verkle-r1/coord-key-32b/v1"
VERKLE_VC_PROFILE_V1_SCHEMA = b"bitblock/verkle-r1/vector-commitment-profile/v1"

GENERATOR_BASIS_DOMAIN = b"bitblock/verkle-r1/generator-basis-spec/v1"
FIAT_SHAMIR_DOMAIN = b"bitblock/verkle-r1/fiat-shamir-transcript/v1"
WORLD_ROOT_CANDIDATE_DOMAIN = b"bitblock/verkle-r1/world-root-candidate/v1"

# Official prime-order scalar moduli
BANDERSNATCH_R = (
    13108968793781547619861935127046491459309155893440570251786403306729687672801
)
BLS12_381_R = (
    52435875175126190479447740508185965837690552500527637822603658699938581184513
)

EXPECTED_WORLD_POLICY_TAG = (
    "#bb.policy.sha256.vkvqipvpzty76ylzflzkwdyuwq3u2ikjwumbtu52e45u5a6jbo3q"
)
EXPECTED_LOCAL_OP_LIMITS_TAG = (
    "#bb.policy.sha256.u72lxb77ydfzak3eamjl4v53km4yb7nfkobb5dtmmzvoqicxsaua"
)

EXPECTED_VKEY_ORIGIN_TAG = (
    "#bb.world.sha256.qwdk4qtaegdp2pbulpkpigsmji5bw7tbl4cnizizsdlco7ahpakq"
)
EXPECTED_VKEY_MIN_TAG = (
    "#bb.world.sha256.pc6425mj5b53xurcudk4xtko375gdh5d4xc7zo3amcie5j26u36a"
)
EXPECTED_VKEY_MAX_TAG = (
    "#bb.world.sha256.krl2q4b3l7emg3i6gzeoqdrzw4znfy5xajzxx73qwweihcpxtuma"
)
EXPECTED_VKEY_MIXED_TAG = (
    "#bb.world.sha256.qklzr6wcwkwgmnk63frylowslg5sbpkoq2lxktpquzotcv2zqypq"
)
EXPECTED_BANDERSNATCH_IPA_PROFILE_TAG = (
    "#bb.policy.sha256.p74jrvkyumadcmam5hrp5cum4smpcciyng2yxfmsmvqbla2lepfq"
)
EXPECTED_BLS12_381_KZG_PROFILE_TAG = (
    "#bb.policy.sha256.tr7fdslagw7eotvfuoerwjfekutud5zh7jhns6v6zwn3uteey2mq"
)


def field(val: bytes) -> bytes:
    return struct.pack(">H", len(val)) + val


def content_tag(kind: str, schema: bytes, payload: bytes) -> tuple[str, bytes]:
    preimage = (
        field(TAG_DOMAIN)
        + field(kind.encode("ascii"))
        + field(HASH_SUITE)
        + field(schema)
        + struct.pack(">Q", len(payload))
        + payload
    )
    digest = hashlib.sha256(preimage).digest()
    encoded = base64.b32encode(digest).decode("ascii").lower().rstrip("=")
    return f"#bb.{kind}.sha256.{encoded}", digest


def decode_tag_digest(tag: str) -> bytes:
    b32 = tag.split(".")[-1].upper()
    pad = "=" * ((8 - len(b32) % 8) % 8)
    return base64.b32decode(b32 + pad)


def bias64(val: int) -> int:
    return (val & 0xFFFFFFFFFFFFFFFF) ^ SIGN_BIAS


def encode_verkle_coord_key(x: int, y: int, z: int) -> bytes:
    return b"\x00" * 8 + struct.pack(">QQQ", bias64(x), bias64(y), bias64(z))


def decode_verkle_coord_key(raw: bytes) -> tuple[int, int, int] | None:
    if len(raw) != 32 or raw[:8] != b"\x00" * 8:
        return None
    ux, uy, uz = struct.unpack(">QQQ", raw[8:32])

    def unbias(u: int) -> int:
        t = u ^ SIGN_BIAS
        return t - (1 << 64) if t >= SIGN_BIAS else t

    return unbias(ux), unbias(uy), unbias(uz)


def encode_vc_profile(
    version: int,
    scheme: int,
    arity: int,
    modulus: int,
    comm_bytes: int,
    floor_bytes: int,
    max_mp: int,
    max_stems: int,
    setup_model: int,
    trusted_flag: int,
    pq_flag: int,
    subgroup_policy: int,
    proof_policy_id: bytes,
    op_limits_id: bytes,
) -> bytes:
    mod_be = modulus.to_bytes(32, "big")
    gen_dig = hashlib.sha256(
        GENERATOR_BASIS_DOMAIN + bytes([scheme]) + struct.pack(">H", arity) + mod_be
    ).digest()
    tr_dig = hashlib.sha256(
        FIAT_SHAMIR_DOMAIN + bytes([scheme]) + struct.pack(">H", arity)
    ).digest()
    key_emb_dig = hashlib.sha256(VERKLE_COORDINATE_KEY_V1_SCHEMA).digest()
    tree_dom_dig = hashlib.sha256(WORLD_ROOT_CANDIDATE_DOMAIN).digest()
    payload = (
        struct.pack(">HBH", version, scheme, arity)
        + mod_be
        + struct.pack(
            ">HIIIBBBB",
            comm_bytes,
            floor_bytes,
            max_mp,
            max_stems,
            setup_model,
            trusted_flag,
            pq_flag,
            subgroup_policy,
        )
        + gen_dig
        + tr_dig
        + key_emb_dig
        + tree_dom_dig
        + proof_policy_id
        + op_limits_id
    )
    assert len(payload) == 247
    return payload


def run_all() -> None:
    # 1. Verify injective 32-byte VerkleCoordinateKeyV1 vectors & order preservation
    vectors = [
        ((0, 0, 0), EXPECTED_VKEY_ORIGIN_TAG),
        ((-(1 << 63), -(1 << 63), -(1 << 63)), EXPECTED_VKEY_MIN_TAG),
        (((1 << 63) - 1, (1 << 63) - 1, (1 << 63) - 1), EXPECTED_VKEY_MAX_TAG),
        ((-1, 0, 1), EXPECTED_VKEY_MIXED_TAG),
    ]
    for coord, expected_tag in vectors:
        raw = encode_verkle_coord_key(*coord)
        assert len(raw) == 32
        assert decode_verkle_coord_key(raw) == coord
        stem_int = int.from_bytes(raw[:31], "big")
        assert stem_int < (1 << 184) < BANDERSNATCH_R < BLS12_381_R
        tag, _ = content_tag("world", VERKLE_COORDINATE_KEY_V1_SCHEMA, raw)
        assert tag == expected_tag, f"{coord}: got {tag}, expected {expected_tag}"

    # Verify strict signed order across boundary scalars
    scalars = [-(1 << 63), -(1 << 63) + 1, -17, -16, -1, 0, 1, 15, 16, (1 << 63) - 1]
    prev = None
    for s in scalars:
        cur = encode_verkle_coord_key(s, 0, 0)
        if prev is not None:
            assert prev < cur
        prev = cur

    # Negative controls for VerkleCoordinateKeyV1
    assert decode_verkle_coord_key(b"\x00" * 24) is None
    assert decode_verkle_coord_key(b"\x00" * 33) is None
    assert decode_verkle_coord_key(b"\x00" * 7 + b"\x01" + b"\x00" * 24) is None

    # 2. Verify self-excluding VectorCommitmentProfileDescriptorV1 golden vectors
    world_pp_id = decode_tag_digest(EXPECTED_WORLD_POLICY_TAG)
    local_op_id = decode_tag_digest(EXPECTED_LOCAL_OP_LIMITS_TAG)

    bander_payload = encode_vc_profile(
        1, 1, 256, BANDERSNATCH_R, 32, 576, 131072, 1024, 1, 0, 0, 1, world_pp_id, local_op_id
    )
    bander_tag, bander_id = content_tag("policy", VERKLE_VC_PROFILE_V1_SCHEMA, bander_payload)
    assert bander_tag == EXPECTED_BANDERSNATCH_IPA_PROFILE_TAG

    kzg_payload = encode_vc_profile(
        1, 2, 256, BLS12_381_R, 48, 48, 131072, 1024, 2, 1, 0, 1, world_pp_id, local_op_id
    )
    kzg_tag, kzg_id = content_tag("policy", VERKLE_VC_PROFILE_V1_SCHEMA, kzg_payload)
    assert kzg_tag == EXPECTED_BLS12_381_KZG_PROFILE_TAG
    assert bander_id != kzg_id

    # Negative control: flipping endianness of arity changes tag
    bad_endian = (
        bander_payload[:3] + struct.pack("<H", 256) + bander_payload[5:]
    )
    bad_tag, _ = content_tag("policy", VERKLE_VC_PROFILE_V1_SCHEMA, bad_endian)
    assert bad_tag != bander_tag

    print(
        "verkle_feasibility_vectors: ok (coord-key-32b, vector-commitment-profile, Bandersnatch/BLS12-381 headroom, and negative controls pass)"
    )


if __name__ == "__main__":
    run_all()
    sys.exit(0)
