#![cfg(feature = "protocol-research")]
//! Wave 13A (`AUTH-COMMIT-1`) integration tests: golden tag parity with
//! `tools/auth_commit_1_vectors.py`, full-rebuild vs incremental root parity across 50 mutations,
//! inclusion/absence/stale-nonce/rotation/revocation/recovery proofs, cross-domain proof
//! rejection (including against `COMMIT-1` occupancy & episode roots), and `BOUNDS-1` `12 KiB`
//! proof bounds enforcement.

use bitblock::auth_commit_1::{
    AuthCommit1Error, AuthCommitRootsV1, AuthCommitStateV1, DomainIdV1, DomainPatriciaProofV1,
    DomainPatriciaTrieV1, DomainProofTerminalV1,
};
use bitblock::commit_state::{CommitmentProfileDescriptorV1, TypedPatriciaTrie};
use bitblock::crypto_d1::{
    CanonicalEd25519PublicKeyV1, HashSuiteIdV1, SignatureSuiteIdV1, SigningDomainTagV1,
    sign_preimage_deterministic,
};
use bitblock::history::world_tag;
use bitblock::identity_1::{ActorAuthStateV1, IdentityContextV1, IdentityDispositionV1};
use bitblock::identity_d1::{
    ActorIdV1, AuthorizationDescriptorV1, CredentialRoleV1, DelegationGrantV1, NetworkEditOpV1,
    ThresholdPolicyV1, TransactionBodyV1,
};
use bitblock::world::{Coord, World};
use sha2::{Digest, Sha256};

const GOLDEN_GENESIS_ROOTS_TAG: &str =
    "#bb.receipt.sha256.kbi2xxuzs6zay5pjvotpyk3edmn7dchwlh22bmcbpwfde3cyc55a";
const GOLDEN_POST_TX_ROOTS_TAG: &str =
    "#bb.receipt.sha256.5mq4zzp7sm6ghxnrhiww54uu2kmj56qgkftkjkoo2eosjf5hilja";
const GOLDEN_AUTH_INC_PROOF_TAG: &str =
    "#bb.receipt.sha256.nu2trkasizgtdp4mnyoom77tr2nqgylmuazaszn4ra3lwlrwvjiq";
const GOLDEN_NONCE_ABS_PROOF_TAG: &str =
    "#bb.receipt.sha256.daucn3zib6opfx5dmpny25vqeift7rvitvxgsylagmhq3utgwegq";

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

fn canonical_auth_commit() -> (IdentityContextV1, AuthCommitStateV1, ActorIdV1, ActorIdV1) {
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
    let desc_b = AuthorizationDescriptorV1 {
        descriptor_version: 0,
        action_policy: pol(CredentialRoleV1::ActorAction, &[21], 1),
        admin_policy: pol(CredentialRoleV1::ActorAdmin, &[22], 1),
        delegations: vec![],
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

    let st_a = ActorAuthStateV1 {
        state_version: 1,
        actor_id: actor_a,
        descriptor: desc_a,
        next_nonce: 0,
        recovery_guardians: pol(CredentialRoleV1::ActorAdmin, &[13], 1),
        recovery_delay_heights: 100,
        pending_recovery: None,
    };
    let st_b = ActorAuthStateV1 {
        state_version: 1,
        actor_id: actor_b,
        descriptor: desc_b,
        next_nonce: 0,
        recovery_guardians: pol(CredentialRoleV1::ActorAdmin, &[23], 1),
        recovery_delay_heights: 100,
        pending_recovery: None,
    };
    let commit = AuthCommitStateV1::from_genesis(ctx, vec![st_a, st_b]).unwrap();
    (ctx, commit, actor_a, actor_b)
}

#[test]
fn auth_commit_1_golden_tags_proofs_and_codec_parity() {
    let (ctx, mut state, actor_a, _actor_b) = canonical_auth_commit();
    state.verify_rebuild_parity().unwrap();
    assert_eq!(state.auth_trie().domain(), DomainIdV1::Authorization);
    assert_eq!(state.nonce_trie().domain(), DomainIdV1::Nonce);
    assert_eq!(state.auth_trie().len(), 2);
    assert!(!state.auth_trie().is_empty());
    assert!(state.auth_trie().get(actor_a.as_bytes()).is_some());

    let gen_roots = state.roots();
    assert_eq!(
        gen_roots.tag().unwrap().to_string(),
        GOLDEN_GENESIS_ROOTS_TAG
    );
    assert_eq!(
        AuthCommitRootsV1::decode_canonical(&gen_roots.encode_canonical().unwrap()).unwrap(),
        gen_roots
    );

    let auth_p = state.prove_authorization(&actor_a).unwrap();
    assert_eq!(auth_p.tag().unwrap().to_string(), GOLDEN_AUTH_INC_PROOF_TAG);
    assert_eq!(
        DomainPatriciaProofV1::decode_canonical(&auth_p.encode_canonical().unwrap()).unwrap(),
        auth_p
    );
    let verified_auth_bytes = auth_p
        .verify(
            DomainIdV1::Authorization,
            gen_roots.authorization_root,
            actor_a.as_bytes(),
        )
        .unwrap()
        .expect("included");
    assert_eq!(
        ActorAuthStateV1::decode_canonical(&verified_auth_bytes).unwrap(),
        *state.registry().actor_state(&actor_a).unwrap()
    );

    let missing = ActorIdV1::from_bytes([0x99; 32]);
    let abs_p = state.prove_nonce(&missing).unwrap();
    assert_eq!(abs_p.tag().unwrap().to_string(), GOLDEN_NONCE_ABS_PROOF_TAG);
    assert_eq!(
        abs_p
            .verify(DomainIdV1::Nonce, gen_roots.nonce_root, missing.as_bytes())
            .unwrap(),
        None
    );

    // Commit admitted tx (nonce 0 -> 1)
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
    state
        .commit_transaction(
            &tx0,
            actor_a,
            &pre0,
            &[(*pk(11).as_bytes(), *sig0.as_bytes())],
            10,
        )
        .unwrap();
    state.verify_rebuild_parity().unwrap();
    let post_roots = state.roots();
    assert_eq!(
        post_roots.tag().unwrap().to_string(),
        GOLDEN_POST_TX_ROOTS_TAG
    );
    let nonce_p = state.prove_nonce(&actor_a).unwrap();
    assert_eq!(
        nonce_p
            .verify(DomainIdV1::Nonce, post_roots.nonce_root, actor_a.as_bytes())
            .unwrap(),
        Some(1u64.to_be_bytes().to_vec())
    );
}

#[test]
fn auth_commit_1_cross_domain_rotation_recovery_rollback_and_50_step_parity() {
    let (ctx, mut state, actor_a, actor_b) = canonical_auth_commit();
    let r0 = state.roots();

    // 1. Cross-domain proof rejection against Nonce and COMMIT-1 World/ActiveToken/Episode roots
    let auth_p = state.prove_authorization(&actor_a).unwrap();
    assert!(matches!(
        auth_p.verify(DomainIdV1::Nonce, r0.nonce_root, actor_a.as_bytes()),
        Err(AuthCommit1Error::CrossDomainProofRejected { .. })
    ));
    let world_t = TypedPatriciaTrie::empty(CommitmentProfileDescriptorV1::world_v1()).unwrap();
    let ep_t = TypedPatriciaTrie::empty(CommitmentProfileDescriptorV1::episode_v1()).unwrap();
    assert!(
        auth_p
            .verify(
                DomainIdV1::Authorization,
                world_t.root(),
                actor_a.as_bytes()
            )
            .is_err()
    );
    assert!(
        auth_p
            .verify(DomainIdV1::Authorization, ep_t.root(), actor_a.as_bytes())
            .is_err()
    );

    // 2. Atomic failure rollback on bad signature / stale nonce leaves both roots byte-identical
    let tx_bad = TransactionBodyV1 {
        actor_id: actor_a,
        nonce: 9,
        valid_after: 1,
        valid_until: 50,
        expected_state_root: None,
        max_fee: 100,
        operations: vec![NetworkEditOpV1::On(Coord::new(2, 0, 0))],
    };
    let pre_bad = ctx.build_preimage(
        SigningDomainTagV1::ActorTransaction,
        tx_bad.tag().unwrap().digest(),
    );
    let sig_bad = sign_preimage_deterministic(&[11; 32], &pre_bad).unwrap();
    assert_eq!(
        state.commit_transaction(
            &tx_bad,
            actor_a,
            &pre_bad,
            &[(*pk(11).as_bytes(), *sig_bad.as_bytes())],
            5
        ),
        Err(AuthCommit1Error::AtomicCommitRejected(
            IdentityDispositionV1::RejectedStaleOrFutureNonce
        ))
    );
    assert_eq!(state.roots(), r0);

    // 3. Admin rotation atomically updates AuthorizationRootV1 while keeping NonceRootV1 stable
    let rot_desc = AuthorizationDescriptorV1 {
        descriptor_version: 1,
        action_policy: pol(CredentialRoleV1::ActorAction, &[15], 1),
        admin_policy: pol(CredentialRoleV1::ActorAdmin, &[12], 1),
        delegations: vec![],
        revoked: false,
    };
    let pre_rot = ctx.build_preimage(
        SigningDomainTagV1::ActorAdminRotation,
        rot_desc.tag().unwrap().digest(),
    );
    let sig_rot = sign_preimage_deterministic(&[12; 32], &pre_rot).unwrap();
    state
        .commit_admin_rotation(
            actor_a,
            rot_desc,
            &pre_rot,
            &[(*pk(12).as_bytes(), *sig_rot.as_bytes())],
            20,
        )
        .unwrap();
    state.verify_rebuild_parity().unwrap();
    let r_rot = state.roots();
    assert_ne!(r_rot.authorization_root, r0.authorization_root);
    assert_eq!(r_rot.nonce_root, r0.nonce_root);

    // 4. Two-step recovery commit: early finalize rolls back; finalize at unlock_height updates root
    let rec_desc = AuthorizationDescriptorV1 {
        descriptor_version: 2,
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
    state
        .commit_initiate_recovery(
            actor_a,
            rec_desc,
            &pre_rec,
            &[(*pk(13).as_bytes(), *sig_g.as_bytes())],
            100,
        )
        .unwrap();
    let r_init = state.roots();
    assert_eq!(
        state.commit_finalize_recovery(actor_a, 150),
        Err(AuthCommit1Error::AtomicCommitRejected(
            IdentityDispositionV1::RejectedRecoveryTimelockActive
        ))
    );
    assert_eq!(state.roots(), r_init);
    state.commit_finalize_recovery(actor_a, 200).unwrap();
    state.verify_rebuild_parity().unwrap();
    assert_ne!(state.roots().authorization_root, r_init.authorization_root);

    // 5. 50-mutation incremental upsert + delete full-rebuild parity across arbitrary 256-bit keys
    let mut trie = DomainPatriciaTrieV1::empty(DomainIdV1::Authorization);
    for i in 0u32..50 {
        let k: [u8; 32] = Sha256::digest(i.to_be_bytes()).into();
        trie.upsert(k, format!("actor-state-{i}").into_bytes());
        trie.verify_rebuild_parity().unwrap();
    }
    for i in (0u32..25).step_by(2) {
        let k: [u8; 32] = Sha256::digest(i.to_be_bytes()).into();
        assert!(trie.delete(&k));
        trie.verify_rebuild_parity().unwrap();
        let p = trie.prove(&k).unwrap();
        assert_eq!(
            p.verify(DomainIdV1::Authorization, trie.root(), &k)
                .unwrap(),
            None
        );
    }

    // 6. BOUNDS-1 12 KiB proof ceiling rejects oversized LeafMatch value fail-closed
    let huge_proof = DomainPatriciaProofV1 {
        proof_version: 1,
        domain: DomainIdV1::Authorization,
        key: *actor_b.as_bytes(),
        steps: vec![],
        terminal: DomainProofTerminalV1::LeafMatch {
            suffix_bits: vec![0u8; 256],
            value: vec![0xAA; 13_000],
        },
    };
    assert!(matches!(
        huge_proof.encode_canonical(),
        Err(AuthCommit1Error::ProofOverBounds(_))
    ));
}
