#!/usr/bin/env python3
"""Independent pure-Python reference implementation and vector suite for RESOURCE-1."""

from __future__ import annotations

import struct
import sys

from crypto_d1_vectors import content_tag
from profile_bootstrap_vectors import decode_tag_digest as tag_digest

POLICY_SCHEMA = b"bitblock/resource-1/fee-policy/v1"
SETTLEMENT_SCHEMA = b"bitblock/resource-1/block-settlement/v1"

# Canonical pinned upstream digests from BOUNDS-1 (PR #34) and ASSET-D1 (PR #26).
BOUNDS_1_PROFILE_TAG = "#bb.policy.sha256.yecxf5k5a5gs3y7qm76pzax2f5e6bkidmxytmsthsxhdqozzplaa"
ASSET_D1_POLICY_TAG = "#bb.policy.sha256.nxy3syo3wcq5ch7ayryyut25sbyustu4snj5vphox52fsnnlo4ca"


def pack_u128(v: int) -> bytes:
    return v.to_bytes(16, "big", signed=False)


def pack_i128(v: int) -> bytes:
    return v.to_bytes(16, "big", signed=True)


def encode_resource_policy(
    bounds_digest: bytes,
    asset_digest: bytes,
    gas_tx_base: int = 21_000,
    gas_tx_byte: int = 16,
    gas_sig: int = 35_000,
    gas_write: int = 12_000,
    gas_proof_byte: int = 32,
    gas_op: int = 2_000,
    target_gas: int = 50_000_000,
    max_gas: int = 100_000_000,
    denom: int = 8,
    min_bf: int = 1,
    max_bf: int = 1_000_000_000,
    max_tip: int = 100_000_000,
    deposit: int = 1_000_000,
) -> bytes:
    flags = 1 | 2  # burn_base_fee | charge_admitted_failure; state_rent=0
    out = bytearray()
    out += struct.pack(">H", 1)
    out += bounds_digest
    out += asset_digest
    out += struct.pack(
        ">IIIIIIQQH",
        gas_tx_base,
        gas_tx_byte,
        gas_sig,
        gas_write,
        gas_proof_byte,
        gas_op,
        target_gas,
        max_gas,
        denom,
    )
    out += pack_u128(min_bf)
    out += pack_u128(max_bf)
    out += pack_u128(max_tip)
    out += pack_u128(deposit)
    out += struct.pack(">B", flags)
    assert len(out) == 173
    return bytes(out)


def next_base_fee(
    prev_bf: int,
    gas_used: int,
    target_gas: int = 50_000_000,
    denom: int = 8,
    min_bf: int = 1,
    max_bf: int = 1_000_000_000,
) -> int:
    prev = max(min_bf, min(max_bf, prev_bf))
    if gas_used == target_gas:
        return prev
    if gas_used > target_gas:
        inc = max(1, (prev * (gas_used - target_gas)) // target_gas // denom)
        return min(max_bf, prev + inc)
    dec = (prev * (target_gas - gas_used)) // target_gas // denom
    return max(min_bf, prev - dec)


def meter_work(tx_bytes: int, sigs: int, writes: int, proof_bytes: int, ops: int) -> int:
    return (
        21_000
        + tx_bytes * 16
        + sigs * 35_000
        + writes * 12_000
        + proof_bytes * 32
        + ops * 2_000
    )


def encode_settlement(
    policy_digest: bytes,
    block_height: int,
    proposer: bytes,
    base_fee: int,
    next_bf: int,
    block_gas: int,
    total_burned: int,
    total_tips: int,
    net_deposit_delta: int,
    charges: list[tuple[bytes, bool, int, int, int, int, int]],
) -> bytes:
    out = bytearray()
    out += policy_digest
    out += struct.pack(">Q", block_height)
    out += proposer
    out += pack_u128(base_fee)
    out += pack_u128(next_bf)
    out += struct.pack(">Q", block_gas)
    out += pack_u128(total_burned)
    out += pack_u128(total_tips)
    out += pack_i128(net_deposit_delta)
    out += struct.pack(">I", len(charges))
    for payer, ok, gas, burned, tip, locked, refunded in charges:
        out += payer
        out += struct.pack(">BQ", 1 if ok else 0, gas)
        out += pack_u128(burned)
        out += pack_u128(tip)
        out += pack_u128(locked)
        out += pack_u128(refunded)
    return bytes(out)


def main() -> int:
    bounds_d = tag_digest(BOUNDS_1_PROFILE_TAG)
    asset_d = tag_digest(ASSET_D1_POLICY_TAG)
    pol_bytes = encode_resource_policy(bounds_d, asset_d)
    pol_tag, pol_digest = content_tag("policy", POLICY_SCHEMA, pol_bytes)

    # Golden block settlement fixture matching tests/wave7_resource_1.rs
    alice = bytes([0x11] * 32)
    bob = bytes([0x22] * 32)
    proposer = bytes([0x99] * 32)
    base_fee = 100
    g1 = meter_work(512, 2, 4, 512, 4)
    g2 = meter_work(256, 1, 2, 256, 2)
    b1, t1 = g1 * base_fee, g1 * 15
    b2, t2 = g2 * base_fee, g2 * 10
    block_gas = g1 + g2
    nbf = next_base_fee(base_fee, block_gas)
    charges = [
        (alice, True, g1, b1, t1, 4_000_000, 0),
        (bob, False, g2, b2, t2, 0, 0),
    ]
    settle_bytes = encode_settlement(
        pol_digest,
        1,
        proposer,
        base_fee,
        nbf,
        block_gas,
        b1 + b2,
        t1 + t2,
        4_000_000,
        charges,
    )
    settle_tag, _ = content_tag("receipt", SETTLEMENT_SCHEMA, settle_bytes)
    assert pol_tag == "#bb.policy.sha256.6yvp73mvdkouoxk2xiolnj33lbxucxta5ikhjcwnvd267tzfrfyq", pol_tag
    assert settle_tag == "#bb.receipt.sha256.5n46g22vwa6oby2idk57kterg5sra3rx6y5xrf4hbjkaqgm6b7fa", settle_tag

    # Independent EIP-1559 32-block saturation + 32-block decay trajectory sanity checks
    bf = 100
    for _ in range(32):
        bf = next_base_fee(bf, 100_000_000)
    assert bf > 3_500, f"expected surge > 3500, got {bf}"
    peak_bf = bf
    for _ in range(32):
        bf = next_base_fee(bf, 0)
    assert bf < peak_bf // 20, f"expected sharp decay, got {bf} from {peak_bf}"

    if "--print-tags" in sys.argv:
        print(f"POLICY_TAG={pol_tag}")
        print(f"SETTLEMENT_TAG={settle_tag}")
        print(f"PEAK_32_BF={peak_bf}")
        print(f"DECAY_32_BF={bf}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
