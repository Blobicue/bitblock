//! Release lifecycle, custody-tamper, and 2,048-signature batch-blame probe for `CRYPTO-1` (`PS-070`).

use std::time::Instant;

use bitblock::crypto_1::{
    CustodyBackupEnvelopeV1, DEFAULT_CUSTODY_KDF_ITERATIONS, KeyLifecycleRegistryV1,
    MAX_CRYPTO_1_BATCH_SIGNATURES, SuiteMigrationPolicyV1, public_key_from_seed,
};
use bitblock::crypto_d1::{
    HashSuiteIdV1, SignatureSuiteIdV1, SigningDomainTagV1, SigningPreimageV1,
    sign_preimage_deterministic,
};

fn main() {
    let seed_a = [0x11u8; 32];
    let seed_b = [0x22u8; 32];
    let pk_a = public_key_from_seed(&seed_a).unwrap();
    let pk_b = public_key_from_seed(&seed_b).unwrap();

    let mut reg = KeyLifecycleRegistryV1::new();
    let _ = reg
        .register_active_key(pk_a, SigningDomainTagV1::ActorTransaction, 10)
        .unwrap();
    let (rot_rec, _) = reg.rotate_key(&pk_a, pk_b, 50).unwrap();

    let env = CustodyBackupEnvelopeV1::seal(
        &seed_a,
        SigningDomainTagV1::ActorTransaction,
        b"bitblock-devnet-passphrase-v1",
        [0xA1; 16],
        [0xB2; 16],
        DEFAULT_CUSTODY_KDF_ITERATIONS,
    )
    .unwrap();
    let mig = SuiteMigrationPolicyV1::v1_pq_upgrade_ready(1_000, 3_048);

    // 9x9 cross-domain rejection sweep
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
    let mut cross_reg = KeyLifecycleRegistryV1::new();
    let mut domain_keys = Vec::new();
    for (i, &dom) in all_domains.iter().enumerate() {
        let mut s = [0u8; 32];
        s[0] = (i as u8) + 1;
        s[31] = 0xC1;
        let pk = public_key_from_seed(&s).unwrap();
        cross_reg.register_active_key(pk, dom, 0).unwrap();
        domain_keys.push((s, pk, dom));
    }
    let mut cross_role_accepted = 0u32;
    let mut cross_role_rejected = 0u32;
    for (s, pk, bound_dom) in &domain_keys {
        for &try_dom in &all_domains {
            let pre = SigningPreimageV1 {
                protocol_version: 1,
                chain_id: 7,
                genesis_id: [0x33; 32],
                launch_manifest_id: [0x44; 32],
                profile_id: [0x55; 32],
                domain_tag: try_dom,
                signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
                hash_suite: HashSuiteIdV1::Sha256V1,
                payload_digest: [0x77; 32],
            };
            let sig = sign_preimage_deterministic(s, &pre).unwrap();
            if cross_reg
                .verify_authorized_signature(pk, &pre, &sig, 5)
                .is_ok()
            {
                assert_eq!(*bound_dom, try_dom);
                cross_role_accepted += 1;
            } else {
                assert_ne!(*bound_dom, try_dom);
                cross_role_rejected += 1;
            }
        }
    }

    // Custody 1-bit tamper sweep across all 136 envelope bytes
    let raw_env = env.encode_canonical().unwrap();
    let mut tamper_caught = 0usize;
    for byte_idx in 0..raw_env.len() {
        let mut mutated = raw_env;
        mutated[byte_idx] ^= 0x01;
        let rejected = match CustodyBackupEnvelopeV1::decode_canonical(&mutated) {
            Err(_) => true,
            Ok(decoded) => decoded
                .restore_seed(b"bitblock-devnet-passphrase-v1")
                .is_err(),
        };
        if rejected {
            tamper_caught += 1;
        }
    }

    // Batch verification scaling (64, 512, 2048 sigs) + single-fault blame isolation at index 1337
    let mut batch_reg = KeyLifecycleRegistryV1::new();
    let batch_seed = [0x42u8; 32];
    let batch_pk = public_key_from_seed(&batch_seed).unwrap();
    batch_reg
        .register_active_key(batch_pk, SigningDomainTagV1::ActorTransaction, 0)
        .unwrap();
    let mut items_2048 = Vec::with_capacity(MAX_CRYPTO_1_BATCH_SIGNATURES);
    for i in 0..MAX_CRYPTO_1_BATCH_SIGNATURES {
        let mut pd = [0u8; 32];
        pd[0..4].copy_from_slice(&(i as u32).to_be_bytes());
        let pre = SigningPreimageV1 {
            protocol_version: 1,
            chain_id: 7,
            genesis_id: [0x33; 32],
            launch_manifest_id: [0x44; 32],
            profile_id: [0x55; 32],
            domain_tag: SigningDomainTagV1::ActorTransaction,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            payload_digest: pd,
        };
        let sig = sign_preimage_deterministic(&batch_seed, &pre).unwrap();
        items_2048.push((batch_pk, pre, sig));
    }

    let measure_ms = |slice: &[_], runs: usize| -> (f64, f64) {
        let mut samples = Vec::with_capacity(runs);
        for _ in 0..runs {
            let t0 = Instant::now();
            batch_reg
                .verify_batch_authorized(slice, 10, MAX_CRYPTO_1_BATCH_SIGNATURES)
                .unwrap();
            samples.push(t0.elapsed().as_secs_f64() * 1e3);
        }
        samples.sort_by(|a, b| a.total_cmp(b));
        (samples[samples.len() / 2], *samples.last().unwrap())
    };
    let (ms64_p50, _) = measure_ms(&items_2048[..64], 15);
    let (ms512_p50, _) = measure_ms(&items_2048[..512], 9);
    let (ms2048_p50, ms2048_max) = measure_ms(&items_2048, 7);

    // Fault injection at index 1337
    let mut faulty = items_2048.clone();
    faulty[1337].1.payload_digest[31] ^= 0xFF;
    let t_fault = Instant::now();
    let blame_err = batch_reg
        .verify_batch_authorized(&faulty, 10, MAX_CRYPTO_1_BATCH_SIGNATURES)
        .unwrap_err();
    let (blame_index, blame_reason) = match blame_err {
        bitblock::crypto_1::Crypto1Error::BatchItemFailed {
            first_invalid_index,
            reason,
        } => (first_invalid_index, reason),
        other => panic!("unexpected blame error: {other:?}"),
    };
    let blame_ms = t_fault.elapsed().as_secs_f64() * 1e3;

    // Custody seal + restore latency
    let t_cust = Instant::now();
    let _ = env.restore_seed(b"bitblock-devnet-passphrase-v1").unwrap();
    let custody_restore_ms = t_cust.elapsed().as_secs_f64() * 1e3;

    println!("{{");
    println!("  \"rot_record_tag\": \"{}\",", rot_rec.tag().unwrap());
    println!("  \"custody_env_tag\": \"{}\",", env.tag().unwrap());
    println!("  \"migration_policy_tag\": \"{}\",", mig.tag().unwrap());
    println!("  \"cross_role_accepted\": {},", cross_role_accepted);
    println!("  \"cross_role_rejected\": {},", cross_role_rejected);
    println!("  \"custody_tamper_caught\": {},", tamper_caught);
    println!("  \"custody_tamper_total\": {},", raw_env.len());
    println!("  \"batch_64_p50_ms\": {:.3},", ms64_p50);
    println!("  \"batch_512_p50_ms\": {:.3},", ms512_p50);
    println!("  \"batch_2048_p50_ms\": {:.3},", ms2048_p50);
    println!("  \"batch_2048_max_ms\": {:.3},", ms2048_max);
    println!("  \"blame_1337_ms\": {:.3},", blame_ms);
    println!("  \"blame_index\": {},", blame_index);
    println!("  \"blame_reason\": \"{}\",", blame_reason);
    println!("  \"custody_restore_ms\": {:.3}", custody_restore_ms);
    println!("}}");
}
