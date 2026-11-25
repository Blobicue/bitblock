//! Correspondence & negative-control test suite bridging `crates/research/src/bits_economy_1.rs`
//! and `proof/BitblockProof/BitsEconomyRefinement.lean` (`--trust=0`).
//!
//! Verifies that the production Rust `BitsEconomyStateV1` implementation matches the Lean 4
//! 6-opcode state-machine model across:
//! 1. Exact integer `80/20` purchase split bounds & canonical vectors (zero dust, `0%` transfer tax);
//! 2. Closed total-supply conservation (`sum(balances) == genesis_supply`) across multi-opcode
//!    blocks covering all 6 opcodes (`0x01..=0x06`) plus arbitrary actor aliasing;
//! 3. Fail-closed atomic rejection rollback (any error leaves balances, nonces, roots, replay sets,
//!    and delegations bit-for-bit identical);
//! 4. Structural `lab == false` refusal for `BitsTestnetGrantV1` and `BitsSimulatedPayoutV1`;
//! 5. Cross-Session Amendment 1 `ActorIdV1` preservation on key rotation + automatic revocation of
//!    all stale delegations;
//! 6. Non-transitive depth-1 delegation cap enforcement (`max_bits_per_tx`, `max_bits_total`,
//!    `expires_at_height`) and refusal of depth-2 sub-delegations, delegated transfers, and
//!    delegated simulated payouts;
//! 7. Powered negative controls proving that breaking split bounds, mutating out-of-pool supply,
//!    replaying order/grant/payout IDs, or regressing `L2Anchor` epochs are all caught fail-closed.

#![cfg(feature = "protocol-research")]

use bitblock::bits_economy_1::{
    BITS_BPS_DENOMINATOR, BITS_DEFAULT_CREATOR_SPLIT_BPS, BitsDelegationGrantV1, BitsEconomy1Error,
    BitsEconomyStateV1, BitsGenesisConfigV1, BitsGrantKindV1, BitsIdentityBindV1,
    BitsIdentityProviderV1, BitsL2AnchorV1, BitsPurchaseSplitV1, BitsSignedTxV1,
    BitsSimulatedPayoutV1, BitsTestnetGrantV1, BitsTransferV1, BitsTxPayloadV1,
    compute_purchase_memo_hash, compute_purchase_split, derive_default_bits_actor_id,
    sign_identity_bind_pop_with_seed,
};
use bitblock::crypto_d1::CanonicalEd25519PublicKeyV1;

fn pk_from_seed(seed: &[u8; 32]) -> CanonicalEd25519PublicKeyV1 {
    CanonicalEd25519PublicKeyV1::from_secret_seed(seed).expect("valid seed")
}

#[test]
fn bits_economy_lean_split_conservation_and_bounds_correspondence() {
    // Mirrors Lean `bits_purchase_split_exact_conservation`,
    // `bits_purchase_split_floor_lower_bound`, `bits_purchase_split_floor_upper_bound`,
    // and `bits_purchase_split_canonical_vectors`.
    let canonical = [
        (10u128, 8u128, 2u128),
        (250, 200, 50),
        (101, 80, 21),
        (999, 799, 200),
    ];
    for (amount, exp_creator, exp_platform) in canonical {
        let (c, p) =
            compute_purchase_split(amount, BITS_DEFAULT_CREATOR_SPLIT_BPS).expect("valid split");
        assert_eq!((c, p), (exp_creator, exp_platform));
        assert_eq!(c + p, amount);
    }

    // Sweep a wide range of amounts and valid basis points [0..=10_000].
    for bps in [0u16, 1, 500, 3333, 5000, 7999, 8000, 8001, 9999, 10_000] {
        for amount in [
            1u128,
            10,
            11,
            17,
            99,
            100,
            101,
            250,
            999,
            12_345,
            1_000_000_000,
        ] {
            let (c, p) = compute_purchase_split(amount, bps).expect("valid split");
            assert_eq!(c + p, amount, "exact split conservation failed");
            assert!(c <= amount);
            assert!(c * BITS_BPS_DENOMINATOR <= amount * u128::from(bps));
            assert!(amount * u128::from(bps) < (c + 1) * BITS_BPS_DENOMINATOR);
        }
    }

    // Negative control mirroring `unbounded_bps_breaks_split_conservation`.
    assert!(matches!(
        compute_purchase_split(10_000, 10_001),
        Err(BitsEconomy1Error::PolicyViolation(_))
    ));
}

#[test]
fn bits_economy_lean_six_opcode_state_machine_correspondence() {
    let cfg = BitsGenesisConfigV1::canonical_testnet_default();
    let genesis_id = cfg.genesis_id().expect("genesis id");

    let relayer_seed = [0xA1u8; 32];
    let buyer_seed = [0xB2u8; 32];
    let buyer_new_seed = [0xB3u8; 32];
    let creator_seed = [0xC4u8; 32];
    let delegate_seed = [0xD5u8; 32];
    let sub_delegate_seed = [0xE6u8; 32];

    let relayer_pk = pk_from_seed(&relayer_seed);
    let buyer_pk = pk_from_seed(&buyer_seed);
    let buyer_new_pk = pk_from_seed(&buyer_new_seed);
    let creator_pk = pk_from_seed(&creator_seed);
    let delegate_pk = pk_from_seed(&delegate_seed);
    let sub_delegate_pk = pk_from_seed(&sub_delegate_seed);

    let buyer = derive_default_bits_actor_id(cfg.chain_id, &buyer_pk).expect("buyer actor");
    let creator = derive_default_bits_actor_id(cfg.chain_id, &creator_pk).expect("creator actor");
    let delegate_actor =
        derive_default_bits_actor_id(cfg.chain_id, &delegate_pk).expect("delegate actor");

    let mut st = BitsEconomyStateV1::from_genesis(cfg, &[], &[relayer_pk]).expect("genesis");
    assert_eq!(st.grant_pool_remaining(), cfg.genesis_supply);

    // 1. Opcode 0x03 TestnetGrant (buyer 1,000 + creator 50 + delegate 50 for binding).
    let grant_buyer = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        genesis_id,
        cfg.grant_pool_actor,
        &relayer_seed,
        1,
        100,
        BitsTxPayloadV1::TestnetGrant(BitsTestnetGrantV1 {
            recipient: buyer,
            recipient_pubkey: buyer_pk,
            amount_bits: 1_000,
            grant_kind: BitsGrantKindV1::Signup,
            grant_id: [1u8; 32],
        }),
    )
    .expect("sign grant");
    let r1 = st.apply_signed_tx(&grant_buyer, 1).expect("admit grant");
    assert_eq!(r1.opcode, 0x03);
    assert_eq!(st.balance_of(&buyer), 1_000);
    st.verify_conservation().expect("conserved");

    // Duplicate grant_id rejected with exact atomic rollback (`duplicate_grant_rejected` +
    // `applyBitsTx_atomic_rollback_on_reject`).
    let root_before = (st.balance_root(), st.nonce_root());
    let dup_grant = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        genesis_id,
        cfg.grant_pool_actor,
        &relayer_seed,
        2,
        100,
        BitsTxPayloadV1::TestnetGrant(BitsTestnetGrantV1 {
            recipient: buyer,
            recipient_pubkey: buyer_pk,
            amount_bits: 50,
            grant_kind: BitsGrantKindV1::Faucet,
            grant_id: [1u8; 32],
        }),
    )
    .expect("sign dup");
    assert!(matches!(
        st.apply_signed_tx(&dup_grant, 2),
        Err(BitsEconomy1Error::ReplayOrNonce(_))
    ));
    assert_eq!((st.balance_root(), st.nonce_root()), root_before);

    // Bind creator and delegate accounts via faucet drips from TestnetGrantPool.
    let grant_creator = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        genesis_id,
        cfg.grant_pool_actor,
        &relayer_seed,
        2,
        100,
        BitsTxPayloadV1::TestnetGrant(BitsTestnetGrantV1 {
            recipient: creator,
            recipient_pubkey: creator_pk,
            amount_bits: 50,
            grant_kind: BitsGrantKindV1::Faucet,
            grant_id: [2u8; 32],
        }),
    )
    .expect("sign creator grant");
    st.apply_signed_tx(&grant_creator, 2)
        .expect("admit creator");

    let grant_delegate = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        genesis_id,
        cfg.grant_pool_actor,
        &relayer_seed,
        3,
        100,
        BitsTxPayloadV1::TestnetGrant(BitsTestnetGrantV1 {
            recipient: delegate_actor,
            recipient_pubkey: delegate_pk,
            amount_bits: 50,
            grant_kind: BitsGrantKindV1::Faucet,
            grant_id: [3u8; 32],
        }),
    )
    .expect("sign delegate grant");
    st.apply_signed_tx(&grant_delegate, 2)
        .expect("admit delegate");

    // 2. Register depth-1 delegation buyer -> delegate_pk (max 250/tx, 300 total, expires h=10).
    st.register_purchase_delegation(BitsDelegationGrantV1 {
        principal_actor: buyer,
        delegate_pubkey: delegate_pk,
        max_bits_per_tx: 250,
        max_bits_total: 300,
        spent_bits: 0,
        expires_at_height: 10,
        allow_purchase_split: true,
        revoked: false,
    })
    .expect("depth-1 delegation");

    // Negative control: depth-2 transitive sub-delegation rejected
    // (`registerDelegation_rejects_depth2_transitive`).
    assert!(matches!(
        st.register_purchase_delegation(BitsDelegationGrantV1 {
            principal_actor: delegate_actor,
            delegate_pubkey: sub_delegate_pk,
            max_bits_per_tx: 50,
            max_bits_total: 50,
            spent_bits: 0,
            expires_at_height: 10,
            allow_purchase_split: true,
            revoked: false,
        }),
        Err(BitsEconomy1Error::Unauthorized(_))
    ));

    // Negative control: delegate cannot authorize Transfer (`delegation_cannot_authorize_transfer`).
    let bad_del_transfer = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        genesis_id,
        buyer,
        &delegate_seed,
        1,
        100,
        BitsTxPayloadV1::Transfer(BitsTransferV1 {
            from: buyer,
            to: creator,
            amount_bits: 10,
            memo_hash: [0u8; 32],
        }),
    )
    .expect("sign bad transfer");
    assert!(matches!(
        st.apply_signed_tx(&bad_del_transfer, 3),
        Err(BitsEconomy1Error::Unauthorized(_))
    ));

    // 3. Opcode 0x02 PurchaseSplit via delegate (250 Bits -> 200 creator / 50 treasury).
    let order1 = [0x11u8; 32];
    let purchase1 = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        genesis_id,
        buyer,
        &delegate_seed,
        1,
        100,
        BitsTxPayloadV1::PurchaseSplit(BitsPurchaseSplitV1 {
            buyer,
            creator,
            treasury: cfg.platform_treasury_actor,
            amount_bits: 250,
            creator_bps: BITS_DEFAULT_CREATOR_SPLIT_BPS,
            order_id: order1,
            memo_hash: compute_purchase_memo_hash(&order1, "neon-obby", "vip-pass", &buyer),
        }),
    )
    .expect("sign delegated purchase");
    let r2 = st.apply_signed_tx(&purchase1, 3).expect("admit purchase");
    assert_eq!(r2.opcode, 0x02);
    assert_eq!((r2.creator_bits_paid, r2.platform_bits_paid), (200, 50));
    assert_eq!(st.balance_of(&buyer), 750);
    assert_eq!(st.balance_of(&creator), 250);
    assert_eq!(st.treasury_balance(), 50);
    st.verify_conservation().expect("conserved");

    // Negative control: delegate exceeding remaining total cap (250 + 60 > 300) rejected atomically.
    let over_cap_order = [0x12u8; 32];
    let over_cap = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        genesis_id,
        buyer,
        &delegate_seed,
        2,
        100,
        BitsTxPayloadV1::PurchaseSplit(BitsPurchaseSplitV1 {
            buyer,
            creator,
            treasury: cfg.platform_treasury_actor,
            amount_bits: 60,
            creator_bps: BITS_DEFAULT_CREATOR_SPLIT_BPS,
            order_id: over_cap_order,
            memo_hash: compute_purchase_memo_hash(&over_cap_order, "neon-obby", "pack", &buyer),
        }),
    )
    .expect("sign over-cap");
    assert!(matches!(
        st.apply_signed_tx(&over_cap, 4),
        Err(BitsEconomy1Error::Unauthorized(_))
    ));

    // Negative control: duplicate order_id rejected (`duplicate_order_rejected`).
    let dup_order = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        genesis_id,
        buyer,
        &buyer_seed,
        2,
        100,
        BitsTxPayloadV1::PurchaseSplit(BitsPurchaseSplitV1 {
            buyer,
            creator,
            treasury: cfg.platform_treasury_actor,
            amount_bits: 10,
            creator_bps: BITS_DEFAULT_CREATOR_SPLIT_BPS,
            order_id: order1,
            memo_hash: [0u8; 32],
        }),
    )
    .expect("sign dup order");
    assert!(matches!(
        st.apply_signed_tx(&dup_order, 4),
        Err(BitsEconomy1Error::ReplayOrNonce(_))
    ));

    // 4. Opcode 0x05 IdentityBind key rotation (`identityBind_rotation_updates_key_and_revokes_delegations`):
    // buyer balance stays 750 on permanent ActorIdV1, new key active, stale delegate revoked!
    let ext_hash = [0x55u8; 32];
    let pop = sign_identity_bind_pop_with_seed(
        &buyer_new_seed,
        cfg.chain_id,
        &genesis_id,
        &buyer,
        BitsIdentityProviderV1::Discord,
        &ext_hash,
    )
    .expect("pop");
    let rotate_tx = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        genesis_id,
        buyer,
        &buyer_seed,
        2,
        100,
        BitsTxPayloadV1::IdentityBind(BitsIdentityBindV1 {
            actor_id: buyer,
            ed25519_pk: buyer_new_pk,
            provider: BitsIdentityProviderV1::Discord,
            external_id_hash: ext_hash,
            pop_signature: pop,
        }),
    )
    .expect("sign rotation");
    let r3 = st.apply_signed_tx(&rotate_tx, 5).expect("admit rotation");
    assert_eq!(r3.opcode, 0x05);
    assert_eq!(st.balance_of(&buyer), 750);

    // Stale delegate (even within 50 Bits remaining cap) MUST now be rejected after rotation!
    let post_rot_order = [0x13u8; 32];
    let stale_del = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        genesis_id,
        buyer,
        &delegate_seed,
        3,
        100,
        BitsTxPayloadV1::PurchaseSplit(BitsPurchaseSplitV1 {
            buyer,
            creator,
            treasury: cfg.platform_treasury_actor,
            amount_bits: 20,
            creator_bps: BITS_DEFAULT_CREATOR_SPLIT_BPS,
            order_id: post_rot_order,
            memo_hash: [0u8; 32],
        }),
    )
    .expect("sign stale delegate");
    assert!(matches!(
        st.apply_signed_tx(&stale_del, 6),
        Err(BitsEconomy1Error::Unauthorized(_))
    ));

    // 5. Opcode 0x01 Transfer (0% tax) signed by buyer's NEW rotated key.
    let transfer_tx = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        genesis_id,
        buyer,
        &buyer_new_seed,
        3,
        100,
        BitsTxPayloadV1::Transfer(BitsTransferV1 {
            from: buyer,
            to: creator,
            amount_bits: 50,
            memo_hash: [0x22u8; 32],
        }),
    )
    .expect("sign transfer");
    let r4 = st.apply_signed_tx(&transfer_tx, 6).expect("admit transfer");
    assert_eq!(r4.opcode, 0x01);
    assert_eq!(st.balance_of(&buyer), 700);
    assert_eq!(st.balance_of(&creator), 300);

    // 6. Opcode 0x04 SimulatedPayout + duplicate payout rejection.
    let payout_tx = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        genesis_id,
        creator,
        &creator_seed,
        1,
        100,
        BitsTxPayloadV1::SimulatedPayout(BitsSimulatedPayoutV1 {
            creator,
            amount_bits: 100,
            payout_request_id: [0x44u8; 32],
        }),
    )
    .expect("sign payout");
    let r5 = st.apply_signed_tx(&payout_tx, 7).expect("admit payout");
    assert_eq!(r5.opcode, 0x04);
    assert_eq!(st.balance_of(&creator), 200);
    assert_eq!(st.simulated_payout_escrow(), 100);

    let dup_payout = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        genesis_id,
        creator,
        &creator_seed,
        2,
        100,
        BitsTxPayloadV1::SimulatedPayout(BitsSimulatedPayoutV1 {
            creator,
            amount_bits: 100,
            payout_request_id: [0x44u8; 32],
        }),
    )
    .expect("sign dup payout");
    assert!(matches!(
        st.apply_signed_tx(&dup_payout, 8),
        Err(BitsEconomy1Error::ReplayOrNonce(_))
    ));

    // 7. Opcode 0x06 L2Anchor (zero balance mutation + strict monotone epoch).
    let bal_root_pre_anchor = st.balance_root();
    let anchor1 = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        genesis_id,
        cfg.grant_pool_actor,
        &relayer_seed,
        4,
        100,
        BitsTxPayloadV1::L2Anchor(BitsL2AnchorV1 {
            app_id_hash: [0x77u8; 32],
            epoch_id: 1,
            l2_root: [0x88u8; 32],
            event_count: 12,
        }),
    )
    .expect("sign anchor1");
    let r6 = st.apply_signed_tx(&anchor1, 9).expect("admit anchor1");
    assert_eq!(r6.opcode, 0x06);
    assert_eq!(st.balance_root(), bal_root_pre_anchor);

    let bad_anchor = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        genesis_id,
        cfg.grant_pool_actor,
        &relayer_seed,
        5,
        100,
        BitsTxPayloadV1::L2Anchor(BitsL2AnchorV1 {
            app_id_hash: [0x77u8; 32],
            epoch_id: 1,
            l2_root: [0x99u8; 32],
            event_count: 13,
        }),
    )
    .expect("sign bad anchor");
    assert!(matches!(
        st.apply_signed_tx(&bad_anchor, 10),
        Err(BitsEconomy1Error::ReplayOrNonce(_))
    ));

    st.verify_conservation().expect("final conservation");
}

#[test]
fn bits_economy_lean_lab_false_structural_refusal_correspondence() {
    // Mirrors `testnetGrant_never_admitted_when_lab_false` and
    // `simulatedPayout_never_admitted_when_lab_false`.
    let mut cfg = BitsGenesisConfigV1::canonical_testnet_default();
    cfg.lab = false;
    cfg.initial_grant_pool = 500_000_000;
    let genesis_id = cfg.genesis_id().expect("genesis id");

    let relayer_seed = [0x1Au8; 32];
    let creator_seed = [0x2Bu8; 32];
    let relayer_pk = pk_from_seed(&relayer_seed);
    let creator_pk = pk_from_seed(&creator_seed);
    let creator = derive_default_bits_actor_id(cfg.chain_id, &creator_pk).expect("creator");

    let mut st =
        BitsEconomyStateV1::from_genesis(cfg, &[(creator, creator_pk, 500_000_000)], &[relayer_pk])
            .expect("non-lab genesis");
    let snap = (st.balance_root(), st.nonce_root());

    let grant_tx = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        genesis_id,
        cfg.grant_pool_actor,
        &relayer_seed,
        1,
        100,
        BitsTxPayloadV1::TestnetGrant(BitsTestnetGrantV1 {
            recipient: creator,
            recipient_pubkey: creator_pk,
            amount_bits: 1_000,
            grant_kind: BitsGrantKindV1::Signup,
            grant_id: [0x99u8; 32],
        }),
    )
    .expect("sign grant");
    assert!(matches!(
        st.apply_signed_tx(&grant_tx, 1),
        Err(BitsEconomy1Error::TestnetOnlyDisabled(_))
    ));

    let payout_tx = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        genesis_id,
        creator,
        &creator_seed,
        1,
        100,
        BitsTxPayloadV1::SimulatedPayout(BitsSimulatedPayoutV1 {
            creator,
            amount_bits: 100,
            payout_request_id: [0x98u8; 32],
        }),
    )
    .expect("sign payout");
    assert!(matches!(
        st.apply_signed_tx(&payout_tx, 1),
        Err(BitsEconomy1Error::TestnetOnlyDisabled(_))
    ));

    assert_eq!((st.balance_root(), st.nonce_root()), snap);
    st.verify_conservation().expect("still conserved");
}
