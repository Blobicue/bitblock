#![cfg(feature = "protocol-research")]
//! Wave 12A (`IDENTITY-1`) integration tests: golden tag parity with `tools/identity_1_vectors.py`
//! and complete adversarial authorization matrix (mutation, replay, cross-chain/genesis/world/
//! version, wrong-suite, validator key cross-use, duplicate key, torsion key, stale/future nonce,
//! depth-1 delegation expiry/scope/non-transitivity, timelocked recovery cancel vs finalize, and
//! permanent revocation).

use bitblock::crypto_d1::{
    CanonicalEd25519PublicKeyV1, HashSuiteIdV1, SignatureSuiteIdV1, SigningDomainTagV1,
    sign_preimage_deterministic,
};
use bitblock::history::world_tag;
use bitblock::identity_1::{
    ActorAuthStateV1, Identity1Error, IdentityActionKindV1, IdentityAuthReceiptV1,
    IdentityContextV1, IdentityDispositionV1, IdentityRegistryV1,
};
use bitblock::identity_d1::{
    ActorIdV1, AuthorizationDescriptorV1, CredentialRoleV1, DelegationGrantV1, NetworkEditOpV1,
    ThresholdPolicyV1, TransactionBodyV1,
};
use bitblock::protocol::{ContentTag, TagKind};
use bitblock::world::{Coord, World};

const GOLDEN_AUTH_STATE_A_TAG: &str =
    "#bb.receipt.sha256.hthq4z3wjktuwvlwzg4uilsby2w4g2qia5tky55bk6meccqm26eq";
const GOLDEN_DIRECT_RECEIPT_TAG: &str =
    "#bb.receipt.sha256.63wc5rwka53hfx4nqtais7zz4lsxyfimvkt3mola6mqjqey4indq";
const GOLDEN_DELEGATED_RECEIPT_TAG: &str =
    "#bb.receipt.sha256.q46n6n3ms6vjtbhy3marabz7d4jjhtvq6mjhuwyk7azceajbspmq";
const GOLDEN_RECOVERY_RECEIPT_TAG: &str =
    "#bb.receipt.sha256.yo2g77yhasl2edcaxur5teoh4je2qz4chztcwsnmhqd7txduplha";

fn pk(seed: u8) -> CanonicalEd25519PublicKeyV1 {
    CanonicalEd25519PublicKeyV1::from_secret_seed(&[seed; 32]).unwrap()
}

fn pol(role: CredentialRoleV1, seeds: &[u8], threshold: u16) -> ThresholdPolicyV1 {
    let mut keys: Vec<_> = seeds.iter().map(|&s| pk(s)).collect();
    keys.sort();
    ThresholdPolicyV1 {
        role,
        threshold,
        keys,
    }
}

fn canonical_fixture() -> (
    IdentityContextV1,
    IdentityRegistryV1,
    ActorIdV1,
    ActorIdV1,
    ActorIdV1,
    ActorAuthStateV1,
) {
    let w_tag = world_tag(&World::new());
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
        action_policy: pol(CredentialRoleV1::ActorAction, &[31], 1),
        admin_policy: pol(CredentialRoleV1::ActorAdmin, &[32], 1),
        delegations: vec![],
        revoked: false,
    };
    let actor_c = ActorIdV1::derive(ctx.chain_id, "actor-c", &desc_c).unwrap();
    let desc_b = AuthorizationDescriptorV1 {
        descriptor_version: 0,
        action_policy: pol(CredentialRoleV1::ActorAction, &[21], 1),
        admin_policy: pol(CredentialRoleV1::ActorAdmin, &[22], 1),
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
        action_policy: pol(CredentialRoleV1::ActorAction, &[11], 1),
        admin_policy: pol(CredentialRoleV1::ActorAdmin, &[12], 1),
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
        descriptor: desc_a,
        next_nonce: 0,
        recovery_guardians: pol(CredentialRoleV1::ActorAdmin, &[13], 1),
        recovery_delay_heights: 100,
        pending_recovery: None,
    };
    let state_b = ActorAuthStateV1 {
        state_version: 1,
        actor_id: actor_b,
        descriptor: desc_b,
        next_nonce: 0,
        recovery_guardians: pol(CredentialRoleV1::ActorAdmin, &[23], 1),
        recovery_delay_heights: 100,
        pending_recovery: None,
    };
    let state_c = ActorAuthStateV1 {
        state_version: 1,
        actor_id: actor_c,
        descriptor: desc_c,
        next_nonce: 0,
        recovery_guardians: pol(CredentialRoleV1::ActorAdmin, &[33], 1),
        recovery_delay_heights: 100,
        pending_recovery: None,
    };

    let mut reg = IdentityRegistryV1::new(ctx);
    reg.register_validator_key(pk(99)).unwrap();
    reg.register_actor(state_c).unwrap();
    reg.register_actor(state_b).unwrap();
    reg.register_actor(state_a.clone()).unwrap();
    (ctx, reg, actor_a, actor_b, actor_c, state_a)
}

#[test]
fn identity_1_golden_tags_and_codec_roundtrip() {
    let (ctx, mut reg, actor_a, actor_b, _actor_c, state_a) = canonical_fixture();
    assert_eq!(state_a.tag().unwrap().to_string(), GOLDEN_AUTH_STATE_A_TAG);
    let enc = state_a.encode_canonical().unwrap();
    assert_eq!(ActorAuthStateV1::decode_canonical(&enc).unwrap(), state_a);

    let tx0 = TransactionBodyV1 {
        actor_id: actor_a,
        nonce: 0,
        valid_after: 1,
        valid_until: 50,
        expected_state_root: Some(ctx.world_tag.digest()),
        max_fee: 100,
        operations: vec![NetworkEditOpV1::On(Coord::new(1, 0, 0))],
    };
    let pre0 = ctx.build_preimage(
        SigningDomainTagV1::ActorTransaction,
        tx0.tag().unwrap().digest(),
    );
    let sig0 = sign_preimage_deterministic(&[11; 32], &pre0).unwrap();
    let r0 = reg.authorize_transaction(
        &tx0,
        actor_a,
        &pre0,
        &[(*pk(11).as_bytes(), *sig0.as_bytes())],
        10,
    );
    assert_eq!(r0.disposition, IdentityDispositionV1::Admitted);
    assert_eq!(r0.tag().unwrap().to_string(), GOLDEN_DIRECT_RECEIPT_TAG);
    assert_eq!(
        IdentityAuthReceiptV1::decode_canonical(&r0.encode_canonical().unwrap()).unwrap(),
        r0
    );

    let tx1 = TransactionBodyV1 {
        actor_id: actor_a,
        nonce: 1,
        valid_after: 1,
        valid_until: 120,
        expected_state_root: Some(ctx.world_tag.digest()),
        max_fee: 100,
        operations: vec![NetworkEditOpV1::On(Coord::new(2, 0, 0))],
    };
    let pre1 = ctx.build_preimage(
        SigningDomainTagV1::ActorTransaction,
        tx1.tag().unwrap().digest(),
    );
    let sig1 = sign_preimage_deterministic(&[21; 32], &pre1).unwrap();
    let r1 = reg.authorize_transaction(
        &tx1,
        actor_b,
        &pre1,
        &[(*pk(21).as_bytes(), *sig1.as_bytes())],
        20,
    );
    assert_eq!(r1.disposition, IdentityDispositionV1::Admitted);
    assert_eq!(r1.action_kind, IdentityActionKindV1::DelegatedTransaction);
    assert_eq!(r1.tag().unwrap().to_string(), GOLDEN_DELEGATED_RECEIPT_TAG);
    assert_eq!(reg.actor_state(&actor_a).unwrap().next_nonce, 2);
    assert_eq!(reg.actor_state(&actor_b).unwrap().next_nonce, 0);

    // Recovery finalize golden receipt tag
    let rec_desc = AuthorizationDescriptorV1 {
        descriptor_version: 1,
        action_policy: pol(CredentialRoleV1::ActorAction, &[41], 1),
        admin_policy: pol(CredentialRoleV1::ActorAdmin, &[42], 1),
        delegations: vec![],
        revoked: false,
    };
    let pre_rec = ctx.build_preimage(
        SigningDomainTagV1::ActorAdminRotation,
        rec_desc.tag().unwrap().digest(),
    );
    let sig_g = sign_preimage_deterministic(&[13; 32], &pre_rec).unwrap();
    let r_init = reg.initiate_recovery(
        actor_a,
        rec_desc,
        &pre_rec,
        &[(*pk(13).as_bytes(), *sig_g.as_bytes())],
        150,
    );
    assert_eq!(r_init.disposition, IdentityDispositionV1::Admitted);
    let pending_enc = reg
        .actor_state(&actor_a)
        .unwrap()
        .encode_canonical()
        .unwrap();
    assert_eq!(
        ActorAuthStateV1::decode_canonical(&pending_enc).unwrap(),
        *reg.actor_state(&actor_a).unwrap()
    );
    let r_fin = reg.finalize_recovery(actor_a, 250);
    assert_eq!(r_fin.disposition, IdentityDispositionV1::Admitted);
    assert_eq!(
        r_fin.tag().unwrap().to_string(),
        GOLDEN_RECOVERY_RECEIPT_TAG
    );
    assert_eq!(
        reg.actor_state(&actor_a)
            .unwrap()
            .descriptor
            .descriptor_version,
        1
    );
    assert!(
        reg.actor_state(&actor_a)
            .unwrap()
            .descriptor
            .delegations
            .is_empty()
    );
}

#[test]
fn identity_1_adversarial_matrix_and_key_domain_isolation() {
    let (ctx, mut reg, actor_a, actor_b, actor_c, _) = canonical_fixture();

    // Validator key registration collision against actor key is rejected fail-closed.
    assert_eq!(
        reg.register_validator_key(pk(11)),
        Err(Identity1Error::ValidatorActorKeyCollision)
    );

    let tx = TransactionBodyV1 {
        actor_id: actor_a,
        nonce: 0,
        valid_after: 5,
        valid_until: 50,
        expected_state_root: Some(ctx.world_tag.digest()),
        max_fee: 100,
        operations: vec![NetworkEditOpV1::On(Coord::new(5, 0, 0))],
    };
    let pre = ctx.build_preimage(
        SigningDomainTagV1::ActorTransaction,
        tx.tag().unwrap().digest(),
    );
    let sig_a = sign_preimage_deterministic(&[11; 32], &pre).unwrap();

    // 1. Mutation, torsion key, duplicate signer
    let mut mut_sig = *sig_a.as_bytes();
    mut_sig[15] ^= 0x80;
    assert_eq!(
        reg.authorize_transaction(&tx, actor_a, &pre, &[(*pk(11).as_bytes(), mut_sig)], 10)
            .disposition,
        IdentityDispositionV1::RejectedInvalidOrDuplicateSignature
    );
    assert_eq!(
        reg.authorize_transaction(&tx, actor_a, &pre, &[([0u8; 32], *sig_a.as_bytes())], 10)
            .disposition,
        IdentityDispositionV1::RejectedInvalidOrDuplicateSignature
    );
    assert_eq!(
        reg.authorize_transaction(
            &tx,
            actor_a,
            &pre,
            &[
                (*pk(11).as_bytes(), *sig_a.as_bytes()),
                (*pk(11).as_bytes(), *sig_a.as_bytes())
            ],
            10
        )
        .disposition,
        IdentityDispositionV1::RejectedInvalidOrDuplicateSignature
    );

    // 2. Cross-chain, cross-genesis, cross-version, cross-world, and world-root mismatch
    let mut bad_chain = pre;
    bad_chain.chain_id = 78;
    assert_eq!(
        reg.authorize_transaction(
            &tx,
            actor_a,
            &bad_chain,
            &[(*pk(11).as_bytes(), *sig_a.as_bytes())],
            10
        )
        .disposition,
        IdentityDispositionV1::RejectedDomainBindingMismatch
    );
    let mut bad_gen = pre;
    bad_gen.genesis_id[0] ^= 0xFF;
    assert_eq!(
        reg.authorize_transaction(
            &tx,
            actor_a,
            &bad_gen,
            &[(*pk(11).as_bytes(), *sig_a.as_bytes())],
            10
        )
        .disposition,
        IdentityDispositionV1::RejectedDomainBindingMismatch
    );
    let mut bad_ver = pre;
    bad_ver.protocol_version = 2;
    assert_eq!(
        reg.authorize_transaction(
            &tx,
            actor_a,
            &bad_ver,
            &[(*pk(11).as_bytes(), *sig_a.as_bytes())],
            10
        )
        .disposition,
        IdentityDispositionV1::RejectedDomainBindingMismatch
    );
    let other_world_tag = ContentTag::from_digest(TagKind::World, [0x77; 32]);
    let mut other_world_ctx = ctx;
    other_world_ctx.world_tag = other_world_tag;
    let pre_other_world = other_world_ctx.build_preimage(
        SigningDomainTagV1::ActorTransaction,
        tx.tag().unwrap().digest(),
    );
    assert_eq!(
        reg.authorize_transaction(
            &tx,
            actor_a,
            &pre_other_world,
            &[(*pk(11).as_bytes(), *sig_a.as_bytes())],
            10
        )
        .disposition,
        IdentityDispositionV1::RejectedDomainBindingMismatch
    );
    // Updating registry world_tag flips binding cleanly and restores original tag afterward.
    reg.set_world_tag(other_world_tag);
    assert_eq!(reg.context().world_tag, other_world_tag);
    reg.set_world_tag(ctx.world_tag);
    let mut bad_root_tx = tx.clone();
    bad_root_tx.expected_state_root = Some([0xEE; 32]);
    let pre_bad_root = ctx.build_preimage(
        SigningDomainTagV1::ActorTransaction,
        bad_root_tx.tag().unwrap().digest(),
    );
    assert_eq!(
        reg.authorize_transaction(
            &bad_root_tx,
            actor_a,
            &pre_bad_root,
            &[(*pk(11).as_bytes(), *sig_a.as_bytes())],
            10
        )
        .disposition,
        IdentityDispositionV1::RejectedWorldRootPrecondition
    );

    // 3. Validator key cross-domain use
    let val_sig = sign_preimage_deterministic(&[99; 32], &pre).unwrap();
    assert_eq!(
        reg.authorize_transaction(
            &tx,
            actor_a,
            &pre,
            &[(*pk(99).as_bytes(), *val_sig.as_bytes())],
            10
        )
        .disposition,
        IdentityDispositionV1::RejectedValidatorKeyDomainCrossUse
    );

    // 4. Validity window & nonce replay / future gap
    assert_eq!(
        reg.authorize_transaction(
            &tx,
            actor_a,
            &pre,
            &[(*pk(11).as_bytes(), *sig_a.as_bytes())],
            4
        )
        .disposition,
        IdentityDispositionV1::RejectedValidityWindow
    );
    assert_eq!(
        reg.authorize_transaction(
            &tx,
            actor_a,
            &pre,
            &[(*pk(11).as_bytes(), *sig_a.as_bytes())],
            10
        )
        .disposition,
        IdentityDispositionV1::Admitted
    );
    assert_eq!(
        reg.authorize_transaction(
            &tx,
            actor_a,
            &pre,
            &[(*pk(11).as_bytes(), *sig_a.as_bytes())],
            11
        )
        .disposition,
        IdentityDispositionV1::RejectedStaleOrFutureNonce
    );

    // 5. Depth-2 transitive delegation (A -> B -> C), scope denial (Off), and expiry (h > 100)
    let tx1 = TransactionBodyV1 {
        actor_id: actor_a,
        nonce: 1,
        valid_after: 1,
        valid_until: 150,
        expected_state_root: None,
        max_fee: 100,
        operations: vec![NetworkEditOpV1::On(Coord::new(6, 0, 0))],
    };
    let pre1 = ctx.build_preimage(
        SigningDomainTagV1::ActorTransaction,
        tx1.tag().unwrap().digest(),
    );
    let sig_c = sign_preimage_deterministic(&[31; 32], &pre1).unwrap();
    assert_eq!(
        reg.authorize_transaction(
            &tx1,
            actor_c,
            &pre1,
            &[(*pk(31).as_bytes(), *sig_c.as_bytes())],
            20
        )
        .disposition,
        IdentityDispositionV1::RejectedTransitiveDelegation
    );
    let sig_b = sign_preimage_deterministic(&[21; 32], &pre1).unwrap();
    assert_eq!(
        reg.authorize_transaction(
            &tx1,
            actor_b,
            &pre1,
            &[(*pk(21).as_bytes(), *sig_b.as_bytes())],
            101
        )
        .disposition,
        IdentityDispositionV1::RejectedDelegationMissingOrExpired
    );
    let mut tx_off = tx1.clone();
    tx_off.operations = vec![NetworkEditOpV1::Off(Coord::new(6, 0, 0))];
    let pre_off = ctx.build_preimage(
        SigningDomainTagV1::ActorTransaction,
        tx_off.tag().unwrap().digest(),
    );
    let sig_off_b = sign_preimage_deterministic(&[21; 32], &pre_off).unwrap();
    assert_eq!(
        reg.authorize_transaction(
            &tx_off,
            actor_b,
            &pre_off,
            &[(*pk(21).as_bytes(), *sig_off_b.as_bytes())],
            20
        )
        .disposition,
        IdentityDispositionV1::RejectedDelegationScopeDenied
    );

    // 6. Recovery early-finalize reject -> admin cancel -> permanent revocation blocks everything
    let rec_desc = AuthorizationDescriptorV1 {
        descriptor_version: 1,
        action_policy: pol(CredentialRoleV1::ActorAction, &[41], 1),
        admin_policy: pol(CredentialRoleV1::ActorAdmin, &[42], 1),
        delegations: vec![],
        revoked: false,
    };
    let pre_rec = ctx.build_preimage(
        SigningDomainTagV1::ActorAdminRotation,
        rec_desc.tag().unwrap().digest(),
    );
    let sig_g = sign_preimage_deterministic(&[13; 32], &pre_rec).unwrap();
    assert_eq!(
        reg.initiate_recovery(
            actor_a,
            rec_desc.clone(),
            &pre_rec,
            &[(*pk(13).as_bytes(), *sig_g.as_bytes())],
            100
        )
        .disposition,
        IdentityDispositionV1::Admitted
    );
    assert_eq!(
        reg.finalize_recovery(actor_a, 199).disposition,
        IdentityDispositionV1::RejectedRecoveryTimelockActive
    );
    let sig_adm = sign_preimage_deterministic(&[12; 32], &pre_rec).unwrap();
    assert_eq!(
        reg.cancel_recovery(
            actor_a,
            &pre_rec,
            &[(*pk(12).as_bytes(), *sig_adm.as_bytes())],
            150
        )
        .disposition,
        IdentityDispositionV1::Admitted
    );
    assert_eq!(
        reg.finalize_recovery(actor_a, 250).disposition,
        IdentityDispositionV1::RejectedRecoveryStateInvalid
    );

    // Permanent revocation via ActorAdmin blocks future txs, rotations, and recoveries.
    let revoked_desc = AuthorizationDescriptorV1 {
        descriptor_version: 1,
        action_policy: pol(CredentialRoleV1::ActorAction, &[11], 1),
        admin_policy: pol(CredentialRoleV1::ActorAdmin, &[12], 1),
        delegations: vec![],
        revoked: true,
    };
    let pre_rev = ctx.build_preimage(
        SigningDomainTagV1::ActorAdminRotation,
        revoked_desc.tag().unwrap().digest(),
    );
    let sig_rev = sign_preimage_deterministic(&[12; 32], &pre_rev).unwrap();
    let rev_rcpt = reg.apply_admin_rotation(
        actor_a,
        pol(CredentialRoleV1::ActorAction, &[11], 1),
        pol(CredentialRoleV1::ActorAdmin, &[12], 1),
        vec![],
        true,
        &pre_rev,
        &[(*pk(12).as_bytes(), *sig_rev.as_bytes())],
        160,
    );
    assert_eq!(rev_rcpt.disposition, IdentityDispositionV1::Admitted);
    assert_eq!(rev_rcpt.action_kind, IdentityActionKindV1::AdminRevocation);
    assert_eq!(
        reg.authorize_transaction(
            &tx1,
            actor_a,
            &pre1,
            &[(*pk(11).as_bytes(), *sig_a.as_bytes())],
            161
        )
        .disposition,
        IdentityDispositionV1::RejectedActorRevoked
    );
    assert_eq!(
        reg.initiate_recovery(
            actor_a,
            rec_desc,
            &pre_rec,
            &[(*pk(13).as_bytes(), *sig_g.as_bytes())],
            162
        )
        .disposition,
        IdentityDispositionV1::RejectedActorRevoked
    );
}
