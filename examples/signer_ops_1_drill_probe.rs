use std::fs;
use std::net::TcpListener;
use std::thread;
use std::time::Instant;

use bitblock::consensus_1::ConsensusClusterHarnessV1;
use bitblock::crypto_1::KeyLifecycleRegistryV1;
use bitblock::crypto_d1::SigningDomainTagV1;
use bitblock::identity_d1::{ActorIdV1, CredentialRoleV1};
use bitblock::signer_ops_1::signer_io::{
    PersistentValidatorSignerV1, RemoteSignRequestV1, call_remote_signer,
    serve_one_remote_sign_request,
};
use bitblock::signer_ops_1::{
    SignerPhaseCodeV1, SignerWatermarkV1, ValidatorKeyCeremonyBundleV1, ValidatorSignerCoreV1,
};

fn p50_us(samples: &mut [f64]) -> f64 {
    samples.sort_by(|a, b| a.total_cmp(b));
    samples[samples.len() / 2]
}

fn main() {
    let cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().unwrap();
    let fork = cluster.fixture.fork_context;
    let vid = ActorIdV1::from_bytes([1u8; 32]);
    let prop_seed = ConsensusClusterHarnessV1::validator_proposal_seed(1);
    let vote_seed = ConsensusClusterHarnessV1::validator_vote_seed(1);
    let mut reg = KeyLifecycleRegistryV1::default();
    let ceremony = ValidatorKeyCeremonyBundleV1::execute(
        &fork,
        vid,
        &prop_seed,
        &vote_seed,
        b"bitblock-signer-ops-v1-ceremony",
        [0xA1; 16],
        [0xB1; 16],
        [0xA2; 16],
        [0xB2; 16],
        1_000,
        &mut reg,
    )
    .unwrap();

    // Compute exact golden tags
    let mut golden_core =
        ValidatorSignerCoreV1::from_ceremony(fork, &ceremony, prop_seed, vote_seed).unwrap();
    let (_, golden_rcpt) = golden_core
        .authorize_and_sign_digest(
            CredentialRoleV1::ValidatorVote,
            SigningDomainTagV1::ConsensusVote,
            1,
            SignerWatermarkV1 {
                epoch: 0,
                height: 1,
                round: 0,
                phase: SignerPhaseCodeV1::PrepareVote,
            },
            [0x44; 32],
            [0x55; 32],
            &reg,
        )
        .unwrap();
    let policy_tag = ceremony.policy.tag().unwrap().to_string();
    let audit_tag = golden_rcpt.tag().unwrap().to_string();

    let dir = std::env::temp_dir().join(format!("bitblock_sops_probe_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let mut persistent = PersistentValidatorSignerV1::open_or_create(
        dir.clone(),
        fork,
        &ceremony,
        prop_seed,
        vote_seed,
    )
    .unwrap();

    // Benchmark 1: atomic fsync+rename disk watermark sign
    let mut disk_samples = Vec::with_capacity(15);
    for h in 1..=15u64 {
        let wm = SignerWatermarkV1 {
            epoch: 0,
            height: h,
            round: 0,
            phase: SignerPhaseCodeV1::PrepareVote,
        };
        let t0 = Instant::now();
        let _ = persistent
            .sign_digest_durable(
                CredentialRoleV1::ValidatorVote,
                SigningDomainTagV1::ConsensusVote,
                1,
                wm,
                [h as u8; 32],
                [0xAA; 32],
                &reg,
            )
            .unwrap();
        disk_samples.push(t0.elapsed().as_secs_f64() * 1e6);
    }

    // Benchmark 2: anti-equivocation refusal in core
    let mut refuse_samples = Vec::with_capacity(31);
    let wm1 = SignerWatermarkV1 {
        epoch: 0,
        height: 1,
        round: 0,
        phase: SignerPhaseCodeV1::PrepareVote,
    };
    for _ in 0..31 {
        let t0 = Instant::now();
        let _ = golden_core.authorize_and_sign_digest(
            CredentialRoleV1::ValidatorVote,
            SigningDomainTagV1::ConsensusVote,
            1,
            wm1,
            [0x99; 32],
            [0x56; 32],
            &reg,
        );
        refuse_samples.push(t0.elapsed().as_secs_f64() * 1e6);
    }

    // Benchmark 3: 127.0.0.1 loopback-TCP remote-signer RPC roundtrip (BBSIG1)
    let mut tcp_samples = Vec::with_capacity(11);
    for h in 20..31u64 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let req = RemoteSignRequestV1 {
            role_code: CredentialRoleV1::ValidatorVote.code(),
            domain_code: SigningDomainTagV1::ConsensusVote.code(),
            failover_epoch: 1,
            watermark: SignerWatermarkV1 {
                epoch: 0,
                height: h,
                round: 0,
                phase: SignerPhaseCodeV1::PrepareVote,
            },
            block_id: [h as u8; 32],
            unsigned_payload_digest: [0xBB; 32],
        };
        let reg_c = reg.clone();
        let t0 = Instant::now();
        let srv = thread::spawn(move || {
            let _ = serve_one_remote_sign_request(&listener, &mut persistent, &reg_c).unwrap();
            persistent
        });
        let (sig, _) = call_remote_signer(addr, &req).unwrap();
        assert!(sig.is_some());
        persistent = srv.join().unwrap();
        tcp_samples.push(t0.elapsed().as_secs_f64() * 1e6);
    }

    drop(persistent);
    let _ = fs::remove_dir_all(&dir);

    println!("SOPS_DISK_US={:.2}", p50_us(&mut disk_samples));
    println!("SOPS_TCP_US={:.2}", p50_us(&mut tcp_samples));
    println!("SOPS_REFUSE_US={:.2}", p50_us(&mut refuse_samples));
    println!("SOPS_POLICY_TAG={policy_tag}");
    println!("SOPS_AUDIT_TAG={audit_tag}");
}
