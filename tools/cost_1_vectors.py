#!/usr/bin/env python3
"""Independent clean-room Python vector verifier for COST-1 platform cost envelopes.

Reconstructs canonical big-endian encodings and ContentTags for:
  - bitblock/cost-1/platform-budget-profile/v1 (46 bytes, TagKind::Policy)
  - bitblock/cost-1/measurement-receipt/v1     (111 bytes, TagKind::Receipt)

Zero Rust imports; stdlib only.
"""

from __future__ import annotations

import struct

from crypto_d1_vectors import content_tag

COST_1_PLATFORM_BUDGET_PROFILE_SCHEMA = b"bitblock/cost-1/platform-budget-profile/v1"
COST_1_MEASUREMENT_RECEIPT_SCHEMA = b"bitblock/cost-1/measurement-receipt/v1"

SECONDS_PER_YEAR = 365 * 24 * 60 * 60
BYTES_PER_MIB = 1_048_576

EXPECTED_PROFILE_TAG = "#bb.policy.sha256.2lyhmz5uypgxkhwjmvlho7psyiaoc7glzrhtcx4gyikqqgtohrkq"
EXPECTED_RECEIPT_TAG = "#bb.receipt.sha256.us2uwb6mpt6tvt4yd6it7es6jhnkhwqgw23xloik2krw6iiv3cuq"


def encode_budget_profile(
    *,
    version: int = 1,
    max_light_wasm_gzip_bytes: int = 256_000,
    max_phone_session_bytes: int = 1_048_576,
    max_wallet_proof_bytes: int = 12_288,
    max_purchase_tx_wire_bytes: int = 512,
    max_receipt_wire_bytes: int = 256,
    max_verify_wallet_proof_us: int = 500,
    max_apply_purchase_split_us: int = 1_000,
    max_block_verify_budget_ms: int = 400,
    max_annual_mib_at_10tps: int = 262_144,
) -> bytes:
    wire = struct.pack(
        ">HIIIIIQQII",
        version,
        max_light_wasm_gzip_bytes,
        max_phone_session_bytes,
        max_wallet_proof_bytes,
        max_purchase_tx_wire_bytes,
        max_receipt_wire_bytes,
        max_verify_wallet_proof_us,
        max_apply_purchase_split_us,
        max_block_verify_budget_ms,
        max_annual_mib_at_10tps,
    )
    assert len(wire) == 46
    return wire


def annual_mib_at_tps(bytes_per_tx: int, tps: int) -> int:
    total = bytes_per_tx * tps * SECONDS_PER_YEAR
    return (total + BYTES_PER_MIB - 1) // BYTES_PER_MIB


def encode_measurement_receipt(
    *,
    version: int,
    profile_digest: bytes,
    disposition: int,
    light_wasm_gzip_bytes: int,
    purchase_tx_wire_bytes: int,
    receipt_wire_bytes: int,
    wallet_inclusion_proof_bytes: int,
    wallet_absence_proof_bytes: int,
    phone_session_door_bytes: int,
    active_accounts_sampled: int,
    projected_annual_mib_at_10tps: int,
    apply_purchase_split_p50_us: int,
    apply_purchase_split_max_us: int,
    verify_wallet_proof_p50_us: int,
    verify_wallet_proof_max_us: int,
    apply_grant_p50_us: int,
    powered_controls_mask: int,
) -> bytes:
    assert len(profile_digest) == 32
    wire = struct.pack(
        ">H32sBIIIIIIIIQQQQQI",
        version,
        profile_digest,
        disposition,
        light_wasm_gzip_bytes,
        purchase_tx_wire_bytes,
        receipt_wire_bytes,
        wallet_inclusion_proof_bytes,
        wallet_absence_proof_bytes,
        phone_session_door_bytes,
        active_accounts_sampled,
        projected_annual_mib_at_10tps,
        apply_purchase_split_p50_us,
        apply_purchase_split_max_us,
        verify_wallet_proof_p50_us,
        verify_wallet_proof_max_us,
        apply_grant_p50_us,
        powered_controls_mask,
    )
    assert len(wire) == 111
    return wire


def main() -> None:
    prof_wire = encode_budget_profile()
    prof_tag, prof_digest = content_tag(
        "policy", COST_1_PLATFORM_BUDGET_PROFILE_SCHEMA, prof_wire
    )
    assert prof_tag == EXPECTED_PROFILE_TAG, f"profile tag mismatch: {prof_tag}"

    projected_mib = annual_mib_at_tps(367 + 236, 10)
    assert projected_mib == 181_353, f"unexpected annual MiB: {projected_mib}"

    rec_wire = encode_measurement_receipt(
        version=1,
        profile_digest=prof_digest,
        disposition=0,  # Met
        light_wasm_gzip_bytes=149_181,
        purchase_tx_wire_bytes=367,
        receipt_wire_bytes=236,
        wallet_inclusion_proof_bytes=460,
        wallet_absence_proof_bytes=380,
        phone_session_door_bytes=30_720,
        active_accounts_sampled=64,
        projected_annual_mib_at_10tps=projected_mib,
        apply_purchase_split_p50_us=120,
        apply_purchase_split_max_us=210,
        verify_wallet_proof_p50_us=15,
        verify_wallet_proof_max_us=32,
        apply_grant_p50_us=95,
        powered_controls_mask=0x3F,
    )
    rec_tag, _ = content_tag(
        "receipt", COST_1_MEASUREMENT_RECEIPT_SCHEMA, rec_wire
    )
    assert rec_tag == EXPECTED_RECEIPT_TAG, f"receipt tag mismatch: {rec_tag}"

    # Negative control: flipping 1 byte of the budget profile changes the tag.
    mut_prof_tag, _ = content_tag(
        "policy",
        COST_1_PLATFORM_BUDGET_PROFILE_SCHEMA,
        encode_budget_profile(max_wallet_proof_bytes=12_287),
    )
    assert mut_prof_tag != EXPECTED_PROFILE_TAG
    print("cost_1_vectors: ok (2/2 canonical tags matched byte-for-byte)")


if __name__ == "__main__":
    main()
