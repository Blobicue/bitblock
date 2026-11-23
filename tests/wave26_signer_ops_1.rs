#![cfg(feature = "protocol-research")]

use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;

use bitblock::consensus_1::{ConsensusClusterHarnessV1, ConsensusVotePhaseV1, ConsensusVoteV1};
use bitblock::crypto_1::KeyLifecycleRegistryV1;
use bitblock::crypto_d1::SigningDomainTagV1;
use bitblock::identity_d1::{ActorIdV1, CredentialRoleV1};
use bitblock::signer_ops_1::signer_io::{
    PersistentValidatorSignerV1, RemoteSignRequestV1, call_remote_signer,
    serve_one_remote_sign_request,
};
use bitblock::signer_ops_1::{
    SignerDecisionV1, SignerOps1Error, SignerPhaseCodeV1, SignerPolicyManifestV1,
    SignerWatermarkV1, ValidatorKeyCeremonyBundleV1, ValidatorSignerCoreV1, verify_audit_chain,
};
use sha2::{Digest, Sha256};

fn fresh_temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bitblock_wave26_{label}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn wave26_ceremony_atomic_disk_and_loopback_tcp_remote_signer() {
    let mut cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().unwrap();
    let pre_wr = cluster.nodes[0].harness().snapshot().unwrap().world_root;
    let bundle = cluster
        .fixture
        .canonical_mixed_race_bundle(1, pre_wr)
        .unwrap();
    let cert = cluster
        .run_round_with_subset(&bundle, 0, &[0, 1, 2, 3])
        .unwrap();
    let sched0 = cluster.nodes[0].schedule_at(0).unwrap().clone();
    let vid = ActorIdV1::from_bytes([1u8; 32]);
    let prop_seed = ConsensusClusterHarnessV1::validator_proposal_seed(1);
    let vote_seed = ConsensusClusterHarnessV1::validator_vote_seed(1);

    let mut reg = KeyLifecycleRegistryV1::default();
    let ceremony = ValidatorKeyCeremonyBundleV1::execute(
        &cluster.fixture.fork_context,
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

    let dir = fresh_temp_dir("tcp_remote");
    let mut persistent = PersistentValidatorSignerV1::open_or_create(
        dir.clone(),
        cluster.fixture.fork_context,
        &ceremony,
        prop_seed,
        vote_seed,
    )
    .unwrap();

    // Build an unsigned CONSENSUS-1 Prepare vote and sign it via real 127.0.0.1 loopback TCP!
    let mut vote = ConsensusVoteV1 {
        vote_version: 1,
        height: 2,
        epoch: 0,
        view: 0,
        phase: ConsensusVotePhaseV1::Prepare,
        block_id: [0x77; 32],
        schedule_epoch_digest: sched0.tag().unwrap().digest(),
        execution_root_ref_digest: Sha256::digest(
            cert.proposal.execution_root_ref.encode_canonical(),
        )
        .into(),
        has_full_body: true,
        voter_id: vid,
        vote_pubkey: [0u8; 32],
        signature: [0u8; 64],
    };
    let req = RemoteSignRequestV1 {
        role_code: CredentialRoleV1::ValidatorVote.code(),
        domain_code: SigningDomainTagV1::ConsensusVote.code(),
        failover_epoch: 1,
        watermark: SignerWatermarkV1 {
            epoch: 0,
            height: 2,
            round: 0,
            phase: SignerPhaseCodeV1::PrepareVote,
        },
        block_id: vote.block_id,
        unsigned_payload_digest: vote.unsigned_payload_digest(),
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let reg_clone = reg.clone();
    let srv = thread::spawn(move || {
        let rcpt = serve_one_remote_sign_request(&listener, &mut persistent, &reg_clone).unwrap();
        (persistent, rcpt)
    });
    let (maybe_sig, client_rcpt) = call_remote_signer(addr, &req).unwrap();
    let (persistent, server_rcpt) = srv.join().unwrap();

    let sig = maybe_sig.expect("remote signer approved PrepareVote");
    assert_eq!(client_rcpt, server_rcpt);
    assert_eq!(client_rcpt.decision, SignerDecisionV1::SignedApproved);
    verify_audit_chain(persistent.core().audit_log()).unwrap();

    // Verify that the remote-signed vote passes strict CONSENSUS-1 verification!
    vote.vote_pubkey = *ceremony.policy.vote_key.as_bytes();
    vote.signature = *sig.as_bytes();
    let weight = vote
        .verify_against_schedule(&cluster.fixture.fork_context, &sched0)
        .unwrap();
    assert_eq!(weight, 100_000_000_000);

    // Exercise sign_vote_durable (height 2 CommitVote) and sign_consensus_proposal (height 3 Proposal)
    let mut persistent = persistent;
    let mut commit_vote = vote;
    commit_vote.phase = ConsensusVotePhaseV1::Commit;
    let (signed_commit_vote, commit_rcpt) =
        persistent.sign_vote_durable(commit_vote, 1, &reg).unwrap();
    assert!(commit_rcpt.signature_emitted);
    assert_eq!(
        signed_commit_vote
            .verify_against_schedule(&cluster.fixture.fork_context, &sched0)
            .unwrap(),
        100_000_000_000
    );

    let mut prop = cert.proposal;
    prop.height = 3;
    prop.proposer_id = vid;
    let mut prop_core = persistent.core().clone();
    let (signed_prop, prop_rcpt) = prop_core.sign_consensus_proposal(prop, 1, &reg).unwrap();
    assert!(prop_rcpt.signature_emitted);
    assert_eq!(
        signed_prop.proposal_pubkey,
        *ceremony.policy.proposal_key.as_bytes()
    );

    drop(persistent);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn wave26_seven_operational_drills_and_role_separation() {
    let cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().unwrap();
    let fork = cluster.fixture.fork_context;
    let vid = ActorIdV1::from_bytes([1u8; 32]);
    let prop_seed = ConsensusClusterHarnessV1::validator_proposal_seed(1);
    let vote_seed = ConsensusClusterHarnessV1::validator_vote_seed(1);
    let passphrase = b"bitblock-signer-ops-v1-ceremony";

    let mut reg = KeyLifecycleRegistryV1::default();
    let ceremony = ValidatorKeyCeremonyBundleV1::execute(
        &fork, vid, &prop_seed, &vote_seed, passphrase, [0xA1; 16], [0xB1; 16], [0xA2; 16],
        [0xB2; 16], 1_000, &mut reg,
    )
    .unwrap();

    let dir = fresh_temp_dir("seven_drills");
    let mut signer = PersistentValidatorSignerV1::open_or_create(
        dir.clone(),
        fork,
        &ceremony,
        prop_seed,
        vote_seed,
    )
    .unwrap();

    // Sign height 5 PrepareVote cleanly
    let wm5 = SignerWatermarkV1 {
        epoch: 0,
        height: 5,
        round: 0,
        phase: SignerPhaseCodeV1::PrepareVote,
    };
    let (_, rcpt1) = signer
        .sign_digest_durable(
            CredentialRoleV1::ValidatorVote,
            SigningDomainTagV1::ConsensusVote,
            1,
            wm5,
            [0x11; 32],
            [0x21; 32],
            &reg,
        )
        .unwrap();
    assert!(rcpt1.signature_emitted);

    // Drill D3: Concurrent replica OS exclusive lock MUST fail immediately while primary is open!
    assert!(matches!(
        PersistentValidatorSignerV1::open_or_create(
            dir.clone(),
            fork,
            &ceremony,
            prop_seed,
            vote_seed,
        ),
        Err(SignerOps1Error::ConcurrentReplicaLocked(_))
    ));

    // Drill D4a: Equivocation refusal (same watermark, distinct block_id -> RefusedEquivocation)
    assert!(matches!(
        signer.sign_digest_durable(
            CredentialRoleV1::ValidatorVote,
            SigningDomainTagV1::ConsensusVote,
            1,
            wm5,
            [0x99; 32],
            [0x22; 32],
            &reg,
        ),
        Err(SignerOps1Error::EquivocationRefused { .. })
    ));

    // Drill D4b: Rollback refusal (lower watermark height 4 -> RefusedRollback)
    let wm4 = SignerWatermarkV1 {
        epoch: 0,
        height: 4,
        round: 0,
        phase: SignerPhaseCodeV1::CommitVote,
    };
    assert!(matches!(
        signer.sign_digest_durable(
            CredentialRoleV1::ValidatorVote,
            SigningDomainTagV1::ConsensusVote,
            1,
            wm4,
            [0x33; 32],
            [0x23; 32],
            &reg,
        ),
        Err(SignerOps1Error::WatermarkRollbackRefused { .. })
    ));

    // Cross-role / cross-domain refusals (Governance, Release, ActorTransaction, mismatched role)
    assert!(matches!(
        signer.sign_digest_durable(
            CredentialRoleV1::ValidatorProposal,
            SigningDomainTagV1::ConsensusVote,
            1,
            SignerWatermarkV1 {
                epoch: 0,
                height: 6,
                round: 0,
                phase: SignerPhaseCodeV1::PrepareVote
            },
            [0x44; 32],
            [0x24; 32],
            &reg,
        ),
        Err(SignerOps1Error::RoleOrDomainMismatch(_))
    ));
    assert!(matches!(
        signer.sign_digest_durable(
            CredentialRoleV1::ReleaseSigner,
            SigningDomainTagV1::ReleaseArtifact,
            1,
            SignerWatermarkV1 {
                epoch: 0,
                height: 6,
                round: 0,
                phase: SignerPhaseCodeV1::PrepareVote
            },
            [0x44; 32],
            [0x24; 32],
            &reg,
        ),
        Err(SignerOps1Error::RoleOrDomainMismatch(_))
    ));
    verify_audit_chain(signer.core().audit_log()).unwrap();

    // Drill D1: Simulated crash + restart preserves monotonic height 5; corrupt watermark fails closed!
    drop(signer);
    let mut reopened = PersistentValidatorSignerV1::open_or_create(
        dir.clone(),
        fork,
        &ceremony,
        prop_seed,
        vote_seed,
    )
    .unwrap();
    assert_eq!(reopened.core().persisted_state().last_watermark, wm5);
    assert!(matches!(
        reopened.sign_digest_durable(
            CredentialRoleV1::ValidatorVote,
            SigningDomainTagV1::ConsensusVote,
            1,
            wm5,
            [0xEE; 32],
            [0x25; 32],
            &reg,
        ),
        Err(SignerOps1Error::EquivocationRefused { .. })
    ));
    drop(reopened);

    // Corrupt watermark file -> fail-closed startup refusal
    let bad_dir = fresh_temp_dir("corrupt_wm");
    let s_tmp = PersistentValidatorSignerV1::open_or_create(
        bad_dir.clone(),
        fork,
        &ceremony,
        prop_seed,
        vote_seed,
    )
    .unwrap();
    drop(s_tmp);
    let wm_file = bad_dir.join("watermark.bin");
    let mut raw = fs::read(&wm_file).unwrap();
    raw[10] ^= 0xFF;
    fs::write(&wm_file, &raw).unwrap();
    assert!(matches!(
        PersistentValidatorSignerV1::open_or_create(
            bad_dir.clone(),
            fork,
            &ceremony,
            prop_seed,
            vote_seed,
        ),
        Err(SignerOps1Error::CorruptWatermarkState(_))
    ));
    let _ = fs::remove_dir_all(&bad_dir);

    // Drill D2: Stale backup restore refused unless failover_epoch AND safe floor strictly advance!
    assert!(matches!(
        ValidatorSignerCoreV1::restore_from_encrypted_backup(
            fork,
            &ceremony,
            passphrase,
            SignerWatermarkV1::ZERO,
            wm5,
            1,
            2,
            wm5
        ),
        Err(SignerOps1Error::StaleBackupFloorRefused { .. })
    ));
    assert!(matches!(
        ValidatorSignerCoreV1::restore_from_encrypted_backup(
            fork,
            &ceremony,
            passphrase,
            SignerWatermarkV1::ZERO,
            wm5,
            1,
            1,
            SignerWatermarkV1 {
                epoch: 0,
                height: 6,
                round: 0,
                phase: SignerPhaseCodeV1::Proposal
            },
        ),
        Err(SignerOps1Error::StaleFailoverFence { .. })
    ));

    // Drill D5 & D7: Standby failover + Disaster Recovery on clean host from encrypted backup
    let wm6_floor = SignerWatermarkV1 {
        epoch: 0,
        height: 6,
        round: 0,
        phase: SignerPhaseCodeV1::Proposal,
    };
    let dr_core = ValidatorSignerCoreV1::restore_from_encrypted_backup(
        fork,
        &ceremony,
        passphrase,
        SignerWatermarkV1::ZERO,
        wm5,
        1,
        2,
        wm6_floor,
    )
    .unwrap();
    let dr_dir = fresh_temp_dir("disaster_recovery");
    let mut standby = PersistentValidatorSignerV1::open_from_core(dr_dir.clone(), dr_core).unwrap();
    // Stale failover_epoch=1 is rejected on standby (failover_epoch=2)!
    assert!(matches!(
        standby.sign_digest_durable(
            CredentialRoleV1::ValidatorVote,
            SigningDomainTagV1::ConsensusVote,
            1,
            SignerWatermarkV1 {
                epoch: 0,
                height: 6,
                round: 0,
                phase: SignerPhaseCodeV1::PrepareVote
            },
            [0x66; 32],
            [0x26; 32],
            &reg,
        ),
        Err(SignerOps1Error::StaleFailoverFence { .. })
    ));
    // Valid failover_epoch=2 at height 6 succeeds!
    let (_, dr_rcpt) = standby
        .sign_digest_durable(
            CredentialRoleV1::ValidatorVote,
            SigningDomainTagV1::ConsensusVote,
            2,
            SignerWatermarkV1 {
                epoch: 0,
                height: 6,
                round: 0,
                phase: SignerPhaseCodeV1::PrepareVote,
            },
            [0x66; 32],
            [0x26; 32],
            &reg,
        )
        .unwrap();
    assert!(dr_rcpt.signature_emitted);

    // Drill D6: CRYPTO-1 key compromise immediately halts signing until clean rotation!
    reg.report_compromise(&ceremony.policy.vote_key, 6, 100)
        .unwrap();
    assert!(matches!(
        standby.sign_digest_durable(
            CredentialRoleV1::ValidatorVote,
            SigningDomainTagV1::ConsensusVote,
            2,
            SignerWatermarkV1 {
                epoch: 0,
                height: 7,
                round: 0,
                phase: SignerPhaseCodeV1::PrepareVote
            },
            [0x77; 32],
            [0x27; 32],
            &reg,
        ),
        Err(SignerOps1Error::KeyRevokedOrCompromised(_))
    ));
    let mut rotated_core = standby.core().clone();
    let new_vote_pk = rotated_core
        .install_rotated_key(
            SigningDomainTagV1::ConsensusVote,
            [199u8; 32],
            3,
            &mut reg,
            7,
        )
        .unwrap();
    assert_ne!(new_vote_pk, ceremony.policy.vote_key);
    let (_, rot_rcpt) = rotated_core
        .authorize_and_sign_digest(
            CredentialRoleV1::ValidatorVote,
            SigningDomainTagV1::ConsensusVote,
            3,
            SignerWatermarkV1 {
                epoch: 0,
                height: 7,
                round: 0,
                phase: SignerPhaseCodeV1::PrepareVote,
            },
            [0x77; 32],
            [0x27; 32],
            &reg,
        )
        .unwrap();
    assert!(rot_rcpt.signature_emitted);
    assert_eq!(rot_rcpt.signer_public_key, new_vote_pk);

    // Wire policy negative control: allow_stale_backup_resume=true rejected
    let mut bad_pol = ceremony.policy.encode_canonical().unwrap();
    bad_pol[SignerPolicyManifestV1::WIRE_LEN - 1] = 1;
    assert!(SignerPolicyManifestV1::decode_canonical(&bad_pol).is_err());

    drop(standby);
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&dr_dir);
}
