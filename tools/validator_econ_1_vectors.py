#!/usr/bin/env python3
"""Clean-room pure-Python independent verifier & vector suite for VALIDATOR-ECON-1 (PS-041).

Imports NO Rust or Bitblock code. Independently reproduces:
  * bitblock/validator-econ-1/consensus-adapter-receipt/v1 (443-byte consensus-to-economic receipt)
  * bitblock/validator-econ-1/adversarial-assay/v1 (196-byte 7-vector adversarial incentive assay)
  * Negative controls: produces_execution_root_domain=true refused, block_token_used_as_stake=true
    refused, and 1-bit flip tag separation.
"""

from __future__ import annotations

import hashlib
import struct
import sys

from auth_commit_1_vectors import DOM_ECONOMIC_STAKE, DomainPatriciaPy
from commit_state_vectors import EXPECTED_DOMAIN_REG_TAG
from consensus_1_vectors import (
    CONTRACT_SCHEMA,
    SCOPE_SCHEMA,
    THREAT_MODEL_SCHEMA,
    build_consensus_1_vectors,
    decode_tag_digest,
    encode_base_contract,
    encode_threat_model,
)
from crypto_d1_vectors import content_tag, ed_public_from_seed, encode_suite_descriptor
from economic_state_1_vectors import (
    encode_econ_receipt,
    encode_econ_root,
    encode_val_record,
    u128_be,
)
from genesis_d1_vectors import LAUNCH_PREDICATES
from pre_genesis_d1_vectors import (
    ASSET_POLICY_SCHEMA,
    CONSENSUS_POLICY_SCHEMA,
    GOVERNANCE_POLICY_SCHEMA,
    VALIDATOR_ECON_POLICY_SCHEMA,
    encode_asset_policy,
    encode_consensus_policy,
    encode_governance_policy,
    encode_validator_econ_policy,
)

ADAPTER_RECEIPT_SCHEMA = b"bitblock/validator-econ-1/consensus-adapter-receipt/v1"
ADVERSARIAL_ASSAY_SCHEMA = b"bitblock/validator-econ-1/adversarial-assay/v1"
VECON_ASSAY_TRANSCRIPT_PREFIX = b"bb.vecon1.assay_transcript.v1"


def build_validator_econ_1_vectors() -> tuple[str, str]:
    _, _, ctx = build_consensus_1_vectors()
    genesis_id = ctx["genesis_id"]
    profile_id = ctx["profile_id"]
    block_id = ctx["block_id"]
    post_ref = ctx["post_ref"]
    gen_vals = ctx["gen_vals"]

    # Recompute exact policy_tag_digest and threat_model_digest independently
    _, base_spec_d = content_tag("policy", CONTRACT_SCHEMA, encode_base_contract([]))
    commit_dom_reg_d = decode_tag_digest(EXPECTED_DOMAIN_REG_TAG)
    _, crypto_d = content_tag("policy", b"bitblock/crypto-d1/suite-descriptor/v1", encode_suite_descriptor())
    _, asset_d = content_tag("policy", ASSET_POLICY_SCHEMA, encode_asset_policy())
    _, econ_policy_d = content_tag("policy", VALIDATOR_ECON_POLICY_SCHEMA, encode_validator_econ_policy())
    _, cons_d = content_tag("policy", CONSENSUS_POLICY_SCHEMA, encode_consensus_policy())
    council_keys = sorted(ed_public_from_seed(bytes([s]) * 32) for s in (31, 32, 33))
    _, gov_d = content_tag("policy", GOVERNANCE_POLICY_SCHEMA, encode_governance_policy(council_keys))

    scope_b = bytearray(struct.pack(">H", 1))
    scope_b += base_spec_d + commit_dom_reg_d + crypto_d + asset_d + econ_policy_d + cons_d + gov_d
    scope_b += struct.pack(">IIIIIH", 16_384, 1_024, 65_536, 12_288, 65_536, len(LAUNCH_PREDICATES))
    for p in LAUNCH_PREDICATES:
        pb = p.encode("utf-8")
        scope_b += struct.pack(">H", len(pb)) + pb
    _, scope_d = content_tag("policy", SCOPE_SCHEMA, bytes(scope_b))
    _, threat_d = content_tag("policy", THREAT_MODEL_SCHEMA, encode_threat_model(scope_d))

    # 1. Reconstruct exact pre & post ECONOMIC-STATE-1 root + receipt for Epoch 1 reward budget=999_999_999
    stake = 100_000_000_000
    total_stake = stake * 4
    pre_trie = DomainPatriciaPy(
        DOM_ECONOMIC_STAKE,
        {vid: encode_val_record(s, 0, 0, False, 0, []) for vid, _, _, s in gen_vals},
    ).root()
    pre_econ_sha = hashlib.sha256(
        encode_econ_root(profile_id, 0, 4, 4, total_stake, 0, 0, 0, 0, total_stake, 0, 0, pre_trie)
    ).digest()

    share = 999_999_999 // 4  # 249_999_999
    issued = share * 4  # 999_999_996
    dust = 999_999_999 - issued  # 3
    post_trie = DomainPatriciaPy(
        DOM_ECONOMIC_STAKE,
        {vid: encode_val_record(s, share, 0, False, 0, []) for vid, _, _, s in gen_vals},
    ).root()
    post_econ_sha = hashlib.sha256(
        encode_econ_root(
            profile_id, 1, 4, 4, total_stake, 0, issued, 0, 0, total_stake, 0, issued, post_trie
        )
    ).digest()

    subj_ep1 = hashlib.sha256(struct.pack(">Q", 1)).digest()
    econ_rcpt_bytes = encode_econ_receipt(
        profile_id, 1, 4, 0, issued, dust, subj_ep1, pre_econ_sha, post_econ_sha
    )
    econ_rcpt_sha = hashlib.sha256(econ_rcpt_bytes).digest()

    # ValidatorEconAdapterReceiptV1 (443 bytes)
    adapter_bytes = (
        struct.pack(">H", 1)
        + genesis_id
        + profile_id
        + post_ref
        + block_id
        + struct.pack(">QQB", 1, 1, 2)  # height=1, epoch=1, action=EpochReward(2)
        + pre_econ_sha
        + post_econ_sha
        + econ_rcpt_sha
        + struct.pack(">I", 4)
        + u128_be(total_stake)
        + u128_be(issued)
        + u128_be(dust)
        + bytes([0, 0])  # produces_execution_root_domain=false, block_token_used_as_stake=false
    )
    assert len(adapter_bytes) == 443
    adapter_tag, _ = content_tag("receipt", ADAPTER_RECEIPT_SCHEMA, adapter_bytes)

    # 2. Reconstruct ValidatorEconAdversarialAssayV1 (196 bytes)
    min_equiv_stake = (total_stake // 3) + 1
    min_burn_units = (min_equiv_stake * 5_000) // 10_000
    min_burn_bps = (3_334 * 5_000 + 10_000 - 1) // 10_000  # 1667
    scenarios = [
        (1, 1, min_burn_units, min_burn_units + 1, 1),  # A1 Bribery + alert(1)
        (2, 1, total_stake // 4, (total_stake // 4) * 2, 2),  # A2 Censorship + alert(2)
        (3, 1, 50_000_000_000, 40_000_000_000, 0),  # A3 Equivocation slash-and-flee
        (4, 1, 1_000_000_000, 1_000, 0),  # A4 Griefing (1B slash + 1000 base fee)
        (5, 1, 2_500, 3, 0),  # A5 Concentration (2500 bps -> Nakamoto safety=3)
        (6, 1, 21, 22, 0),  # A6 Long-range (horizon=21, rejected at 22)
        (7, 1, 100_000_000_000, 4, 0),  # A7 Recovery (100B new bond -> 4 active)
    ]
    th = hashlib.sha256()
    th.update(VECON_ASSAY_TRANSCRIPT_PREFIX)
    for sid, passed, m1, m2, lc in scenarios:
        th.update(bytes([sid, passed]) + u128_be(m1) + u128_be(m2) + bytes([lc]))
    transcript_dig = th.digest()

    assay_bytes = (
        struct.pack(">H", 1)
        + genesis_id
        + profile_id
        + econ_policy_d
        + threat_d
        + u128_be(total_stake)
        + struct.pack(">IIHHBIB", 2, 3, 5_000, min_burn_bps, 0x7F, 2, 1)
        + transcript_dig
    )
    assert len(assay_bytes) == 196
    assay_tag, _ = content_tag("receipt", ADVERSARIAL_ASSAY_SCHEMA, assay_bytes)

    # Negative controls:
    # 1) produces_execution_root_domain=true moves adapter tag
    bad_dom = bytearray(adapter_bytes)
    bad_dom[-2] = 1
    assert content_tag("receipt", ADAPTER_RECEIPT_SCHEMA, bytes(bad_dom))[0] != adapter_tag
    # 2) block_token_used_as_stake=true moves adapter tag
    bad_tok = bytearray(adapter_bytes)
    bad_tok[-1] = 1
    assert content_tag("receipt", ADAPTER_RECEIPT_SCHEMA, bytes(bad_tok))[0] != adapter_tag
    # 3) Nakamoto bounds check
    assert (3_334 + 2_500 - 1) // 2_500 == 2 and (6_667 + 2_500 - 1) // 2_500 == 3
    return adapter_tag, assay_tag


def main() -> int:
    adapter_tag, assay_tag = build_validator_econ_1_vectors()
    if "--print-tags" in sys.argv:
        print("ADAPTER_TAG =", adapter_tag)
        print("ASSAY_TAG   =", assay_tag)
        return 0
    assert adapter_tag == EXPECTED_ADAPTER_TAG, f"adapter mismatch: {adapter_tag}"
    assert assay_tag == EXPECTED_ASSAY_TAG, f"assay mismatch: {assay_tag}"
    print(f"validator_econ_1_vectors: PASS (adapter={adapter_tag[:26]}..., assay={assay_tag[:26]}...)")
    return 0


EXPECTED_ADAPTER_TAG = "#bb.receipt.sha256.kwwsv73gxzeuug576j55yktxgcdn6zo5zuv4pguns5ewzq4nukda"
EXPECTED_ASSAY_TAG = "#bb.receipt.sha256.diw6cjminiwaapvdpjqh36cu7tthymzfzg62wrdeviizgnlftjta"

if __name__ == "__main__":
    raise SystemExit(main())
