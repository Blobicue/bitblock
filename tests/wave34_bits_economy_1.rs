#![cfg(feature = "protocol-research")]

use bitblock::bits_economy_1::{
    BITS_SIMULATED_PAYOUT_LABEL, BITS_TESTNET_CUSTODY_LABEL, BitsDelegationGrantV1,
    BitsDispositionV1, BitsEconomy1Error, BitsEconomyStateV1, BitsGenesisConfigV1, BitsGrantKindV1,
    BitsIdentityBindV1, BitsIdentityProviderV1, BitsL2AnchorV1, BitsPurchaseSplitV1,
    BitsSignedTxV1, BitsSimulatedPayoutV1, BitsStateReceiptV1, BitsTestnetGrantV1, BitsTransferV1,
    BitsTxPayloadV1, compute_purchase_memo_hash, compute_purchase_split,
    derive_default_bits_actor_id, sign_identity_bind_pop_with_seed, verify_bits_wallet_proof,
};
use bitblock::crypto_d1::CanonicalEd25519PublicKeyV1;
use sha2::{Digest, Sha256};

#[test]
fn wave34_bits_economy_1_eight_golden_tags_match_python_exact() {
    assert_eq!(
        BITS_TESTNET_CUSTODY_LABEL,
        "DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY"
    );
    assert_eq!(
        BITS_SIMULATED_PAYOUT_LABEL,
        "SIMULATED-TEST-ONLY-NO-REAL-MONEY"
    );

    let cfg = BitsGenesisConfigV1::canonical_testnet_default();
    assert_eq!(
        cfg.tag().unwrap().to_string(),
        "#bb.policy.sha256.ffoppkqwuspygjt4uroahhuly56g6rhcvxvly6uqyavaeebamm2q"
    );

    let s1 = [0x11u8; 32];
    let s2 = [0x22u8; 32];
    let s_rel = [0x99u8; 32];
    let pk1 = CanonicalEd25519PublicKeyV1::from_secret_seed(&s1).unwrap();
    let pk2 = CanonicalEd25519PublicKeyV1::from_secret_seed(&s2).unwrap();
    let relayer_pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&s_rel).unwrap();
    let a1 = derive_default_bits_actor_id(cfg.chain_id, &pk1).unwrap();
    let a2 = derive_default_bits_actor_id(cfg.chain_id, &pk2).unwrap();

    let mut st = BitsEconomyStateV1::from_genesis(cfg, &[], &[relayer_pk]).unwrap();
    assert_eq!(st.config(), &cfg);
    assert_eq!(st.account_count(), 1);
    let gid = st.genesis_id();

    // 1. TestnetGrant (1,000 Bits Signup -> a1)
    let grant = BitsTestnetGrantV1 {
        recipient: a1,
        recipient_pubkey: pk1,
        amount_bits: 1_000,
        grant_kind: BitsGrantKindV1::Signup,
        grant_id: [0xA1; 32],
    };
    assert_eq!(
        grant.tag().unwrap().to_string(),
        "#bb.receipt.sha256.evchb43nwqjxbe6yzeei6azy3szjk2k3utbmhifxwbkrvriz25iq"
    );
    let grant_tx = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        gid,
        cfg.grant_pool_actor,
        &s_rel,
        1,
        10,
        BitsTxPayloadV1::TestnetGrant(grant),
    )
    .unwrap();
    let grant_wire = grant_tx.encode_canonical().unwrap();
    assert_eq!(
        BitsSignedTxV1::decode_canonical(&grant_wire).unwrap(),
        grant_tx
    );
    let _ = grant_tx.signing_digest().unwrap();
    let grant_rcpt = st.apply_signed_tx(&grant_tx, 1).unwrap();
    assert_eq!(grant_rcpt.disposition, BitsDispositionV1::Admitted);
    assert_eq!(st.balance_of(&a1), 1_000);

    // 2. PurchaseSplit (250 Bits, 80/20 -> creator +200, treasury +50)
    let order_id = [0xB2; 32];
    let memo_hash = compute_purchase_memo_hash(&order_id, "aurora_forge_plaza", "pass_vip", &a1);
    let split = BitsPurchaseSplitV1 {
        buyer: a1,
        creator: a2,
        treasury: cfg.platform_treasury_actor,
        amount_bits: 250,
        creator_bps: 8_000,
        order_id,
        memo_hash,
    };
    assert_eq!(
        split.tag().unwrap().to_string(),
        "#bb.receipt.sha256.pvzttfomarxcv5y62nspvwxmty647hbqaol6u3ryhwcak7fta4dq"
    );
    let split_tx = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        gid,
        a1,
        &s1,
        1,
        10,
        BitsTxPayloadV1::PurchaseSplit(split),
    )
    .unwrap();
    let split_rcpt = st.apply_signed_tx(&split_tx, 2).unwrap();
    let split_rcpt_wire = split_rcpt.encode_canonical().unwrap();
    assert_eq!(
        BitsStateReceiptV1::decode_canonical(&split_rcpt_wire).unwrap(),
        split_rcpt
    );
    assert_eq!(
        split_rcpt.tag().unwrap().to_string(),
        "#bb.receipt.sha256.2nvkbca2plvh6ftaj5yknwj7svq7kzb7q3jbprptli7pth7vnpva"
    );

    // 3. Transfer (75 Bits a1 -> a2, 0% tax)
    let transfer = BitsTransferV1 {
        from: a1,
        to: a2,
        amount_bits: 75,
        memo_hash: [0xD3; 32],
    };
    assert_eq!(
        transfer.tag().unwrap().to_string(),
        "#bb.receipt.sha256.d7f3f4762dfz3s5dtqkdcj4ba6h3ssfdwbjw5m2lsvvegwqvhn6q"
    );
    let transfer_tx = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        gid,
        a1,
        &s1,
        2,
        10,
        BitsTxPayloadV1::Transfer(transfer),
    )
    .unwrap();
    let _ = st.apply_signed_tx(&transfer_tx, 3).unwrap();
    assert_eq!(st.balance_of(&a1), 675);
    assert_eq!(st.balance_of(&a2), 275);

    // 4. SimulatedPayout (150 Bits a2 -> payout_escrow)
    let payout = BitsSimulatedPayoutV1 {
        creator: a2,
        amount_bits: 150,
        payout_request_id: [0xE4; 32],
    };
    assert_eq!(
        payout.tag().unwrap().to_string(),
        "#bb.receipt.sha256.o3yqx3h5qo7fjxlbleywjr5frgb37c5dhwzreykoqvovpbqdctwa"
    );
    // First bind a2's key via IdentityBind so a2 can sign SimulatedPayout
    let a2_pop = sign_identity_bind_pop_with_seed(
        &s2,
        cfg.chain_id,
        &gid,
        &a2,
        BitsIdentityProviderV1::StudioHandle,
        &[0xCD; 32],
    )
    .unwrap();
    let a2_bind_tx = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        gid,
        a2,
        &s2,
        1,
        10,
        BitsTxPayloadV1::IdentityBind(BitsIdentityBindV1 {
            actor_id: a2,
            ed25519_pk: pk2,
            provider: BitsIdentityProviderV1::StudioHandle,
            external_id_hash: [0xCD; 32],
            pop_signature: a2_pop,
        }),
    )
    .unwrap();
    st.apply_signed_tx(&a2_bind_tx, 4).unwrap();
    let payout_tx = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        gid,
        a2,
        &s2,
        2,
        10,
        BitsTxPayloadV1::SimulatedPayout(payout),
    )
    .unwrap();
    st.apply_signed_tx(&payout_tx, 5).unwrap();
    assert_eq!(st.balance_of(&a2), 125);
    assert_eq!(st.simulated_payout_escrow(), 150);

    // 5. IdentityBind golden tag (a1 + pk1 + Discord)
    let a1_pop = sign_identity_bind_pop_with_seed(
        &s1,
        cfg.chain_id,
        &gid,
        &a1,
        BitsIdentityProviderV1::Discord,
        &[0xCC; 32],
    )
    .unwrap();
    let bind = BitsIdentityBindV1 {
        actor_id: a1,
        ed25519_pk: pk1,
        provider: BitsIdentityProviderV1::Discord,
        external_id_hash: [0xCC; 32],
        pop_signature: a1_pop,
    };
    assert_eq!(
        bind.tag().to_string(),
        "#bb.receipt.sha256.5paczgevchguypppkwxu3iehum7x5o6xrjd5u5knc3ntijtag5da"
    );

    // 6. L2Anchor golden tag (audit-only epoch 1)
    let app_hash: [u8; 32] = Sha256::digest(b"gameladder").into();
    let anchor = BitsL2AnchorV1 {
        app_id_hash: app_hash,
        epoch_id: 1,
        l2_root: [0xF5; 32],
        event_count: 42,
    };
    assert_eq!(
        anchor.tag().to_string(),
        "#bb.receipt.sha256.js5krkkb4hixcxoh3w4cdrfex6j7t7cvwefhdtky5q3ovcurqjba"
    );
    let anchor_tx = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        gid,
        cfg.grant_pool_actor,
        &s_rel,
        2,
        10,
        BitsTxPayloadV1::L2Anchor(anchor),
    )
    .unwrap();
    st.apply_signed_tx(&anchor_tx, 6).unwrap();
    // Stale/duplicate epoch must fail closed
    let stale_anchor_tx = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        gid,
        cfg.grant_pool_actor,
        &s_rel,
        3,
        10,
        BitsTxPayloadV1::L2Anchor(anchor),
    )
    .unwrap();
    assert!(matches!(
        st.apply_signed_tx(&stale_anchor_tx, 7),
        Err(BitsEconomy1Error::ReplayOrNonce(_))
    ));
}

#[test]
fn wave34_amendment1_key_rotation_delegation_and_lab_false_refusal() {
    let cfg = BitsGenesisConfigV1::canonical_testnet_default();
    let s1 = [0x31u8; 32];
    let s1_rotated = [0x32u8; 32];
    let s_creator = [0x33u8; 32];
    let s_relayer = [0x34u8; 32];
    let s_third = [0x35u8; 32];
    let pk1 = CanonicalEd25519PublicKeyV1::from_secret_seed(&s1).unwrap();
    let pk1_rot = CanonicalEd25519PublicKeyV1::from_secret_seed(&s1_rotated).unwrap();
    let pk_creator = CanonicalEd25519PublicKeyV1::from_secret_seed(&s_creator).unwrap();
    let pk_relayer = CanonicalEd25519PublicKeyV1::from_secret_seed(&s_relayer).unwrap();
    let pk_third = CanonicalEd25519PublicKeyV1::from_secret_seed(&s_third).unwrap();
    let a1 = derive_default_bits_actor_id(cfg.chain_id, &pk1).unwrap();
    let a_creator = derive_default_bits_actor_id(cfg.chain_id, &pk_creator).unwrap();
    let a_relayer = derive_default_bits_actor_id(cfg.chain_id, &pk_relayer).unwrap();

    let mut st = BitsEconomyStateV1::from_genesis(cfg, &[], &[pk_relayer]).unwrap();
    let gid = st.genesis_id();

    // Unfunded wallet still resolves deterministically to canonical ActorIdV1 (Amendment 1!)
    let unfunded_wp = st.prove_wallet(*pk1.as_bytes()).unwrap();
    assert_eq!(unfunded_wp.actor_id, a1);
    assert_eq!(unfunded_wp.balance_bits, 0);
    assert_eq!(
        verify_bits_wallet_proof(st.balance_root(), &unfunded_wp).unwrap(),
        0
    );

    // Fund a1 via Signup grant (1,000 Bits)
    let gtx = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        gid,
        cfg.grant_pool_actor,
        &s_relayer,
        1,
        20,
        BitsTxPayloadV1::TestnetGrant(BitsTestnetGrantV1 {
            recipient: a1,
            recipient_pubkey: pk1,
            amount_bits: 1_000,
            grant_kind: BitsGrantKindV1::Signup,
            grant_id: [0x71; 32],
        }),
    )
    .unwrap();
    st.apply_signed_tx(&gtx, 1).unwrap();

    // Duplicate grant_id rejected fail-closed
    let dup_gtx = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        gid,
        cfg.grant_pool_actor,
        &s_relayer,
        2,
        20,
        BitsTxPayloadV1::TestnetGrant(BitsTestnetGrantV1 {
            recipient: a1,
            recipient_pubkey: pk1,
            amount_bits: 1_000,
            grant_kind: BitsGrantKindV1::Signup,
            grant_id: [0x71; 32],
        }),
    )
    .unwrap();
    assert!(matches!(
        st.apply_signed_tx(&dup_gtx, 2),
        Err(BitsEconomy1Error::ReplayOrNonce(_))
    ));

    // Register depth-1 scoped PurchaseSplit delegation for relayer (cap 100/tx, 150 total, exp 10)
    st.register_purchase_delegation(BitsDelegationGrantV1 {
        principal_actor: a1,
        delegate_pubkey: pk_relayer,
        max_bits_per_tx: 100,
        max_bits_total: 150,
        spent_bits: 0,
        expires_at_height: 10,
        allow_purchase_split: true,
        revoked: false,
    })
    .unwrap();

    // Depth-2 transitive sub-delegation rejected fail-closed!
    // Bind relayer actor first so actor_active_key knows a_relayer -> pk_relayer
    let rel_pop = sign_identity_bind_pop_with_seed(
        &s_relayer,
        cfg.chain_id,
        &gid,
        &a_relayer,
        BitsIdentityProviderV1::GameladderPlayerHash,
        &[0x88; 32],
    )
    .unwrap();
    let rel_bind = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        gid,
        a_relayer,
        &s_relayer,
        1,
        20,
        BitsTxPayloadV1::IdentityBind(BitsIdentityBindV1 {
            actor_id: a_relayer,
            ed25519_pk: pk_relayer,
            provider: BitsIdentityProviderV1::GameladderPlayerHash,
            external_id_hash: [0x88; 32],
            pop_signature: rel_pop,
        }),
    )
    .unwrap();
    st.apply_signed_tx(&rel_bind, 2).unwrap();
    assert!(matches!(
        st.register_purchase_delegation(BitsDelegationGrantV1 {
            principal_actor: a_relayer,
            delegate_pubkey: pk_third,
            max_bits_per_tx: 50,
            max_bits_total: 50,
            spent_bits: 0,
            expires_at_height: 10,
            allow_purchase_split: true,
            revoked: false,
        }),
        Err(BitsEconomy1Error::Unauthorized(_))
    ));

    // Delegated PurchaseSplit within cap succeeds (80/20 of 99 -> 79 creator + 20 treasury)
    let (c99, p99) = compute_purchase_split(99, 8_000).unwrap();
    assert_eq!((c99, p99), (79, 20));
    let del_split_tx = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        gid,
        a1,
        &s_relayer,
        1,
        20,
        BitsTxPayloadV1::PurchaseSplit(BitsPurchaseSplitV1 {
            buyer: a1,
            creator: a_creator,
            treasury: cfg.platform_treasury_actor,
            amount_bits: 99,
            creator_bps: 8_000,
            order_id: [0x91; 32],
            memo_hash: [0x92; 32],
        }),
    )
    .unwrap();
    st.apply_signed_tx(&del_split_tx, 3).unwrap();
    assert_eq!(st.balance_of(&a1), 901);
    assert_eq!(st.balance_of(&a_creator), 79);
    assert_eq!(st.treasury_balance(), 20);

    // Delegated Transfer is rejected (wrong scope); over-cap delegated PurchaseSplit rejected
    let bad_del_transfer = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        gid,
        a1,
        &s_relayer,
        2,
        20,
        BitsTxPayloadV1::Transfer(BitsTransferV1 {
            from: a1,
            to: a_creator,
            amount_bits: 10,
            memo_hash: [0u8; 32],
        }),
    )
    .unwrap();
    assert!(matches!(
        st.apply_signed_tx(&bad_del_transfer, 4),
        Err(BitsEconomy1Error::Unauthorized(_))
    ));

    // Amendment 1: rotate a1's key pk1 -> pk1_rot (preserving ActorIdV1 & balance 901!)
    let rot_pop = sign_identity_bind_pop_with_seed(
        &s1_rotated,
        cfg.chain_id,
        &gid,
        &a1,
        BitsIdentityProviderV1::Discord,
        &[0xDD; 32],
    )
    .unwrap();
    let rot_tx = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        gid,
        a1,
        &s1,
        2,
        20,
        BitsTxPayloadV1::IdentityBind(BitsIdentityBindV1 {
            actor_id: a1,
            ed25519_pk: pk1_rot,
            provider: BitsIdentityProviderV1::Discord,
            external_id_hash: [0xDD; 32],
            pop_signature: rot_pop,
        }),
    )
    .unwrap();
    st.apply_signed_tx(&rot_tx, 5).unwrap();

    // Lookup by new pk1_rot resolves to the EXACT same ActorIdV1 and balance 901!
    let wp_rot = st.prove_wallet(*pk1_rot.as_bytes()).unwrap();
    assert_eq!(wp_rot.actor_id, a1);
    assert_eq!(wp_rot.balance_bits, 901);
    assert_eq!(
        verify_bits_wallet_proof(st.balance_root(), &wp_rot).unwrap(),
        901
    );

    // Old key pk1 AND stale delegation are both rejected after rotation!
    let old_key_tx = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        gid,
        a1,
        &s1,
        3,
        20,
        BitsTxPayloadV1::Transfer(BitsTransferV1 {
            from: a1,
            to: a_creator,
            amount_bits: 10,
            memo_hash: [0u8; 32],
        }),
    )
    .unwrap();
    assert!(matches!(
        st.apply_signed_tx(&old_key_tx, 6),
        Err(BitsEconomy1Error::Unauthorized(_))
    ));
    let stale_del_tx = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        gid,
        a1,
        &s_relayer,
        3,
        20,
        BitsTxPayloadV1::PurchaseSplit(BitsPurchaseSplitV1 {
            buyer: a1,
            creator: a_creator,
            treasury: cfg.platform_treasury_actor,
            amount_bits: 10,
            creator_bps: 8_000,
            order_id: [0x93; 32],
            memo_hash: [0x94; 32],
        }),
    )
    .unwrap();
    assert!(matches!(
        st.apply_signed_tx(&stale_del_tx, 6),
        Err(BitsEconomy1Error::Unauthorized(_))
    ));

    // New rotated key pk1_rot spends smoothly!
    let new_key_tx = BitsSignedTxV1::sign_with_seed(
        cfg.chain_id,
        gid,
        a1,
        &s1_rotated,
        3,
        20,
        BitsTxPayloadV1::Transfer(BitsTransferV1 {
            from: a1,
            to: a_creator,
            amount_bits: 51,
            memo_hash: [0u8; 32],
        }),
    )
    .unwrap();
    st.apply_signed_tx(&new_key_tx, 6).unwrap();
    assert_eq!(st.balance_of(&a1), 850);
    assert_eq!(st.balance_of(&a_creator), 130);

    // Structural lab == false hard refusal for TestnetGrant AND SimulatedPayout!
    let mut non_lab = cfg;
    non_lab.lab = false;
    non_lab.initial_grant_pool = 0;
    let mut st_non_lab = BitsEconomyStateV1::from_genesis(
        non_lab,
        &[(a1, pk1, non_lab.genesis_supply)],
        &[pk_relayer],
    )
    .unwrap();
    let nl_gid = st_non_lab.genesis_id();
    let nl_grant = BitsSignedTxV1::sign_with_seed(
        non_lab.chain_id,
        nl_gid,
        non_lab.grant_pool_actor,
        &s_relayer,
        1,
        20,
        BitsTxPayloadV1::TestnetGrant(BitsTestnetGrantV1 {
            recipient: a_creator,
            recipient_pubkey: pk_creator,
            amount_bits: 50,
            grant_kind: BitsGrantKindV1::Faucet,
            grant_id: [0xAA; 32],
        }),
    )
    .unwrap();
    assert!(matches!(
        st_non_lab.apply_signed_tx(&nl_grant, 1),
        Err(BitsEconomy1Error::TestnetOnlyDisabled(_))
    ));
    let nl_payout = BitsSignedTxV1::sign_with_seed(
        non_lab.chain_id,
        nl_gid,
        a1,
        &s1,
        1,
        20,
        BitsTxPayloadV1::SimulatedPayout(BitsSimulatedPayoutV1 {
            creator: a1,
            amount_bits: 100,
            payout_request_id: [0xBB; 32],
        }),
    )
    .unwrap();
    assert!(matches!(
        st_non_lab.apply_signed_tx(&nl_payout, 1),
        Err(BitsEconomy1Error::TestnetOnlyDisabled(_))
    ));
}
