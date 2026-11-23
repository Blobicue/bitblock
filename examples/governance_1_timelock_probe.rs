//! Release probe for Wave 14C `GOVERNANCE-1` (`PS-081`):
//! exercises the complete timelocked proposal -> pause/cooldown -> Safe-Halt -> stake-only restart
//! lifecycle plus all 8 adversarial governance gates and emits JSON for `plot_governance_1.py`.

use bitblock::bounds_1::SafeHaltEvidenceV1;
use bitblock::consensus_gov_d1::{
    GovernancePolicyDescriptorV1, InitialValidatorDescriptorV1, InitialValidatorSetV1,
};
use bitblock::crypto_d1::{
    CanonicalEd25519PublicKeyV1, SigningDomainTagV1, sign_preimage_deterministic,
};
use bitblock::governance_1::{
    GovernanceContextV1, GovernanceDispositionV1, GovernanceProposalSubmissionV1,
    GovernanceStateV1, emergency_pause_digest, proposal_approval_digest,
};
use bitblock::identity_d1::{ActorIdV1, CredentialRoleV1, ThresholdPolicyV1};

fn pk(seed: u8) -> CanonicalEd25519PublicKeyV1 {
    CanonicalEd25519PublicKeyV1::from_secret_seed(&[seed; 32]).unwrap()
}

fn main() {
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
    let vroot = vset.active_validator_root();
    let mut gov = GovernanceStateV1::from_genesis(ctx, policy, &[(1, 50_000_000)]).unwrap();

    let sign_prop = |pid: [u8; 32],
                     pkey: u32,
                     pval: u64,
                     app: u64,
                     act: u64,
                     exp: u64,
                     is_restart: bool,
                     root: [u8; 32],
                     signers: &[u8]| {
        let dig = proposal_approval_digest(&root, &pid, pkey, pval, app, act, exp, is_restart);
        let pre = ctx.preimage(SigningDomainTagV1::GovernanceAction, dig);
        GovernanceProposalSubmissionV1 {
            proposal_id: pid,
            param_key: pkey,
            param_value: pval,
            activation_epoch: act,
            expiry_epoch: exp,
            is_restart,
            submitted_validator_root: root,
            approvals: signers
                .iter()
                .map(|&i| {
                    (
                        ActorIdV1::from_bytes([i; 32]),
                        sign_preimage_deterministic(&[10 + i; 32], &pre).unwrap(),
                    )
                })
                .collect(),
        }
    };

    // Gate 1: Unauthorized (< 2/3 stake)
    assert_eq!(
        gov.queue_proposal(
            &vset,
            &sign_prop([1; 32], 1, 60_000_000, 1, 22, 30, false, vroot, &[1, 2]),
            1,
        )
        .unwrap()
        .disposition,
        GovernanceDispositionV1::RejectedUnauthorized
    );

    // Queue valid proposal at e=1 -> e=22
    let pid = [0xAA; 32];
    let ok_sub = sign_prop(pid, 1, 60_000_000, 1, 22, 30, false, vroot, &[1, 2, 3]);
    assert_eq!(
        gov.queue_proposal(&vset, &ok_sub, 1).unwrap().disposition,
        GovernanceDispositionV1::Admitted
    );
    let queued_tag = gov.root_commitment().tag().unwrap().to_string();

    // Gate 2: Replay & Gate 3: Conflict & Gate 4: Rushed timelock
    assert_eq!(
        gov.queue_proposal(&vset, &ok_sub, 1).unwrap().disposition,
        GovernanceDispositionV1::RejectedReplayedProposal
    );
    assert_eq!(
        gov.queue_proposal(
            &vset,
            &sign_prop([2; 32], 1, 70_000_000, 1, 22, 30, false, vroot, &[1, 2, 3]),
            1,
        )
        .unwrap()
        .disposition,
        GovernanceDispositionV1::RejectedConflictingProposal
    );
    assert_eq!(
        gov.activate_proposal(pid, 21).unwrap().disposition,
        GovernanceDispositionV1::RejectedRushedTimelock
    );

    // Activate at e=22
    assert_eq!(
        gov.activate_proposal(pid, 22).unwrap().disposition,
        GovernanceDispositionV1::Admitted
    );
    let activated_tag = gov.root_commitment().tag().unwrap().to_string();

    // Gate 5: Pause e=25..28 & anti-DoS cooldown rejection at e=28
    let ev = [0xEE; 32];
    let pause_sigs = |e: u64, d: u64| {
        let dig = emergency_pause_digest(e, d, &ev);
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
    assert_eq!(
        gov.trigger_emergency_pause(25, 3, ev, &pause_sigs(25, 3))
            .unwrap()
            .disposition,
        GovernanceDispositionV1::Admitted
    );
    assert_eq!(
        gov.trigger_emergency_pause(28, 1, ev, &pause_sigs(28, 1))
            .unwrap()
            .disposition,
        GovernanceDispositionV1::RejectedPauseCooldownDoS
    );

    // Gate 6: Stale set
    assert_eq!(
        gov.queue_proposal(
            &vset,
            &sign_prop(
                [3; 32],
                1,
                80_000_000,
                31,
                52,
                60,
                false,
                [0xFF; 32],
                &[1, 2, 3],
            ),
            31,
        )
        .unwrap()
        .disposition,
        GovernanceDispositionV1::RejectedStaleValidatorSet
    );

    // Gate 7 & 8: Emergency overreach & history rewrite
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

    // Safe-Halt at e=35, Stake-only restart queued at e=35 -> activated at e=56
    let sh_ev = SafeHaltEvidenceV1 {
        schema_version: 1,
        bounds_profile_digest: [0x77; 32],
        artifact_digest: [0x88; 32],
        violation_code: 3,
        observed_value: 20_000,
        committed_limit: 12_288,
    };
    assert_eq!(
        gov.trigger_safe_halt_from_bounds_evidence(&sh_ev, 35)
            .unwrap()
            .disposition,
        GovernanceDispositionV1::Admitted
    );
    let r_pid = [0x99; 32];
    assert_eq!(
        gov.queue_proposal(
            &vset,
            &sign_prop(r_pid, 0, 56, 35, 56, 70, true, vroot, &[1, 2, 3]),
            35,
        )
        .unwrap()
        .disposition,
        GovernanceDispositionV1::Admitted
    );
    assert_eq!(
        gov.activate_proposal(r_pid, 56).unwrap().disposition,
        GovernanceDispositionV1::Admitted
    );
    let restarted_tag = gov.root_commitment().tag().unwrap().to_string();
    let proof_bytes = gov
        .prove_param(1)
        .unwrap()
        .encode_canonical()
        .unwrap()
        .len();
    gov.verify_rebuild_parity().unwrap();

    println!("{{");
    println!("  \"queued_root_tag\": \"{}\",", queued_tag);
    println!("  \"activated_root_tag\": \"{}\",", activated_tag);
    println!("  \"restarted_root_tag\": \"{}\",", restarted_tag);
    println!("  \"queued_root_tag_short\": \"{}\",", &queued_tag[..25]);
    println!(
        "  \"activated_root_tag_short\": \"{}\",",
        &activated_tag[..25]
    );
    println!(
        "  \"restarted_root_tag_short\": \"{}\",",
        &restarted_tag[..25]
    );
    println!("  \"proof_bytes\": {},", proof_bytes);
    println!("  \"gates_passed\": 8,");
    println!("  \"gates_total\": 8,");
    println!("  \"gate_rows\": [");
    println!(
        "    {{\"gate\": \"Unauthorized (<= 2/3 Stake)\", \"disposition\": \"RejectedUnauthorized\"}},"
    );
    println!(
        "    {{\"gate\": \"Replayed Proposal ID\", \"disposition\": \"RejectedReplayedProposal\"}},"
    );
    println!(
        "    {{\"gate\": \"Conflicting Param Key\", \"disposition\": \"RejectedConflictingProposal\"}},"
    );
    println!(
        "    {{\"gate\": \"Rushed Timelock (e < +21)\", \"disposition\": \"RejectedRushedTimelock\"}},"
    );
    println!(
        "    {{\"gate\": \"Back-to-Back Pause DoS\", \"disposition\": \"RejectedPauseCooldownDoS\"}},"
    );
    println!(
        "    {{\"gate\": \"Stale Set / Compromised Key\", \"disposition\": \"RejectedStale/Compromised\"}},"
    );
    println!(
        "    {{\"gate\": \"Council Mutate / Unhalt\", \"disposition\": \"RejectedEmergencyOverreach\"}},"
    );
    println!(
        "    {{\"gate\": \"History Rewrite / Rollback\", \"disposition\": \"RejectedHistoryRewrite\"}}"
    );
    println!("  ]");
    println!("}}");
}
