#![cfg(feature = "protocol-research")]

use bitblock::crypto_1::{
    Crypto1Error, CustodyBackupEnvelopeV1, DEFAULT_CUSTODY_KDF_ITERATIONS, KeyLifecycleRecordV1,
    KeyLifecycleRegistryV1, KeyStatusV1, MAX_CRYPTO_1_BATCH_SIGNATURES, SuiteMigrationPolicyV1,
    canonical_role_for_domain, public_key_from_seed,
};
use bitblock::crypto_d1::{
    HashSuiteIdV1, SignatureSuiteIdV1, SigningDomainTagV1, SigningPreimageV1,
    sign_preimage_deterministic,
};

const GOLDEN_ROT_RECORD_TAG: &str =
    "#bb.receipt.sha256.3dyl5u2pyn7r4svpu5p7sxnny7qgv3hqui3edgasuwfiiwjzvkcq";
const GOLDEN_CUSTODY_ENV_TAG: &str =
    "#bb.receipt.sha256.lc3kdko2lsupgbqwysbcsoyadi3z62p4pde3ch5ioldg3hqriumq";
const GOLDEN_MIGRATION_POLICY_TAG: &str =
    "#bb.policy.sha256.3r2uumuzxwfdhct2e6xniiatblaj555mjjgxpy4dtdy4tvmup6oq";

fn sample_preimage(domain_tag: SigningDomainTagV1, salt: u8) -> SigningPreimageV1 {
    SigningPreimageV1 {
        protocol_version: 1,
        chain_id: 7,
        genesis_id: [0x33; 32],
        launch_manifest_id: [0x44; 32],
        profile_id: [0x55; 32],
        domain_tag,
        signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
        hash_suite: HashSuiteIdV1::Sha256V1,
        payload_digest: [salt; 32],
    }
}

#[test]
fn wave10_crypto_1_golden_python_parity_and_custody_tamper_sweep() {
    let seed_a = [0x11u8; 32];
    let seed_b = [0x22u8; 32];
    let pk_a = public_key_from_seed(&seed_a).unwrap();
    let pk_b = public_key_from_seed(&seed_b).unwrap();

    let mut reg = KeyLifecycleRegistryV1::new();
    let _ = reg
        .register_active_key(pk_a, SigningDomainTagV1::ActorTransaction, 10)
        .unwrap();
    let (rot_rec, _) = reg.rotate_key(&pk_a, pk_b, 50).unwrap();
    assert_eq!(rot_rec.tag().unwrap().to_string(), GOLDEN_ROT_RECORD_TAG);
    assert_eq!(
        KeyLifecycleRecordV1::decode_canonical(&rot_rec.encode_canonical().unwrap()).unwrap(),
        rot_rec
    );
    assert!(KeyLifecycleRecordV1::decode_canonical(&[0u8; 93]).is_err());

    let env = CustodyBackupEnvelopeV1::seal(
        &seed_a,
        SigningDomainTagV1::ActorTransaction,
        b"bitblock-devnet-passphrase-v1",
        [0xA1; 16],
        [0xB2; 16],
        DEFAULT_CUSTODY_KDF_ITERATIONS,
    )
    .unwrap();
    assert_eq!(env.tag().unwrap().to_string(), GOLDEN_CUSTODY_ENV_TAG);
    assert_eq!(
        env.restore_seed(b"bitblock-devnet-passphrase-v1").unwrap(),
        seed_a
    );
    assert_eq!(
        env.restore_seed(b"wrong-passphrase"),
        Err(Crypto1Error::CustodyMacMismatch)
    );
    assert_eq!(
        env.restore_seed(b""),
        Err(Crypto1Error::InvalidCustodyParameters(
            "passphrase must be non-empty"
        ))
    );

    // Exhaustive 1-bit tamper check across every single byte of the 136-byte custody envelope.
    let raw_env = env.encode_canonical().unwrap();
    assert_eq!(raw_env.len(), CustodyBackupEnvelopeV1::WIRE_LEN);
    for idx in 0..raw_env.len() {
        let mut bad = raw_env;
        bad[idx] ^= 0x01;
        let ok = CustodyBackupEnvelopeV1::decode_canonical(&bad)
            .and_then(|e| e.restore_seed(b"bitblock-devnet-passphrase-v1"));
        assert!(ok.is_err(), "1-bit tamper at byte {idx} must fail closed");
    }

    let mig = SuiteMigrationPolicyV1::v1_pq_upgrade_ready(1_000, 3_048);
    assert_eq!(mig.tag().unwrap().to_string(), GOLDEN_MIGRATION_POLICY_TAG);
    assert_eq!(
        SuiteMigrationPolicyV1::decode_canonical(&mig.encode_canonical().unwrap()).unwrap(),
        mig
    );
    assert!(SuiteMigrationPolicyV1::decode_canonical(&[0u8; 35]).is_err());
}

#[test]
fn wave10_crypto_1_lifecycle_cross_role_batch_blame_and_anti_downgrade() {
    let all_domains = [
        SigningDomainTagV1::ActorTransaction,
        SigningDomainTagV1::ActorAdminRotation,
        SigningDomainTagV1::BlockProposal,
        SigningDomainTagV1::ConsensusVote,
        SigningDomainTagV1::ValidatorTransport,
        SigningDomainTagV1::RewardWithdrawal,
        SigningDomainTagV1::GovernanceAction,
        SigningDomainTagV1::EmergencyPause,
        SigningDomainTagV1::ReleaseArtifact,
    ];

    // 1. 9x9 strict cross-domain separation & duplicate/cross-role registration refusal.
    let mut reg = KeyLifecycleRegistryV1::new();
    let mut keys = Vec::new();
    for (i, &dom) in all_domains.iter().enumerate() {
        let mut s = [0u8; 32];
        s[0] = (i as u8) + 1;
        s[31] = 0xD9;
        let pk = public_key_from_seed(&s).unwrap();
        let rec = reg.register_active_key(pk, dom, 10).unwrap();
        assert_eq!(rec.role, canonical_role_for_domain(dom));
        // Re-registering same key under any other domain fails closed.
        let other_dom = all_domains[(i + 1) % all_domains.len()];
        assert_eq!(
            reg.register_active_key(pk, other_dom, 10),
            Err(Crypto1Error::CrossRoleOrDuplicateKeyReuse)
        );
        keys.push((s, pk, dom));
    }

    for (s, pk, bound) in &keys {
        for &attempt in &all_domains {
            let pre = sample_preimage(attempt, 0x5A);
            let sig = sign_preimage_deterministic(s, &pre).unwrap();
            let res = reg.verify_authorized_signature(pk, &pre, &sig, 20);
            if *bound == attempt {
                assert!(res.is_ok());
            } else {
                assert_eq!(
                    res,
                    Err(Crypto1Error::CrossDomainSignatureRejected {
                        bound: *bound,
                        attempted: attempt,
                    })
                );
            }
        }
    }

    // 2. Irreversible revocation & compromise quarantine.
    let (_, pk0, dom0) = keys[0];
    let (_, pk1, _) = keys[1];
    let s_fresh = [0x99u8; 32];
    let pk_fresh = public_key_from_seed(&s_fresh).unwrap();
    let _ = reg.rotate_key(&pk0, pk_fresh, 100).unwrap();
    // Cannot reuse an already-registered key as successor.
    assert_eq!(
        reg.rotate_key(&pk_fresh, pk1, 120),
        Err(Crypto1Error::CrossRoleOrDuplicateKeyReuse)
    );
    let comp = reg.report_compromise(&pk_fresh, 120, 500).unwrap();
    assert_eq!(comp.status, KeyStatusV1::Compromised);
    assert_eq!(comp.quarantine_until_height, 620);
    // Compromised key cannot sign, rotate, revoke, or re-compromise.
    let pre_c = sample_preimage(dom0, 0xCC);
    let sig_c = sign_preimage_deterministic(&s_fresh, &pre_c).unwrap();
    assert_eq!(
        reg.verify_authorized_signature(&pk_fresh, &pre_c, &sig_c, 120),
        Err(Crypto1Error::KeyNotAuthoritative(KeyStatusV1::Compromised))
    );
    assert!(reg.revoke_key(&pk_fresh, 125).is_err());
    assert!(reg.rotate_key(&pk_fresh, pk0, 125).is_err());

    let _ = reg.revoke_key(&pk1, 130).unwrap();
    assert!(reg.revoke_key(&pk1, 135).is_err());

    // 3. Batch verification with exact blame index + 2,049 over-bound preflight refusal.
    let (sb, pkb, domb) = keys[2];
    let mut batch = Vec::new();
    for i in 0..32u8 {
        let pre = sample_preimage(domb, i);
        let sig = sign_preimage_deterministic(&sb, &pre).unwrap();
        batch.push((pkb, pre, sig));
    }
    assert!(reg.verify_batch_authorized(&batch, 20, 64).is_ok());

    let mut bad_batch = batch.clone();
    bad_batch[19].1.payload_digest[0] ^= 0x01;
    assert_eq!(
        reg.verify_batch_authorized(&bad_batch, 20, 64),
        Err(Crypto1Error::BatchItemFailed {
            first_invalid_index: 19,
            reason: "ed25519_verify_failed",
        })
    );
    assert_eq!(
        reg.verify_batch_authorized(&batch, 20, 16),
        Err(Crypto1Error::BatchOverLimit {
            observed: 32,
            limit: 16,
        })
    );
    assert_eq!(MAX_CRYPTO_1_BATCH_SIGNATURES, 2_048);

    // 4. Anti-downgrade SuiteMigrationPolicyV1 negative controls.
    let mut downgrade = SuiteMigrationPolicyV1::v1_pq_upgrade_ready(1_000, 3_048);
    downgrade.old_sig_suite = 2;
    downgrade.new_sig_suite = 1;
    assert!(downgrade.validate().is_err());

    let mut bad_fallback = SuiteMigrationPolicyV1::v1_pq_upgrade_ready(1_000, 3_048);
    bad_fallback.allow_legacy_fallback_after_sunset = true;
    assert!(bad_fallback.validate().is_err());

    let mut bad_overlap = SuiteMigrationPolicyV1::v1_pq_upgrade_ready(1_000, 1_500);
    assert!(bad_overlap.validate().is_err());
    bad_overlap.sunset_height = 2_024;
    assert!(bad_overlap.validate().is_ok());
    assert!(bad_overlap.check_suite_at_height(2, 1, 999).is_err());
    assert!(bad_overlap.check_suite_at_height(1, 1, 1_500).is_ok());
    assert!(bad_overlap.check_suite_at_height(2, 1, 1_500).is_ok());
    assert!(bad_overlap.check_suite_at_height(1, 1, 2_024).is_err());
    assert!(bad_overlap.check_suite_at_height(2, 1, 2_024).is_ok());
}
