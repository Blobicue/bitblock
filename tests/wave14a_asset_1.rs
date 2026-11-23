//! Wave 14A integration suite for `ASSET-1` (`PS-038`):
//! dual-domain authenticated balance & occupancy-episode roots, transfer-carries-deposit-claim,
//! burn-after-transfer refusal, bedrock exclusion, concurrent write rollback, `IDENTITY-1` /
//! `AUTH-COMMIT-1` cryptographic authorization + replay protection, and cross-domain isolation.

#![cfg(feature = "protocol-research")]

use bitblock::asset_1::{
    AssetDispositionV1, AssetLedgerStateV1, AssetOperationV1, AuthorizedAssetCallV1,
    EpisodeOwnershipValueV1, episode_trie_key,
};
use bitblock::asset_d1::AssetPolicyDescriptorV1;
use bitblock::auth_commit_1::{AuthCommit1Error, AuthCommitStateV1, DomainIdV1};
use bitblock::crypto_d1::{
    CanonicalEd25519PublicKeyV1, HashSuiteIdV1, SignatureSuiteIdV1, SigningDomainTagV1,
    sign_preimage_deterministic,
};
use bitblock::history::world_tag;
use bitblock::identity_1::{ActorAuthStateV1, IdentityContextV1};
use bitblock::identity_d1::{
    ActorIdV1, AuthorizationDescriptorV1, CredentialRoleV1, NetworkEditOpV1, ThresholdPolicyV1,
    TransactionBodyV1,
};
use bitblock::world::{BEDROCK_COORD, Coord, World};

fn pk(seed: u8) -> CanonicalEd25519PublicKeyV1 {
    CanonicalEd25519PublicKeyV1::from_secret_seed(&[seed; 32]).unwrap()
}

fn make_ctx() -> IdentityContextV1 {
    IdentityContextV1 {
        protocol_version: 1,
        chain_id: 77,
        genesis_id: [0x11u8; 32],
        launch_manifest_id: [0x22u8; 32],
        profile_id: [0x44u8; 32],
        world_tag: world_tag(&World::new()),
        signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
        hash_suite: HashSuiteIdV1::Sha256V1,
    }
}

fn make_actor(ctx: &IdentityContextV1, seed: u8, salt: &str) -> (ActorAuthStateV1, u8) {
    let desc = AuthorizationDescriptorV1 {
        descriptor_version: 0,
        action_policy: ThresholdPolicyV1 {
            role: CredentialRoleV1::ActorAction,
            threshold: 1,
            keys: vec![pk(seed)],
        },
        admin_policy: ThresholdPolicyV1 {
            role: CredentialRoleV1::ActorAdmin,
            threshold: 1,
            keys: vec![pk(seed + 100)],
        },
        delegations: vec![],
        revoked: false,
    };
    let actor_id = ActorIdV1::derive(ctx.chain_id, salt, &desc).expect("derive");
    let state = ActorAuthStateV1 {
        state_version: 1,
        actor_id,
        descriptor: desc,
        next_nonce: 0,
        recovery_guardians: ThresholdPolicyV1 {
            role: CredentialRoleV1::ActorAdmin,
            threshold: 1,
            keys: vec![pk(seed + 150)],
        },
        recovery_delay_heights: 100,
        pending_recovery: None,
    };
    (state, seed)
}

fn sign_call(
    ctx: &IdentityContextV1,
    op: AssetOperationV1,
    actor_id: ActorIdV1,
    nonce: u64,
    signer_seed: u8,
) -> AuthorizedAssetCallV1 {
    let tx = TransactionBodyV1 {
        actor_id,
        nonce,
        valid_after: 1,
        valid_until: 1_000,
        expected_state_root: Some(ctx.world_tag.digest()),
        max_fee: 1_000_000,
        operations: vec![NetworkEditOpV1::On(
            op.target_coord().unwrap_or(Coord::new(1, 0, 0)),
        )],
    };
    let preimage = ctx.build_preimage(
        SigningDomainTagV1::ActorTransaction,
        tx.tag().expect("tx tag").digest(),
    );
    let sig = sign_preimage_deterministic(&[signer_seed; 32], &preimage).expect("sign");
    AuthorizedAssetCallV1 {
        operation: op,
        tx,
        signer_actor_id: actor_id,
        preimage,
        raw_signatures: vec![(*pk(signer_seed).as_bytes(), *sig.as_bytes())],
    }
}

#[test]
fn asset_1_authorized_lifecycle_transfer_deposit_burn_and_conservation() {
    let ctx = make_ctx();
    let (state_a, seed_a) = make_actor(&ctx, 11, "asset-a");
    let (state_b, seed_b) = make_actor(&ctx, 21, "asset-b");
    let id_a = state_a.actor_id;
    let id_b = state_b.actor_id;

    let mut auth = AuthCommitStateV1::from_genesis(ctx, vec![state_a, state_b]).expect("auth");
    let policy = AssetPolicyDescriptorV1::v1_dual_domain_default();
    let mut ledger = AssetLedgerStateV1::from_genesis(
        policy,
        ctx.profile_id,
        &[(id_a, 6_000_000), (id_b, 4_000_000)],
    )
    .expect("ledger");

    let coord = Coord::new(7, -3, 19);
    let mint_id = [0x5Au8; 32];

    // 1. Actor A mints episode at coord (burns 1_000, locks 1_000_000).
    let call_mint = sign_call(
        &ctx,
        AssetOperationV1::MintEpisode {
            actor: id_a,
            coord,
            mint_episode_id: mint_id,
        },
        id_a,
        0,
        seed_a,
    );
    let r_mint = ledger
        .apply_authorized_batch(&mut auth, &[call_mint], 10)
        .expect("mint");
    assert_eq!(r_mint.disposition, AssetDispositionV1::Admitted);
    assert_eq!(ledger.balance_of(&id_a), 6_000_000 - 1_001_000);
    assert_eq!(ledger.episode_at(coord).unwrap().owner, id_a);

    // 2. Actor A transfers episode to Actor B (carries 1_000_000 deposit claim, keeps mint_id).
    let call_xfer = sign_call(
        &ctx,
        AssetOperationV1::TransferEpisode {
            from: id_a,
            to: id_b,
            coord,
        },
        id_a,
        1,
        seed_a,
    );
    let r_xfer = ledger
        .apply_authorized_batch(&mut auth, &[call_xfer], 20)
        .expect("transfer");
    assert_eq!(r_xfer.disposition, AssetDispositionV1::Admitted);
    let ep_after_xfer = ledger.episode_at(coord).expect("still occupied");
    assert_eq!(ep_after_xfer.owner, id_b);
    assert_eq!(ep_after_xfer.mint_episode_id, mint_id);
    assert_eq!(ep_after_xfer.locked_deposit, 1_000_000);

    // 3. Old owner A attempts burn-after-transfer -> MUST fail closed and roll back nonce!
    let pre_asset_roots = ledger.roots().expect("pre asset roots");
    let pre_auth_roots = auth.roots();
    let call_burn_old = sign_call(
        &ctx,
        AssetOperationV1::BurnEpisode { actor: id_a, coord },
        id_a,
        2,
        seed_a,
    );
    let r_old_burn = ledger
        .apply_authorized_batch(&mut auth, &[call_burn_old], 30)
        .expect("old burn rejected");
    assert_eq!(
        r_old_burn.disposition,
        AssetDispositionV1::RejectedNotEpisodeOwner
    );
    assert_eq!(ledger.roots().expect("unchanged asset"), pre_asset_roots);
    assert_eq!(auth.roots(), pre_auth_roots);

    // 4. New owner B burns episode -> receives 1_000_000 deposit refund minus 1_000 fee!
    let call_burn_b = sign_call(
        &ctx,
        AssetOperationV1::BurnEpisode { actor: id_b, coord },
        id_b,
        0,
        seed_b,
    );
    let r_burn_b = ledger
        .apply_authorized_batch(&mut auth, &[call_burn_b], 40)
        .expect("burn by B");
    assert_eq!(r_burn_b.disposition, AssetDispositionV1::Admitted);
    assert_eq!(r_burn_b.deposits_refunded_delta, 1_000_000);
    assert_eq!(ledger.balance_of(&id_b), 4_000_000 + 1_000_000 - 1_000);
    assert!(ledger.episode_at(coord).is_none());

    // 5. Absence proof for burned coord and inclusion proof for B's updated balance.
    let final_roots = ledger.roots().expect("final roots");
    let ep_abs = ledger.prove_episode(coord).expect("ep abs");
    assert_eq!(
        ep_abs
            .verify(
                DomainIdV1::EpisodeOwnership,
                final_roots.episode_ownership_root,
                &episode_trie_key(coord),
            )
            .expect("verify absence"),
        None
    );
    let bal_b_inc = ledger.prove_balance(&id_b).expect("bal inc");
    let bal_bytes = bal_b_inc
        .verify(
            DomainIdV1::NativeBalance,
            final_roots.native_balance_root,
            id_b.as_bytes(),
        )
        .expect("verify balance")
        .expect("present");
    assert_eq!(
        u128::from_be_bytes(bal_bytes.try_into().expect("16")),
        4_999_000
    );

    // Cross-domain proof rejection: NativeBalance proof MUST fail under EpisodeOwnership & Auth.
    assert!(matches!(
        bal_b_inc.verify(
            DomainIdV1::EpisodeOwnership,
            final_roots.episode_ownership_root,
            id_b.as_bytes(),
        ),
        Err(AuthCommit1Error::CrossDomainProofRejected { .. })
    ));
    assert!(matches!(
        bal_b_inc.verify(
            DomainIdV1::Authorization,
            auth.roots().authorization_root,
            id_b.as_bytes(),
        ),
        Err(AuthCommit1Error::CrossDomainProofRejected { .. })
    ));
    ledger.verify_rebuild_parity().expect("rebuild parity");
    ledger.verify_conservation().expect("conservation");
}

#[test]
fn asset_1_adversarial_bedrock_replay_concurrent_conflict_and_rollback() {
    let ctx = make_ctx();
    let (state_a, seed_a) = make_actor(&ctx, 31, "adv-a");
    let (state_b, seed_b) = make_actor(&ctx, 41, "adv-b");
    let (state_c, _seed_c) = make_actor(&ctx, 51, "adv-c");
    let id_a = state_a.actor_id;
    let id_b = state_b.actor_id;
    let id_c = state_c.actor_id;

    let mut auth =
        AuthCommitStateV1::from_genesis(ctx, vec![state_a, state_b, state_c]).expect("auth");
    let policy = AssetPolicyDescriptorV1::v1_dual_domain_default();
    let mut ledger = AssetLedgerStateV1::from_genesis(
        policy,
        ctx.profile_id,
        &[(id_a, 5_000_000), (id_b, 3_000_000), (id_c, 2_000_000)],
    )
    .expect("ledger");

    // 1. Bedrock mint MUST fail closed with zero state change.
    let pre_roots = ledger.roots().expect("pre");
    let bedrock_call = sign_call(
        &ctx,
        AssetOperationV1::MintEpisode {
            actor: id_a,
            coord: BEDROCK_COORD,
            mint_episode_id: [0x01u8; 32],
        },
        id_a,
        0,
        seed_a,
    );
    let r_bed = ledger
        .apply_authorized_batch(&mut auth, &[bedrock_call], 5)
        .expect("bedrock");
    assert_eq!(
        r_bed.disposition,
        AssetDispositionV1::RejectedBedrockForbidden
    );
    assert_eq!(ledger.roots().expect("unchanged"), pre_roots);

    // 2. Valid native transfer A -> B (nonce 0), then stale nonce 0 replay MUST fail closed.
    let call_t0 = sign_call(
        &ctx,
        AssetOperationV1::TransferNative {
            from: id_a,
            to: id_b,
            amount: 500_000,
        },
        id_a,
        0,
        seed_a,
    );
    assert_eq!(
        ledger
            .apply_authorized_batch(&mut auth, std::slice::from_ref(&call_t0), 10)
            .expect("t0")
            .disposition,
        AssetDispositionV1::Admitted
    );
    let roots_after_t0 = ledger.roots().expect("after t0");
    let r_replay = ledger
        .apply_authorized_batch(&mut auth, &[call_t0], 11)
        .expect("replay");
    assert_eq!(
        r_replay.disposition,
        AssetDispositionV1::RejectedReplayOrNonce
    );
    assert_eq!(ledger.roots().expect("unchanged"), roots_after_t0);

    // 3. Unauthorized signature (B signs A's op) MUST fail closed.
    let call_bad_sig = sign_call(
        &ctx,
        AssetOperationV1::TransferNative {
            from: id_a,
            to: id_b,
            amount: 100_000,
        },
        id_a,
        1,
        seed_b,
    );
    assert_eq!(
        ledger
            .apply_authorized_batch(&mut auth, &[call_bad_sig], 12)
            .expect("bad sig")
            .disposition,
        AssetDispositionV1::RejectedUnauthorized
    );

    // 4. Mint coord (9,9,9), then concurrent double-transfer of (9,9,9) in ONE batch -> atomic rollback!
    let coord = Coord::new(9, 9, 9);
    let mint_ok = sign_call(
        &ctx,
        AssetOperationV1::MintEpisode {
            actor: id_a,
            coord,
            mint_episode_id: [0x99u8; 32],
        },
        id_a,
        1,
        seed_a,
    );
    assert_eq!(
        ledger
            .apply_authorized_batch(&mut auth, &[mint_ok], 15)
            .expect("mint")
            .disposition,
        AssetDispositionV1::Admitted
    );
    let roots_pre_conflict = ledger.roots().expect("pre conflict");
    let auth_pre_conflict = auth.roots();
    let conflict_1 = sign_call(
        &ctx,
        AssetOperationV1::TransferEpisode {
            from: id_a,
            to: id_b,
            coord,
        },
        id_a,
        2,
        seed_a,
    );
    let conflict_2 = sign_call(
        &ctx,
        AssetOperationV1::TransferEpisode {
            from: id_a,
            to: id_c,
            coord,
        },
        id_a,
        3,
        seed_a,
    );
    let r_conflict = ledger
        .apply_authorized_batch(&mut auth, &[conflict_1, conflict_2], 20)
        .expect("conflict");
    assert_eq!(
        r_conflict.disposition,
        AssetDispositionV1::RejectedConcurrentWrite
    );
    assert_eq!(ledger.roots().expect("rollback asset"), roots_pre_conflict);
    assert_eq!(auth.roots(), auth_pre_conflict);
    let ep_val = ledger.episode_at(coord).expect("still A");
    assert_eq!(
        EpisodeOwnershipValueV1::decode_canonical(&ep_val.encode_canonical()).unwrap(),
        ep_val
    );
    assert_eq!(ep_val.owner, id_a);
    ledger.verify_rebuild_parity().expect("rebuild parity");
    ledger.verify_conservation().expect("conservation");
}
