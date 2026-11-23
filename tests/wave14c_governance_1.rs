//! Wave 14C integration suite for `GOVERNANCE-1` (`PS-081`):
//! exercises all 10 `CAPABILITY_TREE.md` gate vectors: unauthorized, replayed, conflicting,
//! rushed timelock, back-to-back pause censorship DoS, stale validator set, compromised key,
//! rollback/history-rewrite, emergency-council overreach, and automatic Safe-Halt + stake restart.

#![cfg(feature = "protocol-research")]

use bitblock::auth_commit_1::{AuthCommit1Error, DomainIdV1};
use bitblock::bounds_1::SafeHaltEvidenceV1;
use bitblock::consensus_gov_d1::{
    GovernancePolicyDescriptorV1, InitialValidatorDescriptorV1, InitialValidatorSetV1,
};
use bitblock::crypto_d1::{
    CanonicalEd25519PublicKeyV1, SigningDomainTagV1, sign_preimage_deterministic,
};
use bitblock::governance_1::{
    GovernanceContextV1, GovernanceDispositionV1, GovernanceLifecycleCodeV1,
    GovernanceProposalSubmissionV1, GovernanceStateV1, ProposalRecordV1, emergency_pause_digest,
    param_trie_key, proposal_approval_digest, proposal_trie_key,
};
use bitblock::identity_d1::{ActorIdV1, CredentialRoleV1, ThresholdPolicyV1};

fn pk(seed: u8) -> CanonicalEd25519PublicKeyV1 {
    CanonicalEd25519PublicKeyV1::from_secret_seed(&[seed; 32]).unwrap()
}

fn setup_gov() -> (
    GovernanceContextV1,
    InitialValidatorSetV1,
    GovernanceStateV1,
) {
    let ctx = GovernanceContextV1 {
        protocol_version: 1,
        chain_id: 77,
        genesis_id: [0x11; 32],
        launch_manifest_id: [0x22; 32],
        profile_id: [0x55; 32],
    };
    let mut council_keys = vec![pk(81), pk(82), pk(83)];
    council_keys.sort();
    let policy = GovernancePolicyDescriptorV1 {
        schema_version: 1,
        upgrade_quorum_numerator: 2,
        upgrade_quorum_denominator: 3,
        mandatory_exit_timelock_epochs: 21,
        max_emergency_pause_epochs: 3,
        automatic_safe_halt_on_proven_fault: true,
        emergency_council_pause_only: true,
        forbid_history_rewrite: true,
        emergency_council: ThresholdPolicyV1 {
            role: CredentialRoleV1::EmergencyPauseSigner,
            threshold: 2,
            keys: council_keys,
        },
    };
    let mut vals = Vec::new();
    for i in 1u8..=4 {
        vals.push(InitialValidatorDescriptorV1 {
            validator_id: ActorIdV1::from_bytes([i; 32]),
            vote_key: pk(10 + i),
            proposal_key: pk(20 + i),
            weight: 100,
        });
    }
    let vset = InitialValidatorSetV1::new(vals).unwrap();
    let gov = GovernanceStateV1::from_genesis(ctx, policy, &[(1, 50_000_000), (2, 1_000)]).unwrap();
    (ctx, vset, gov)
}

#[allow(clippy::too_many_arguments)]
fn make_submission(
    ctx: &GovernanceContextV1,
    vset_root: [u8; 32],
    proposal_id: [u8; 32],
    param_key: u32,
    param_value: u64,
    approved_epoch: u64,
    activation_epoch: u64,
    expiry_epoch: u64,
    is_restart: bool,
    signer_ids: &[u8],
) -> GovernanceProposalSubmissionV1 {
    let dig = proposal_approval_digest(
        &vset_root,
        &proposal_id,
        param_key,
        param_value,
        approved_epoch,
        activation_epoch,
        expiry_epoch,
        is_restart,
    );
    let pre = ctx.preimage(SigningDomainTagV1::GovernanceAction, dig);
    let approvals = signer_ids
        .iter()
        .map(|&i| {
            (
                ActorIdV1::from_bytes([i; 32]),
                sign_preimage_deterministic(&[10 + i; 32], &pre).unwrap(),
            )
        })
        .collect();
    GovernanceProposalSubmissionV1 {
        proposal_id,
        param_key,
        param_value,
        activation_epoch,
        expiry_epoch,
        is_restart,
        submitted_validator_root: vset_root,
        approvals,
    }
}

#[test]
fn governance_1_timelock_conflict_replay_expiry_stale_and_compromised_gates() {
    let (ctx, vset, mut gov) = setup_gov();
    let vroot = vset.active_validator_root();

    // 1. Unauthorized (< 2/3 stake: 2 of 4 = 50% <= 66.7%) -> RejectedUnauthorized
    let sub_low = make_submission(
        &ctx,
        vroot,
        [1; 32],
        1,
        60_000_000,
        1,
        22,
        30,
        false,
        &[1, 2],
    );
    assert_eq!(
        gov.queue_proposal(&vset, &sub_low, 1).unwrap().disposition,
        GovernanceDispositionV1::RejectedUnauthorized
    );

    // 2. Rushed timelock (activation 21 < 1 + 21 = 22) -> RejectedRushedTimelock
    let sub_rush = make_submission(
        &ctx,
        vroot,
        [2; 32],
        1,
        60_000_000,
        1,
        21,
        30,
        false,
        &[1, 2, 3],
    );
    assert_eq!(
        gov.queue_proposal(&vset, &sub_rush, 1).unwrap().disposition,
        GovernanceDispositionV1::RejectedRushedTimelock
    );

    // 3. Stale validator set root -> RejectedStaleValidatorSet
    let sub_stale = make_submission(
        &ctx,
        [0xFF; 32],
        [3; 32],
        1,
        60_000_000,
        1,
        22,
        30,
        false,
        &[1, 2, 3],
    );
    assert_eq!(
        gov.queue_proposal(&vset, &sub_stale, 1)
            .unwrap()
            .disposition,
        GovernanceDispositionV1::RejectedStaleValidatorSet
    );

    // 4. Valid 3-of-4 (75% > 2/3) queue at epoch 1 for activation at 22
    let pid_ok = [0x10; 32];
    let sub_ok = make_submission(
        &ctx,
        vroot,
        pid_ok,
        1,
        65_000_000,
        1,
        22,
        30,
        false,
        &[1, 2, 3],
    );
    assert_eq!(
        gov.queue_proposal(&vset, &sub_ok, 1).unwrap().disposition,
        GovernanceDispositionV1::Admitted
    );

    // 5. Replay same proposal_id -> RejectedReplayedProposal
    assert_eq!(
        gov.queue_proposal(&vset, &sub_ok, 2).unwrap().disposition,
        GovernanceDispositionV1::RejectedReplayedProposal
    );

    // 6. Conflicting second proposal targeting same param_key=1 -> RejectedConflictingProposal
    let sub_conflict = make_submission(
        &ctx,
        vroot,
        [0x11; 32],
        1,
        70_000_000,
        2,
        23,
        30,
        false,
        &[1, 2, 3],
    );
    assert_eq!(
        gov.queue_proposal(&vset, &sub_conflict, 2)
            .unwrap()
            .disposition,
        GovernanceDispositionV1::RejectedConflictingProposal
    );

    // 7. Early activation at epoch 21 -> RejectedRushedTimelock; activation at epoch 22 succeeds!
    assert_eq!(
        gov.activate_proposal(pid_ok, 21).unwrap().disposition,
        GovernanceDispositionV1::RejectedRushedTimelock
    );
    assert_eq!(
        gov.activate_proposal(pid_ok, 22).unwrap().disposition,
        GovernanceDispositionV1::Admitted
    );
    assert_eq!(gov.param_value(1), Some(65_000_000));
    // Re-activating already activated proposal -> RejectedReplayedProposal
    assert_eq!(
        gov.activate_proposal(pid_ok, 23).unwrap().disposition,
        GovernanceDispositionV1::RejectedReplayedProposal
    );

    // 8. Compromised key revocation: revoke validator 1's key -> submission using signer 1 fails closed!
    gov.revoke_compromised_key(pk(11));
    let sub_comp = make_submission(
        &ctx,
        vroot,
        [0x12; 32],
        2,
        2_000,
        24,
        45,
        50,
        false,
        &[1, 2, 3],
    );
    assert_eq!(
        gov.queue_proposal(&vset, &sub_comp, 24)
            .unwrap()
            .disposition,
        GovernanceDispositionV1::RejectedCompromisedKey
    );

    // 9. Verify inclusion proofs & cross-domain rejection on GovernanceStateRootV1
    let root = gov.root_commitment();
    let p_proof = gov.prove_param(1).unwrap();
    let val_bytes = p_proof
        .verify(
            DomainIdV1::GovernanceState,
            root.governance_trie_root,
            &param_trie_key(1),
        )
        .unwrap()
        .expect("param 1 present");
    assert_eq!(
        u64::from_be_bytes(val_bytes.try_into().unwrap()),
        65_000_000
    );
    assert!(matches!(
        p_proof.verify(
            DomainIdV1::Authorization,
            root.governance_trie_root,
            &param_trie_key(1),
        ),
        Err(AuthCommit1Error::CrossDomainProofRejected { .. })
    ));
    let pr_proof = gov.prove_proposal(&pid_ok).unwrap();
    let pr_bytes = pr_proof
        .verify(
            DomainIdV1::GovernanceState,
            root.governance_trie_root,
            &proposal_trie_key(&pid_ok),
        )
        .unwrap()
        .expect("proposal present");
    assert_eq!(
        ProposalRecordV1::decode_canonical(&pr_bytes)
            .unwrap()
            .param_value,
        65_000_000
    );
    gov.verify_rebuild_parity().unwrap();
}

#[test]
fn governance_1_pause_cooldown_dos_safe_halt_overreach_and_stake_restart() {
    let (ctx, vset, mut gov) = setup_gov();
    let vroot = vset.active_validator_root();

    let sign_pause = |epoch: u64, dur: u64, ev: [u8; 32]| {
        let dig = emergency_pause_digest(epoch, dur, &ev);
        let pre = ctx.preimage(SigningDomainTagV1::EmergencyPause, dig);
        vec![
            (
                pk(81),
                sign_preimage_deterministic(&[81; 32], &pre).unwrap(),
            ),
            (
                pk(82),
                sign_preimage_deterministic(&[82; 32], &pre).unwrap(),
            ),
        ]
    };

    // 1. Over-duration pause (4 > 3) rejected
    assert_eq!(
        gov.trigger_emergency_pause(25, 4, [0xE1; 32], &sign_pause(25, 4, [0xE1; 32]))
            .unwrap()
            .disposition,
        GovernanceDispositionV1::RejectedPauseCooldownDoS
    );

    // 2. Valid 3-epoch pause at epoch 25 (auto-expires at 28, cooldown until 31)
    assert_eq!(
        gov.trigger_emergency_pause(25, 3, [0xE2; 32], &sign_pause(25, 3, [0xE2; 32]))
            .unwrap()
            .disposition,
        GovernanceDispositionV1::Admitted
    );
    assert_eq!(gov.lifecycle(), GovernanceLifecycleCodeV1::EmergencyPaused);

    // 3. Back-to-back pause at epoch 28 (just expired) and 30 -> REJECTED by anti-DoS cooldown!
    assert_eq!(
        gov.trigger_emergency_pause(28, 2, [0xE3; 32], &sign_pause(28, 2, [0xE3; 32]))
            .unwrap()
            .disposition,
        GovernanceDispositionV1::RejectedPauseCooldownDoS
    );
    assert_eq!(
        gov.trigger_emergency_pause(30, 1, [0xE4; 32], &sign_pause(30, 1, [0xE4; 32]))
            .unwrap()
            .disposition,
        GovernanceDispositionV1::RejectedPauseCooldownDoS
    );
    // Advance epoch 30 -> pause auto-expired cleanly back to Normal!
    gov.advance_epoch(30).unwrap();
    assert_eq!(gov.lifecycle(), GovernanceLifecycleCodeV1::Normal);

    // 4. Emergency council overreach & finalized-history rewrite MUST always reject fail-closed
    assert_eq!(
        gov.attempt_emergency_overreach(31, [0x91; 32])
            .unwrap()
            .disposition,
        GovernanceDispositionV1::RejectedEmergencyOverreach
    );
    assert_eq!(
        gov.attempt_history_rewrite(31, [0x92; 32])
            .unwrap()
            .disposition,
        GovernanceDispositionV1::RejectedHistoryRewrite
    );
    assert_eq!(
        gov.trigger_emergency_pause(20, 1, [0xE5; 32], &sign_pause(20, 1, [0xE5; 32]))
            .unwrap()
            .disposition,
        GovernanceDispositionV1::RejectedHistoryRewrite
    );

    // 5. Automatic Safe-Halt on proven BOUNDS-1 over-limit evidence at epoch 35
    let fault = SafeHaltEvidenceV1 {
        schema_version: 1,
        bounds_profile_digest: [0x77; 32],
        artifact_digest: [0x88; 32],
        violation_code: 3,
        observed_value: 20_000,
        committed_limit: 12_288,
    };
    assert_eq!(
        gov.trigger_safe_halt_from_bounds_evidence(&fault, 35)
            .unwrap()
            .disposition,
        GovernanceDispositionV1::Admitted
    );
    assert_eq!(gov.lifecycle(), GovernanceLifecycleCodeV1::SafeHalted);

    // 6. While SafeHalted: Emergency Council pause/unhalt and ordinary param proposals REJECT!
    assert_eq!(
        gov.trigger_emergency_pause(36, 1, [0xE6; 32], &sign_pause(36, 1, [0xE6; 32]))
            .unwrap()
            .disposition,
        GovernanceDispositionV1::RejectedEmergencyOverreach
    );
    let sub_param = make_submission(
        &ctx,
        vroot,
        [0x50; 32],
        2,
        9_999,
        36,
        57,
        65,
        false,
        &[1, 2, 3],
    );
    assert_eq!(
        gov.queue_proposal(&vset, &sub_param, 36)
            .unwrap()
            .disposition,
        GovernanceDispositionV1::RejectedWhileHaltedOrInvalidFault
    );

    // 7. Stake-only > 2/3 timelocked Safe-Halt restart queued at 35 -> activates at 56!
    let restart_pid = [0x99; 32];
    let sub_restart = make_submission(
        &ctx,
        vroot,
        restart_pid,
        0,
        56,
        35,
        56,
        70,
        true,
        &[1, 2, 3],
    );
    assert_eq!(
        gov.queue_proposal(&vset, &sub_restart, 35)
            .unwrap()
            .disposition,
        GovernanceDispositionV1::Admitted
    );
    assert_eq!(
        gov.activate_proposal(restart_pid, 55).unwrap().disposition,
        GovernanceDispositionV1::RejectedRushedTimelock
    );
    assert_eq!(
        gov.activate_proposal(restart_pid, 56).unwrap().disposition,
        GovernanceDispositionV1::Admitted
    );
    assert_eq!(gov.lifecycle(), GovernanceLifecycleCodeV1::Normal);
    gov.verify_rebuild_parity().unwrap();
}
