#!/usr/bin/env python3
"""Independent stdlib Python vectors and reusable 256-bit Candidate A binary Patricia engine
for AUTH-COMMIT-1 (PS-029) and downstream Waves 14-15 Layer 1 state-producer vector suites.

Imports no Rust/Bitblock code. Reconstructs both Wave 13 canonical schemas:
  * bitblock/auth-commit-1/auth-nonce-roots/v1
  * bitblock/auth-commit-1/domain-patricia-proof/v1
"""

from __future__ import annotations

import hashlib
import struct
import sys

from crypto_d1_vectors import content_tag
from identity_1_vectors import (
    derive_actor_id,
    encode_actor_auth_state,
    encode_auth_descriptor,
    encode_threshold_policy,
    pk,
)

AUTH_COMMIT_ROOTS_SCHEMA = b"bitblock/auth-commit-1/auth-nonce-roots/v1"
AUTH_COMMIT_PROOF_SCHEMA = b"bitblock/auth-commit-1/domain-patricia-proof/v1"

EXPECTED_GENESIS_ROOTS_TAG = (
    "#bb.receipt.sha256.kbi2xxuzs6zay5pjvotpyk3edmn7dchwlh22bmcbpwfde3cyc55a"
)
EXPECTED_POST_TX_ROOTS_TAG = (
    "#bb.receipt.sha256.5mq4zzp7sm6ghxnrhiww54uu2kmj56qgkftkjkoo2eosjf5hilja"
)
EXPECTED_AUTH_INC_PROOF_TAG = (
    "#bb.receipt.sha256.nu2trkasizgtdp4mnyoom77tr2nqgylmuazaszn4ra3lwlrwvjiq"
)
EXPECTED_NONCE_ABS_PROOF_TAG = (
    "#bb.receipt.sha256.daucn3zib6opfx5dmpny25vqeift7rvitvxgsylagmhq3utgwegq"
)

DOM_AUTH = 1
DOM_NONCE = 2
DOM_NATIVE_BALANCE = 3
DOM_EPISODE_OWNERSHIP = 4
DOM_GOVERNANCE = 5
DOM_ECONOMIC_STAKE = 6
DOM_VALIDATOR_SCHEDULE = 7


def key_to_bits(key: bytes) -> list[int]:
    assert len(key) == 32
    return [(key[i >> 3] >> (7 - (i & 7))) & 1 for i in range(256)]


def encode_bitpath(bits: list[int]) -> bytes:
    byte_len = (len(bits) + 7) // 8
    buf = bytearray(byte_len)
    for i, b in enumerate(bits):
        if b:
            buf[i >> 3] |= 1 << (7 - (i & 7))
    return struct.pack(">H", len(bits)) + bytes(buf)


def lcp_len(a: list[int], b: list[int]) -> int:
    n = 0
    for x, y in zip(a, b):
        if x != y:
            break
        n += 1
    return n


def empty_domain_root(domain: int) -> bytes:
    return hashlib.sha256(b"bb.patricia.empty.v1" + bytes([domain])).digest()


def hash_leaf(domain: int, suffix: list[int], value_digest: bytes) -> bytes:
    return hashlib.sha256(
        b"bb.patricia.leaf.v1" + bytes([domain]) + encode_bitpath(suffix) + value_digest
    ).digest()


def hash_ext(domain: int, prefix: list[int], child_hash: bytes) -> bytes:
    return hashlib.sha256(
        b"bb.patricia.ext.v1" + bytes([domain]) + encode_bitpath(prefix) + child_hash
    ).digest()


def hash_branch(domain: int, left_hash: bytes, right_hash: bytes) -> bytes:
    return hashlib.sha256(
        b"bb.patricia.branch.v1" + bytes([domain]) + left_hash + right_hash
    ).digest()


class DomainPatriciaPy:
    """Pure-Python canonical 256-bit Candidate A binary Merkle Patricia Trie."""

    def __init__(self, domain: int, entries: dict[bytes, bytes] | None = None) -> None:
        self.domain = domain
        self.entries: dict[bytes, bytes] = dict(entries or {})

    def _build(self, items: list[tuple[list[int], bytes]], depth: int) -> tuple:
        if not items:
            return ("empty", empty_domain_root(self.domain))
        if len(items) == 1:
            suffix = items[0][0][depth:]
            val = items[0][1]
            vd = hashlib.sha256(val).digest()
            return ("leaf", suffix, val, vd, hash_leaf(self.domain, suffix, vd))
        first = items[0][0][depth:]
        last = items[-1][0][depth:]
        common = lcp_len(first, last)
        split_depth = depth + common
        mid = 0
        while mid < len(items) and items[mid][0][split_depth] == 0:
            mid += 1
        left = self._build(items[:mid], split_depth + 1)
        right = self._build(items[mid:], split_depth + 1)
        br = ("branch", left, right, hash_branch(self.domain, left[-1], right[-1]))
        if common > 0:
            pref = first[:common]
            return ("ext", pref, br, hash_ext(self.domain, pref, br[-1]))
        return br

    def _tree(self) -> tuple:
        items = [(key_to_bits(k), self.entries[k]) for k in sorted(self.entries.keys())]
        return self._build(items, 0)

    def root(self) -> bytes:
        return self._tree()[-1]

    def encode_proof(self, key: bytes) -> bytes:
        node = self._tree()
        kbits = key_to_bits(key)
        depth = 0
        steps: list[bytes] = []
        while True:
            kind = node[0]
            if kind == "empty":
                term = b"\x00"
                break
            if kind == "leaf":
                _, suffix, val, vd, _ = node
                if suffix == kbits[depth:]:
                    term = b"\x01" + encode_bitpath(suffix) + struct.pack(">I", len(val)) + val
                else:
                    term = b"\x02" + encode_bitpath(suffix) + vd
                break
            if kind == "ext":
                _, pref, child, _ = node
                if kbits[depth : depth + len(pref)] == pref:
                    steps.append(b"\x02" + encode_bitpath(pref))
                    depth += len(pref)
                    node = child
                else:
                    term = b"\x03" + encode_bitpath(pref) + child[-1]
                    break
            elif kind == "branch":
                _, left, right, _ = node
                if kbits[depth] == 0:
                    steps.append(b"\x01\x00" + right[-1])
                    node = left
                else:
                    steps.append(b"\x01\x01" + left[-1])
                    node = right
                depth += 1
        return struct.pack(">HB", 1, self.domain) + key + struct.pack(">H", len(steps)) + b"".join(steps) + term


def encode_auth_commit_roots(
    profile_id: bytes, block_height: int, actor_count: int, auth_root: bytes, nonce_root: bytes
) -> bytes:
    out = struct.pack(">H", 1) + profile_id + struct.pack(">QI", block_height, actor_count) + auth_root + nonce_root
    assert len(out) == 110
    return out


def main() -> int:
    chain_id = 77
    profile_id = bytes([0x33]) * 32

    desc_b = encode_auth_descriptor(
        0,
        encode_threshold_policy(0, 1, [pk(21)]),
        encode_threshold_policy(1, 1, [pk(22)]),
        [],
        False,
    )
    actor_b = derive_actor_id(chain_id, "actor-b", desc_b)

    desc_a = encode_auth_descriptor(
        0,
        encode_threshold_policy(0, 1, [pk(11)]),
        encode_threshold_policy(1, 1, [pk(12)]),
        [(actor_b, True, False, 100)],
        False,
    )
    actor_a = derive_actor_id(chain_id, "actor-a", desc_a)

    state_a_0 = encode_actor_auth_state(actor_a, desc_a, 0, encode_threshold_policy(1, 1, [pk(13)]), 100, None)
    state_b_0 = encode_actor_auth_state(actor_b, desc_b, 0, encode_threshold_policy(1, 1, [pk(23)]), 100, None)

    auth_trie = DomainPatriciaPy(DOM_AUTH, {actor_a: state_a_0, actor_b: state_b_0})
    nonce_trie = DomainPatriciaPy(DOM_NONCE, {actor_a: struct.pack(">Q", 0), actor_b: struct.pack(">Q", 0)})

    # 1. Genesis paired roots parity
    gen_roots_bytes = encode_auth_commit_roots(profile_id, 0, 2, auth_trie.root(), nonce_trie.root())
    gen_roots_tag, _ = content_tag("receipt", AUTH_COMMIT_ROOTS_SCHEMA, gen_roots_bytes)
    assert gen_roots_tag == EXPECTED_GENESIS_ROOTS_TAG, (
        f"genesis roots mismatch: {gen_roots_tag} != {EXPECTED_GENESIS_ROOTS_TAG}"
    )

    # 2. Inclusion proof parity (actor-a in AuthorizationRootV1)
    inc_proof_bytes = auth_trie.encode_proof(actor_a)
    inc_proof_tag, _ = content_tag("receipt", AUTH_COMMIT_PROOF_SCHEMA, inc_proof_bytes)
    assert inc_proof_tag == EXPECTED_AUTH_INC_PROOF_TAG, (
        f"auth inc proof mismatch: {inc_proof_tag} != {EXPECTED_AUTH_INC_PROOF_TAG}"
    )

    # 3. Absence proof parity (missing actor in NonceRootV1)
    abs_proof_bytes = nonce_trie.encode_proof(bytes([0x99]) * 32)
    abs_proof_tag, _ = content_tag("receipt", AUTH_COMMIT_PROOF_SCHEMA, abs_proof_bytes)
    assert abs_proof_tag == EXPECTED_NONCE_ABS_PROOF_TAG, (
        f"nonce abs proof mismatch: {abs_proof_tag} != {EXPECTED_NONCE_ABS_PROOF_TAG}"
    )

    # 4. Post-tx roots parity (actor-a nonce 0 -> 1 at h=10)
    state_a_1 = encode_actor_auth_state(actor_a, desc_a, 1, encode_threshold_policy(1, 1, [pk(13)]), 100, None)
    auth_trie_1 = DomainPatriciaPy(DOM_AUTH, {actor_a: state_a_1, actor_b: state_b_0})
    nonce_trie_1 = DomainPatriciaPy(DOM_NONCE, {actor_a: struct.pack(">Q", 1), actor_b: struct.pack(">Q", 0)})
    post_roots_bytes = encode_auth_commit_roots(profile_id, 10, 2, auth_trie_1.root(), nonce_trie_1.root())
    post_roots_tag, _ = content_tag("receipt", AUTH_COMMIT_ROOTS_SCHEMA, post_roots_bytes)
    assert post_roots_tag == EXPECTED_POST_TX_ROOTS_TAG, (
        f"post tx roots mismatch: {post_roots_tag} != {EXPECTED_POST_TX_ROOTS_TAG}"
    )

    # Control: domain separation ensures identical key/value maps in DOM_AUTH vs DOM_NONCE differ.
    assert DomainPatriciaPy(DOM_AUTH, {actor_a: b"x"}).root() != DomainPatriciaPy(DOM_NONCE, {actor_a: b"x"}).root()

    print(
        "auth_commit_1_vectors PASS: 4 golden tags verified across 2 schemas "
        "(AuthCommitRootsV1, DomainPatriciaProofV1)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
