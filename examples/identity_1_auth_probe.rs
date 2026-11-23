//! Release probe for Wave 12A (`IDENTITY-1`): exercises non-transitive depth-1 delegation,
//! two-step timelocked recovery (cancel vs finalize), all adversarial authorization dispositions,
//! and 200-run release latency benchmarks, emitting structured JSON to stdout.

use std::time::Instant;

use bitblock::crypto_d1::{
    CanonicalEd25519PublicKeyV1, HashSuiteIdV1, SignatureSuiteIdV1, SigningDomainTagV1,
    sign_preimage_deterministic,
};
use bitblock::history::world_tag;
use bitblock::identity_1::{
    ActorAuthStateV1, IdentityContextV1, IdentityDispositionV1, IdentityRegistryV1,
};
use bitblock::identity_d1::{
    ActorIdV1, AuthorizationDescriptorV1, CredentialRoleV1, DelegationGrantV1, NetworkEditOpV1,
    ThresholdPolicyV1, TransactionBodyV1,
};
use bitblock::world::{Coord, World};

fn mk_pk(seed_byte: u8) -> CanonicalEd25519PublicKeyV1 {
    CanonicalEd25519PublicKeyV1::from_secret_seed(&[seed_byte; 32]).unwrap()
}

fn mk_policy(role: CredentialRoleV1, seed_bytes: &[u8], threshold: u16) -> ThresholdPolicyV1 {
    let mut keys: Vec<_> = seed_bytes.iter().map(|&b| mk_pk(b)).collect();
    keys.sort();
    ThresholdPolicyV1 {
        role,
        threshold,
        keys,
    }
}

fn median_us(samples: &mut [f64]) -> f64 {
    samples.sort_by(|a, b| a.total_cmp(b));
    samples[samples.len() / 2]
}

fn main() {
    let world = World::new();
    let w_tag = world_tag(&world);
    let ctx = IdentityContextV1 {
        protocol_version: 1,
        chain_id: 77,
        genesis_id: [0x11; 32],
        launch_manifest_id: [0x22; 32],
        profile_id: [0x33; 32],
        world_tag: w_tag,
        signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
        hash_suite: HashSuiteIdV1::Sha256V1,
    };

    let desc_c = AuthorizationDescriptorV1 {
        descriptor_version: 0,
        action_policy: mk_policy(CredentialRoleV1::ActorAction, &[31], 1),
        admin_policy: mk_policy(CredentialRoleV1::ActorAdmin, &[32], 1),
        delegations: vec![],
        revoked: false,
    };
    let actor_c = ActorIdV1::derive(ctx.chain_id, "actor-c", &desc_c).unwrap();

    let desc_b = AuthorizationDescriptorV1 {
        descriptor_version: 0,
        action_policy: mk_policy(CredentialRoleV1::ActorAction, &[21], 1),
        admin_policy: mk_policy(CredentialRoleV1::ActorAdmin, &[22], 1),
        delegations: vec![DelegationGrantV1 {
            delegate_actor_id: actor_c,
            allow_on: true,
            allow_off: true,
            expires_at_height: 200,
        }],
        revoked: false,
    };
    let actor_b = ActorIdV1::derive(ctx.chain_id, "actor-b", &desc_b).unwrap();

    let desc_a = AuthorizationDescriptorV1 {
        descriptor_version: 0,
        action_policy: mk_policy(CredentialRoleV1::ActorAction, &[11], 1),
        admin_policy: mk_policy(CredentialRoleV1::ActorAdmin, &[12], 1),
        delegations: vec![DelegationGrantV1 {
            delegate_actor_id: actor_b,
            allow_on: true,
            allow_off: false,
            expires_at_height: 100,
        }],
        revoked: false,
    };
    let actor_a = ActorIdV1::derive(ctx.chain_id, "actor-a", &desc_a).unwrap();

    let state_a = ActorAuthStateV1 {
        state_version: 1,
        actor_id: actor_a,
        descriptor: desc_a.clone(),
        next_nonce: 0,
        recovery_guardians: mk_policy(CredentialRoleV1::ActorAdmin, &[13], 1),
        recovery_delay_heights: 100,
        pending_recovery: None,
    };
    let state_b = ActorAuthStateV1 {
        state_version: 1,
        actor_id: actor_b,
        descriptor: desc_b,
        next_nonce: 0,
        recovery_guardians: mk_policy(CredentialRoleV1::ActorAdmin, &[23], 1),
        recovery_delay_heights: 100,
        pending_recovery: None,
    };
    let state_c = ActorAuthStateV1 {
        state_version: 1,
        actor_id: actor_c,
        descriptor: desc_c,
        next_nonce: 0,
        recovery_guardians: mk_policy(CredentialRoleV1::ActorAdmin, &[33], 1),
        recovery_delay_heights: 100,
        pending_recovery: None,
    };

    let mut reg = IdentityRegistryV1::new(ctx);
    let val_pk = mk_pk(99);
    reg.register_validator_key(val_pk).unwrap();
    reg.register_actor(state_c).unwrap();
    reg.register_actor(state_b).unwrap();
    reg.register_actor(state_a.clone()).unwrap();

    // 1. Direct authorized tx (nonce 0 -> 1)
    let tx0 = TransactionBodyV1 {
        actor_id: actor_a,
        nonce: 0,
        valid_after: 1,
        valid_until: 50,
        expected_state_root: Some(w_tag.digest()),
        max_fee: 100,
        operations: vec![NetworkEditOpV1::On(Coord::new(1, 0, 0))],
    };
    let pre0 = ctx.build_preimage(
        SigningDomainTagV1::ActorTransaction,
        tx0.tag().unwrap().digest(),
    );
    let sig0 = sign_preimage_deterministic(&[11; 32], &pre0).unwrap();
    let direct_rcpt = reg.authorize_transaction(
        &tx0,
        actor_a,
        &pre0,
        &[(*mk_pk(11).as_bytes(), *sig0.as_bytes())],
        10,
    );
    assert_eq!(direct_rcpt.disposition, IdentityDispositionV1::Admitted);

    // 2. Depth-1 delegated On tx (nonce 1 -> 2)
    let tx1 = TransactionBodyV1 {
        actor_id: actor_a,
        nonce: 1,
        valid_after: 1,
        valid_until: 120,
        expected_state_root: Some(w_tag.digest()),
        max_fee: 100,
        operations: vec![NetworkEditOpV1::On(Coord::new(2, 0, 0))],
    };
    let pre1 = ctx.build_preimage(
        SigningDomainTagV1::ActorTransaction,
        tx1.tag().unwrap().digest(),
    );
    let sig1_b = sign_preimage_deterministic(&[21; 32], &pre1).unwrap();
    let del_rcpt = reg.authorize_transaction(
        &tx1,
        actor_b,
        &pre1,
        &[(*mk_pk(21).as_bytes(), *sig1_b.as_bytes())],
        20,
    );
    assert_eq!(del_rcpt.disposition, IdentityDispositionV1::Admitted);

    // Adversarial matrix checks (8 deterministic gates)
    let tx2 = TransactionBodyV1 {
        actor_id: actor_a,
        nonce: 2,
        valid_after: 1,
        valid_until: 200,
        expected_state_root: Some(w_tag.digest()),
        max_fee: 100,
        operations: vec![NetworkEditOpV1::On(Coord::new(3, 0, 0))],
    };
    let pre2 = ctx.build_preimage(
        SigningDomainTagV1::ActorTransaction,
        tx2.tag().unwrap().digest(),
    );
    let sig2_a = sign_preimage_deterministic(&[11; 32], &pre2).unwrap();

    // Gate 1: Mutated signature + torsion key
    let mut bad_sig = *sig2_a.as_bytes();
    bad_sig[0] ^= 0x01;
    let g1_mut = reg.authorize_transaction(
        &tx2,
        actor_a,
        &pre2,
        &[(*mk_pk(11).as_bytes(), bad_sig)],
        25,
    );
    let g1_tor =
        reg.authorize_transaction(&tx2, actor_a, &pre2, &[([0u8; 32], *sig2_a.as_bytes())], 25);
    assert_eq!(
        g1_mut.disposition,
        IdentityDispositionV1::RejectedInvalidOrDuplicateSignature
    );
    assert_eq!(
        g1_tor.disposition,
        IdentityDispositionV1::RejectedInvalidOrDuplicateSignature
    );

    // Gate 2: Duplicate signer / wrong domain tag
    let g2_dup = reg.authorize_transaction(
        &tx2,
        actor_a,
        &pre2,
        &[
            (*mk_pk(11).as_bytes(), *sig2_a.as_bytes()),
            (*mk_pk(11).as_bytes(), *sig2_a.as_bytes()),
        ],
        25,
    );
    let mut wrong_dom = pre2;
    wrong_dom.domain_tag = SigningDomainTagV1::GovernanceAction;
    let g2_dom = reg.authorize_transaction(
        &tx2,
        actor_a,
        &wrong_dom,
        &[(*mk_pk(11).as_bytes(), *sig2_a.as_bytes())],
        25,
    );
    assert_eq!(
        g2_dup.disposition,
        IdentityDispositionV1::RejectedInvalidOrDuplicateSignature
    );
    assert_eq!(
        g2_dom.disposition,
        IdentityDispositionV1::RejectedDomainBindingMismatch
    );

    // Gate 3: Cross-chain / cross-genesis / cross-world replay
    let mut wrong_chain = pre2;
    wrong_chain.chain_id = 999;
    let g3_chain = reg.authorize_transaction(
        &tx2,
        actor_a,
        &wrong_chain,
        &[(*mk_pk(11).as_bytes(), *sig2_a.as_bytes())],
        25,
    );
    let mut wrong_world = ctx;
    wrong_world.world_tag =
        bitblock::protocol::ContentTag::from_digest(bitblock::protocol::TagKind::World, [0xAA; 32]);
    let pre_ww = wrong_world.build_preimage(
        SigningDomainTagV1::ActorTransaction,
        tx2.tag().unwrap().digest(),
    );
    let g3_world = reg.authorize_transaction(
        &tx2,
        actor_a,
        &pre_ww,
        &[(*mk_pk(11).as_bytes(), *sig2_a.as_bytes())],
        25,
    );
    assert_eq!(
        g3_chain.disposition,
        IdentityDispositionV1::RejectedDomainBindingMismatch
    );
    assert_eq!(
        g3_world.disposition,
        IdentityDispositionV1::RejectedDomainBindingMismatch
    );

    // Gate 4: Validator key cross-domain use
    let val_sig = sign_preimage_deterministic(&[99; 32], &pre2).unwrap();
    let g4_val = reg.authorize_transaction(
        &tx2,
        actor_a,
        &pre2,
        &[(*val_pk.as_bytes(), *val_sig.as_bytes())],
        25,
    );
    assert_eq!(
        g4_val.disposition,
        IdentityDispositionV1::RejectedValidatorKeyDomainCrossUse
    );

    // Gate 5: Stale replay & future gap nonce
    let g5_stale = reg.authorize_transaction(
        &tx0,
        actor_a,
        &pre0,
        &[(*mk_pk(11).as_bytes(), *sig0.as_bytes())],
        25,
    );
    let mut tx_future = tx2.clone();
    tx_future.nonce = 9;
    let pre_fut = ctx.build_preimage(
        SigningDomainTagV1::ActorTransaction,
        tx_future.tag().unwrap().digest(),
    );
    let sig_fut = sign_preimage_deterministic(&[11; 32], &pre_fut).unwrap();
    let g5_fut = reg.authorize_transaction(
        &tx_future,
        actor_a,
        &pre_fut,
        &[(*mk_pk(11).as_bytes(), *sig_fut.as_bytes())],
        25,
    );
    assert_eq!(
        g5_stale.disposition,
        IdentityDispositionV1::RejectedStaleOrFutureNonce
    );
    assert_eq!(
        g5_fut.disposition,
        IdentityDispositionV1::RejectedStaleOrFutureNonce
    );

    // Gate 6: Depth-2 transitive sub-delegation (A -> B -> C forbidden!)
    let sig2_c = sign_preimage_deterministic(&[31; 32], &pre2).unwrap();
    let g6_trans = reg.authorize_transaction(
        &tx2,
        actor_c,
        &pre2,
        &[(*mk_pk(31).as_bytes(), *sig2_c.as_bytes())],
        25,
    );
    assert_eq!(
        g6_trans.disposition,
        IdentityDispositionV1::RejectedTransitiveDelegation
    );

    // Gate 7: Scope denial (Off denied) & expired delegation (h=101 > 100)
    let mut tx_off = tx2.clone();
    tx_off.operations = vec![NetworkEditOpV1::Off(Coord::new(1, 0, 0))];
    let pre_off = ctx.build_preimage(
        SigningDomainTagV1::ActorTransaction,
        tx_off.tag().unwrap().digest(),
    );
    let sig_off_b = sign_preimage_deterministic(&[21; 32], &pre_off).unwrap();
    let g7_scope = reg.authorize_transaction(
        &tx_off,
        actor_b,
        &pre_off,
        &[(*mk_pk(21).as_bytes(), *sig_off_b.as_bytes())],
        25,
    );
    let sig2_b = sign_preimage_deterministic(&[21; 32], &pre2).unwrap();
    let g7_exp = reg.authorize_transaction(
        &tx2,
        actor_b,
        &pre2,
        &[(*mk_pk(21).as_bytes(), *sig2_b.as_bytes())],
        101,
    );
    assert_eq!(
        g7_scope.disposition,
        IdentityDispositionV1::RejectedDelegationScopeDenied
    );
    assert_eq!(
        g7_exp.disposition,
        IdentityDispositionV1::RejectedDelegationMissingOrExpired
    );

    // Gate 8: Two-step recovery timelock active + cancel + finalize + revoked actor
    let rec_desc = AuthorizationDescriptorV1 {
        descriptor_version: 1,
        action_policy: mk_policy(CredentialRoleV1::ActorAction, &[41], 1),
        admin_policy: mk_policy(CredentialRoleV1::ActorAdmin, &[42], 1),
        delegations: vec![],
        revoked: false,
    };
    let pre_rec = ctx.build_preimage(
        SigningDomainTagV1::ActorAdminRotation,
        rec_desc.tag().unwrap().digest(),
    );
    let sig_guard = sign_preimage_deterministic(&[13; 32], &pre_rec).unwrap();
    let init_rcpt = reg.initiate_recovery(
        actor_a,
        rec_desc.clone(),
        &pre_rec,
        &[(*mk_pk(13).as_bytes(), *sig_guard.as_bytes())],
        100,
    );
    assert_eq!(init_rcpt.disposition, IdentityDispositionV1::Admitted);
    let g8_early = reg.finalize_recovery(actor_a, 150);
    assert_eq!(
        g8_early.disposition,
        IdentityDispositionV1::RejectedRecoveryTimelockActive
    );
    let sig_admin = sign_preimage_deterministic(&[12; 32], &pre_rec).unwrap();
    let cancel_rcpt = reg.cancel_recovery(
        actor_a,
        &pre_rec,
        &[(*mk_pk(12).as_bytes(), *sig_admin.as_bytes())],
        140,
    );
    assert_eq!(cancel_rcpt.disposition, IdentityDispositionV1::Admitted);
    let init_again = reg.initiate_recovery(
        actor_a,
        rec_desc,
        &pre_rec,
        &[(*mk_pk(13).as_bytes(), *sig_guard.as_bytes())],
        150,
    );
    assert_eq!(init_again.disposition, IdentityDispositionV1::Admitted);
    let fin_rcpt = reg.finalize_recovery(actor_a, 250);
    assert_eq!(fin_rcpt.disposition, IdentityDispositionV1::Admitted);
    assert!(
        reg.actor_state(&actor_a)
            .unwrap()
            .descriptor
            .delegations
            .is_empty()
    );

    // 200-run release latency microbenchmarks
    let mut direct_us = Vec::with_capacity(200);
    let mut del_us = Vec::with_capacity(200);
    let mut rec_us = Vec::with_capacity(200);
    for _ in 0..200 {
        let mut r = IdentityRegistryV1::new(ctx);
        r.register_actor(ActorAuthStateV1 {
            state_version: 1,
            actor_id: actor_b,
            descriptor: AuthorizationDescriptorV1 {
                descriptor_version: 0,
                action_policy: mk_policy(CredentialRoleV1::ActorAction, &[21], 1),
                admin_policy: mk_policy(CredentialRoleV1::ActorAdmin, &[22], 1),
                delegations: vec![],
                revoked: false,
            },
            next_nonce: 0,
            recovery_guardians: mk_policy(CredentialRoleV1::ActorAdmin, &[23], 1),
            recovery_delay_heights: 100,
            pending_recovery: None,
        })
        .unwrap();
        r.register_actor(state_a.clone()).unwrap();

        let t0 = Instant::now();
        let _ = r.authorize_transaction(
            &tx0,
            actor_a,
            &pre0,
            &[(*mk_pk(11).as_bytes(), *sig0.as_bytes())],
            10,
        );
        direct_us.push(t0.elapsed().as_secs_f64() * 1e6);

        let t1 = Instant::now();
        let _ = r.authorize_transaction(
            &tx1,
            actor_b,
            &pre1,
            &[(*mk_pk(21).as_bytes(), *sig1_b.as_bytes())],
            20,
        );
        del_us.push(t1.elapsed().as_secs_f64() * 1e6);

        let rd = AuthorizationDescriptorV1 {
            descriptor_version: 1,
            action_policy: mk_policy(CredentialRoleV1::ActorAction, &[41], 1),
            admin_policy: mk_policy(CredentialRoleV1::ActorAdmin, &[42], 1),
            delegations: vec![],
            revoked: false,
        };
        let _ = r.initiate_recovery(
            actor_a,
            rd,
            &pre_rec,
            &[(*mk_pk(13).as_bytes(), *sig_guard.as_bytes())],
            100,
        );
        let t2 = Instant::now();
        let _ = r.finalize_recovery(actor_a, 200);
        rec_us.push(t2.elapsed().as_secs_f64() * 1e6);
    }

    let direct_p50 = median_us(&mut direct_us);
    let del_p50 = median_us(&mut del_us);
    let rec_p50 = median_us(&mut rec_us);

    println!("{{");
    println!("  \"auth_state_tag\": \"{}\",", state_a.tag().unwrap());
    println!(
        "  \"direct_receipt_tag\": \"{}\",",
        direct_rcpt.tag().unwrap()
    );
    println!(
        "  \"delegated_receipt_tag\": \"{}\",",
        del_rcpt.tag().unwrap()
    );
    println!(
        "  \"recovery_receipt_tag\": \"{}\",",
        fin_rcpt.tag().unwrap()
    );
    println!("  \"adversarial_passed\": 8,");
    println!("  \"adversarial_total\": 8,");
    println!("  \"direct_auth_p50_us\": {:.3},", direct_p50);
    println!("  \"delegated_auth_p50_us\": {:.3},", del_p50);
    println!("  \"recovery_finalize_p50_us\": {:.3},", rec_p50);
    println!("  \"matrix_rows\": [");
    let rows = [
        (
            "Sig Mutation & Torsion Key",
            "RejectedInvalidOrDuplicateSignature",
        ),
        (
            "Duplicate Signer / Wrong Domain",
            "RejectedInvalidSig / DomainMismatch",
        ),
        (
            "Cross-Chain & Cross-World Replay",
            "RejectedDomainBindingMismatch",
        ),
        (
            "Validator Key Cross-Domain Use",
            "RejectedValidatorKeyDomainCrossUse",
        ),
        (
            "Stale Replay & Future Gap Nonce",
            "RejectedStaleOrFutureNonce",
        ),
        (
            "Depth-2 Transitive Delegation",
            "RejectedTransitiveDelegation",
        ),
        (
            "Scope Denial & Expired Grant",
            "RejectedScope / DelegationExpired",
        ),
        (
            "Early Timelock & Cancel/Finalize",
            "RejectedTimelock -> Admitted @h=250",
        ),
    ];
    for (i, (v, d)) in rows.iter().enumerate() {
        let comma = if i + 1 < rows.len() { "," } else { "" };
        println!("    {{\"vector\": \"{v}\", \"disposition\": \"{d}\"}}{comma}");
    }
    println!("  ]");
    println!("}}");
}
