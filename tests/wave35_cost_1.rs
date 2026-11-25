//! Wave 35 (`COST-1@base`) integration test suite: browser-game-platform resource budgets,
//! live `BitsEconomyStateV1` deterministic wire/proof measurements, and 6 powered negative controls.

#![cfg(feature = "protocol-research")]

use std::time::Instant;

use bitblock::bits_economy_1::{
    BITS_DEFAULT_CREATOR_SPLIT_BPS, BitsEconomyStateV1, BitsGenesisConfigV1, BitsGrantKindV1,
    BitsPurchaseSplitV1, BitsSignedTxV1, BitsTestnetGrantV1, BitsTxPayloadV1,
    compute_purchase_memo_hash, derive_default_bits_actor_id, verify_bits_wallet_proof,
};
use bitblock::cost_1::{
    COST_1_ALL_CONTROLS_MASK, COST_1_CTRL_PHONE_BUDGET_CAP, COST_1_CTRL_TX_WIRE_CAP,
    COST_1_CTRL_UNPOWERED_GUARD, COST_1_CTRL_VERIFY_LATENCY_CAP, COST_1_CTRL_WALLET_PROOF_CAP,
    COST_1_CTRL_WASM_GZIP_CAP, Cost1DispositionV1, Cost1Error, Cost1MeasurementReceiptV1,
    Cost1PlatformBudgetProfileV1, Cost1PlatformObservationV1, annual_mib_at_tps,
    evaluate_cost_1_platform_envelope,
};
use bitblock::crypto_d1::CanonicalEd25519PublicKeyV1;

fn seed_for(tag: u8, idx: u16) -> [u8; 32] {
    let mut out = [tag; 32];
    out[0..2].copy_from_slice(&idx.to_be_bytes());
    out
}

#[test]
fn cost_1_live_bits_economy_64_account_envelope_and_wire_caps() {
    let cfg = BitsGenesisConfigV1::canonical_testnet_default();
    let genesis_id = cfg.genesis_id().expect("genesis_id");
    let relayer_seed = [0xAAu8; 32];
    let relayer_pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&relayer_seed).expect("pk");
    let mut st = BitsEconomyStateV1::from_genesis(cfg, &[], &[relayer_pk]).expect("genesis");

    let mut actor_seeds = Vec::with_capacity(64);
    let mut actors = Vec::with_capacity(64);
    let mut grant_us = Vec::with_capacity(64);

    for i in 0u16..64 {
        let s = seed_for(0x31, i);
        let pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&s).expect("pk");
        let actor = derive_default_bits_actor_id(cfg.chain_id, &pk).expect("actor");
        let mut gid = [0u8; 32];
        gid[0..2].copy_from_slice(&i.to_be_bytes());
        let tx = BitsSignedTxV1::sign_with_seed(
            cfg.chain_id,
            genesis_id,
            cfg.grant_pool_actor,
            &relayer_seed,
            u64::from(i) + 1,
            1_000,
            BitsTxPayloadV1::TestnetGrant(BitsTestnetGrantV1 {
                recipient: actor,
                recipient_pubkey: pk,
                amount_bits: 1_000,
                grant_kind: BitsGrantKindV1::Signup,
                grant_id: gid,
            }),
        )
        .expect("sign grant");
        let t0 = Instant::now();
        st.apply_signed_tx(&tx, 1).expect("admit grant");
        grant_us.push(t0.elapsed().as_micros().max(1) as u64);
        actor_seeds.push(s);
        actors.push(actor);
    }

    let mut purchase_tx_wire_bytes = 0u32;
    let mut receipt_wire_bytes = 0u32;
    let mut purchase_us = Vec::with_capacity(32);

    for i in 0usize..32 {
        let buyer_idx = i * 2;
        let creator_idx = i * 2 + 1;
        let mut order_id = [0x77u8; 32];
        order_id[0..2].copy_from_slice(&(i as u16).to_be_bytes());
        let tx = BitsSignedTxV1::sign_with_seed(
            cfg.chain_id,
            genesis_id,
            actors[buyer_idx],
            &actor_seeds[buyer_idx],
            1,
            1_000,
            BitsTxPayloadV1::PurchaseSplit(BitsPurchaseSplitV1 {
                buyer: actors[buyer_idx],
                creator: actors[creator_idx],
                treasury: cfg.platform_treasury_actor,
                amount_bits: 250,
                creator_bps: BITS_DEFAULT_CREATOR_SPLIT_BPS,
                order_id,
                memo_hash: compute_purchase_memo_hash(
                    &order_id,
                    "studio-flagship",
                    "vip-pass",
                    &actors[buyer_idx],
                ),
            }),
        )
        .expect("sign purchase");
        purchase_tx_wire_bytes = tx.encode_canonical().expect("wire").len() as u32;
        let t0 = Instant::now();
        let rec = st.apply_signed_tx(&tx, 2).expect("admit purchase");
        purchase_us.push(t0.elapsed().as_micros().max(1) as u64);
        receipt_wire_bytes = rec.encode_canonical().expect("rec wire").len() as u32;
    }

    assert_eq!(purchase_tx_wire_bytes, 367);
    assert_eq!(receipt_wire_bytes, 236);
    st.verify_conservation().expect("closed conservation");

    // Measure inclusion and non-membership Domain-3 Patricia proof sizes and verification times.
    let inc_proof = st.prove_wallet(*actors[0].as_bytes()).expect("prove inc");
    let inc_bytes = inc_proof
        .patricia_proof
        .encode_canonical()
        .expect("proof wire")
        .len() as u32;

    let unfunded_seed = [0xEEu8; 32];
    let unfunded_pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&unfunded_seed).expect("pk");
    let abs_proof = st.prove_wallet(*unfunded_pk.as_bytes()).expect("prove abs");
    let abs_bytes = abs_proof
        .patricia_proof
        .encode_canonical()
        .expect("proof wire")
        .len() as u32;
    assert_eq!(abs_proof.balance_bits, 0);

    let mut verify_us = Vec::with_capacity(32);
    for actor in &actors[0..32] {
        let p = st.prove_wallet(*actor.as_bytes()).expect("prove");
        let t0 = Instant::now();
        let bal = verify_bits_wallet_proof(st.balance_root(), &p).expect("verify");
        verify_us.push(t0.elapsed().as_micros().max(1) as u64);
        assert!(bal > 0);
    }

    grant_us.sort_unstable();
    purchase_us.sort_unstable();
    verify_us.sort_unstable();

    let profile = Cost1PlatformBudgetProfileV1::canonical_platform_default();
    let obs = Cost1PlatformObservationV1 {
        light_wasm_gzip_bytes: 149_181,
        purchase_tx_wire_bytes,
        receipt_wire_bytes,
        wallet_inclusion_proof_bytes: inc_bytes,
        wallet_absence_proof_bytes: abs_bytes,
        phone_session_door_bytes: 30_720,
        active_accounts_sampled: 64,
        apply_purchase_split_p50_us: purchase_us[purchase_us.len() / 2],
        apply_purchase_split_max_us: *purchase_us.last().expect("non-empty"),
        verify_wallet_proof_p50_us: verify_us[verify_us.len() / 2],
        verify_wallet_proof_max_us: *verify_us.last().expect("non-empty"),
        apply_grant_p50_us: grant_us[grant_us.len() / 2],
        powered_controls_mask: COST_1_ALL_CONTROLS_MASK,
    };
    let receipt = evaluate_cost_1_platform_envelope(&profile, &obs).expect("evaluate");
    assert!(inc_bytes < profile.max_wallet_proof_bytes);
    assert!(abs_bytes < profile.max_wallet_proof_bytes);
    assert_eq!(receipt.projected_annual_mib_at_10tps, 181_353);
    assert_eq!(
        Cost1MeasurementReceiptV1::decode_canonical(&receipt.encode_canonical().expect("enc"))
            .expect("dec"),
        receipt
    );
}

#[test]
fn cost_1_all_six_powered_negative_controls_trip_fail_closed() {
    let profile = Cost1PlatformBudgetProfileV1::canonical_platform_default();
    let base_obs = Cost1PlatformObservationV1 {
        light_wasm_gzip_bytes: 149_181,
        purchase_tx_wire_bytes: 367,
        receipt_wire_bytes: 236,
        wallet_inclusion_proof_bytes: 460,
        wallet_absence_proof_bytes: 380,
        phone_session_door_bytes: 30_720,
        active_accounts_sampled: 64,
        apply_purchase_split_p50_us: 120,
        apply_purchase_split_max_us: 210,
        verify_wallet_proof_p50_us: 15,
        verify_wallet_proof_max_us: 32,
        apply_grant_p50_us: 95,
        powered_controls_mask: COST_1_ALL_CONTROLS_MASK,
    };

    let mut mask = 0u32;

    // Control 0: 1-byte-tighter wallet proof cap trips Miss.
    let mut p0 = profile;
    p0.max_wallet_proof_bytes = base_obs.wallet_inclusion_proof_bytes - 1;
    if evaluate_cost_1_platform_envelope(&p0, &base_obs)
        .expect("eval")
        .disposition
        == Cost1DispositionV1::Miss
    {
        mask |= COST_1_CTRL_WALLET_PROOF_CAP;
    }

    // Control 1: 1-byte-over WASM gzip cap trips Miss.
    let mut o1 = base_obs;
    o1.light_wasm_gzip_bytes = profile.max_light_wasm_gzip_bytes + 1;
    if evaluate_cost_1_platform_envelope(&profile, &o1)
        .expect("eval")
        .disposition
        == Cost1DispositionV1::Miss
    {
        mask |= COST_1_CTRL_WASM_GZIP_CAP;
    }

    // Control 2: 1-byte-over PurchaseSplit wire cap trips Miss.
    let mut o2 = base_obs;
    o2.purchase_tx_wire_bytes = profile.max_purchase_tx_wire_bytes + 1;
    if evaluate_cost_1_platform_envelope(&profile, &o2)
        .expect("eval")
        .disposition
        == Cost1DispositionV1::Miss
    {
        mask |= COST_1_CTRL_TX_WIRE_CAP;
    }

    // Control 3: 1-us-tighter verify latency cap trips Miss.
    let mut p3 = profile;
    p3.max_verify_wallet_proof_us = base_obs.verify_wallet_proof_p50_us - 1;
    if evaluate_cost_1_platform_envelope(&p3, &base_obs)
        .expect("eval")
        .disposition
        == Cost1DispositionV1::Miss
    {
        mask |= COST_1_CTRL_VERIFY_LATENCY_CAP;
    }

    // Control 4: 1-byte-over daily phone session budget trips Miss.
    let mut o4 = base_obs;
    o4.phone_session_door_bytes = profile.max_phone_session_bytes + 1;
    if evaluate_cost_1_platform_envelope(&profile, &o4)
        .expect("eval")
        .disposition
        == Cost1DispositionV1::Miss
    {
        mask |= COST_1_CTRL_PHONE_BUDGET_CAP;
    }

    // Control 5: missing any powered control bit trips Unmeasured (cannot pass Met).
    let mut o5 = base_obs;
    o5.powered_controls_mask = COST_1_ALL_CONTROLS_MASK & !COST_1_CTRL_WALLET_PROOF_CAP;
    if evaluate_cost_1_platform_envelope(&profile, &o5)
        .expect("eval")
        .disposition
        == Cost1DispositionV1::Unmeasured
    {
        mask |= COST_1_CTRL_UNPOWERED_GUARD;
    }

    assert_eq!(mask, COST_1_ALL_CONTROLS_MASK);

    // Invalid zero ceiling rejected fail-closed.
    let mut bad_prof = profile;
    bad_prof.max_wallet_proof_bytes = 0;
    assert!(matches!(
        bad_prof.encode_canonical(),
        Err(Cost1Error::InvalidProfile(_))
    ));
    assert!(annual_mib_at_tps(u64::MAX, u64::MAX).is_err());
}
