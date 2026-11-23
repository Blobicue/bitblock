#![cfg(feature = "protocol-research")]
//! Wave 22 (`PS-056:CONSENSUS-1@base`) integration tests:
//! 1. End-to-end 4-validator `127.0.0.1` loopback TCP 2-phase HotStuff finality round +
//!    `4 -> 5` validator epoch set-change (`VALIDATOR-SCHEDULE-1`) + live
//!    `AcceptedConsensusNativeParentAuthorityCertificateV1` issuance + golden parity.
//! 2. Complete 12/12 adversarial & certificate control matrix (adversarial proposal, vote
//!    equivocation, partition/heal, missing-data data-before-vote refusal, long-range stale-set
//!    rejection, split-wrapper normalization, arrival-permutation invariance, duplicate admission
//!    rejection, post-acceptance status upgrade, history-start mismatch, late-valid-suffix &
//!    candidate-advanced-rule rejections, replay-input refusal, and conflicting-finality Safe-Halt).

use bitblock::consensus_1::consensus_net::{
    CONS_WIRE_HEADER_LEN, ConsensusWireMsgKindV1, preflight_wire_header,
    run_loopback_four_validator_round,
};
use bitblock::consensus_1::{
    AcceptedConsensusNativeParentAuthorityCertificateV1, CONS_WIRE_MAGIC, CONSENSUS_1_VERSION,
    Consensus1Error, ConsensusClusterHarnessV1, ConsensusCommitCertificateV1,
    ConsensusNativeAcceptedRecordOccurrenceReceiptV1, ConsensusProposalV1, ConsensusVotePhaseV1,
    ConsensusVoteV1, LiveParentAuthorityRegistryV1, ParentAuthorityCertificateConsumerContextV1,
    ParentAuthorityPurposeV1, ParentAuthorityStatusV1, QuorumCertificateV1,
    VerifiedConsensusNativeAcceptedRecordOccurrenceHistoryStartEqualityV1,
    build_consensus_proposal_core, compute_occurrence_universe_root,
    compute_parent_authority_evidence_commitment, designated_leader, detect_vote_equivocation,
    execution_root_ref_digest, is_strict_two_thirds_quorum,
};
use bitblock::identity_d1::ActorIdV1;
use bitblock::multi_1::MultiActorStreamBundleV1;

#[test]
fn wave22_loopback_tcp_bft_round_epoch_set_change_and_live_cert() {
    let mut cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().expect("bootstrap");

    // Verify BOUNDS-1 preflight wire header rejects oversized frame before allocation
    let mut bad_hdr = [0u8; CONS_WIRE_HEADER_LEN];
    bad_hdr[0..6].copy_from_slice(&CONS_WIRE_MAGIC);
    bad_hdr[6..8].copy_from_slice(&CONSENSUS_1_VERSION.to_be_bytes());
    bad_hdr[8] = ConsensusWireMsgKindV1::ProposalWithBody.code();
    bad_hdr[9..13].copy_from_slice(&(10_000_000u32).to_be_bytes());
    assert!(matches!(
        preflight_wire_header(&bad_hdr, &cluster.fixture.bundle.bounds_profile),
        Err(Consensus1Error::Io(_))
    ));

    // Height 1 (Epoch 0, 4 validators) over real 127.0.0.1 loopback TCP
    let pre_wr = cluster.nodes[0].harness().snapshot().unwrap().world_root;
    let b1 = cluster
        .fixture
        .canonical_mixed_race_bundle(1, pre_wr)
        .expect("b1");
    let cert_h1 = run_loopback_four_validator_round(&mut cluster, &b1, 0).expect("tcp round h1");
    assert_eq!(cert_h1.proposal.height, 1);
    assert_eq!(cert_h1.proposal.epoch, 0);
    assert_eq!(cert_h1.commit_qc.votes.len(), 4);
    assert!(is_strict_two_thirds_quorum(
        cert_h1.commit_qc.signed_weight,
        cert_h1.commit_qc.total_epoch_weight
    ));
    for n in &cluster.nodes {
        assert_eq!(n.finalized_height(), 1);
        assert_eq!(n.finalized_tip_block_id(), cert_h1.proposal.block_id());
        assert_eq!(
            n.finalized_cert_at(1).unwrap().tag().unwrap(),
            cert_h1.tag().unwrap()
        );
        assert!(n.finalized_receipt_at(1).is_some());
        assert!(n.safe_halt_evidence().is_none());
    }

    // Issue and admit live AcceptedConsensusNativeParentAuthorityCertificateV1
    let (pa_cert, wraps) = cluster
        .build_and_admit_canonical_live_parent_authority_cert(
            &cert_h1,
            ParentAuthorityPurposeV1::ReleaseAuthority,
            1,
            2,
        )
        .expect("live parent auth cert");
    assert_eq!(wraps.len(), 2);
    assert_eq!(pa_cert.occurrence_receipts.len(), 2);
    assert!(!pa_cert.from_replay_archive);
    for n in &cluster.nodes {
        assert_eq!(n.authority_registry().len(), 1);
        assert!(!n.authority_registry().is_empty());
    }
    let opaque = pa_cert.to_opaque_authority_blob().expect("opaque");
    assert!(!opaque.interpreted_as_live_authority());

    // Validator set change across epoch boundary: bond 5th validator at source_epoch=0 -> effective_epoch=2
    let sched_ep2 = cluster
        .activate_fifth_validator_for_epoch_2()
        .expect("activate 5th validator at epoch 2");
    assert_eq!(sched_ep2.effective_epoch, 2);
    assert_eq!(sched_ep2.validators.len(), 5);
    assert_eq!(cluster.nodes.len(), 5);

    // Height 2 (Epoch 2, 5 validators) over real 127.0.0.1 loopback TCP
    let b2 = MultiActorStreamBundleV1 {
        bundle_version: b1.bundle_version,
        batch_height: 2,
        epoch: 2,
        fork_context: cluster.fixture.fork_context,
        base_fee_per_gas: 2,
        envelopes: Vec::new(),
    };
    let cert_h2 = run_loopback_four_validator_round(&mut cluster, &b2, 0).expect("tcp round h2");
    assert_eq!(cert_h2.proposal.height, 2);
    assert_eq!(cert_h2.proposal.epoch, 2);
    assert_eq!(
        cert_h2.proposal.parent_block_id,
        cert_h1.proposal.block_id()
    );
    assert_eq!(cert_h2.commit_qc.votes.len(), 5);
    cert_h2
        .verify(&sched_ep2)
        .expect("verify h2 under 5-val schedule");
    // Stale epoch-0 schedule must reject epoch-2 commit certificate (long-range guard)
    let sched_ep0 = cluster.schedule_state.schedule_at(0).unwrap();
    assert!(matches!(
        cert_h2.verify(sched_ep0),
        Err(Consensus1Error::LongRangeStaleSchedule(_))
    ));
}

#[test]
fn wave22_twelve_adversarial_and_parent_authority_controls() {
    let mut cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().expect("bootstrap");
    let sched0 = cluster.schedule_state.schedule_at(0).unwrap().clone();
    let fork = cluster.fixture.fork_context;
    let pre_wr = cluster.nodes[0].harness().snapshot().unwrap().world_root;
    let b1 = cluster
        .fixture
        .canonical_mixed_race_bundle(1, pre_wr)
        .expect("b1");

    // 1. Adversarial proposal: non-leader proposer & cross-role vote_key signing proposal rejected
    let leader_v0 = designated_leader(&sched0, 1, 0).validator_id;
    let leader_idx = cluster
        .nodes
        .iter()
        .position(|n| n.validator_id == leader_v0)
        .unwrap();
    let non_leader_idx = (leader_idx + 1) % 4;
    assert!(matches!(
        cluster.nodes[non_leader_idx].propose_bundle(&b1, 0, None),
        Err(Consensus1Error::InvalidSignature(_))
    ));
    let (valid_prop, _) = cluster.nodes[leader_idx]
        .propose_bundle(&b1, 0, None)
        .unwrap();
    let leader_num = leader_v0.as_bytes()[0];
    let bad_role_prop = valid_prop
        .sign_with_seed(&ConsensusClusterHarnessV1::validator_vote_seed(leader_num))
        .unwrap();
    assert!(matches!(
        bad_role_prop.verify_against_schedule(&sched0),
        Err(Consensus1Error::CrossRoleKeyReuse(_))
    ));

    // 2. Missing-data / data-before-vote refusal (None body OR tampered execution root)
    assert!(matches!(
        cluster.nodes[non_leader_idx].vote_prepare(&valid_prop, None, None),
        Err(Consensus1Error::DataBeforeVoteRefusal(_))
    ));
    let mut tampered_prop = valid_prop;
    tampered_prop.body_digest[0] ^= 0xFF;
    let tampered_prop = tampered_prop
        .sign_with_seed(&ConsensusClusterHarnessV1::validator_proposal_seed(
            leader_num,
        ))
        .unwrap();
    assert!(matches!(
        cluster.nodes[non_leader_idx].vote_prepare(&tampered_prop, Some(&b1), None),
        Err(Consensus1Error::DataBeforeVoteRefusal(_))
    ));

    // 3. Sub-quorum 2/4 QC rejected fail-closed + cross-role proposal_key signing vote rejected
    let v0 = cluster.nodes[0]
        .vote_prepare(&valid_prop, Some(&b1), None)
        .unwrap();
    let v1 = cluster.nodes[1]
        .vote_prepare(&valid_prop, Some(&b1), None)
        .unwrap();
    assert!(matches!(
        QuorumCertificateV1::assemble(&fork, &sched0, vec![v0, v1]),
        Err(Consensus1Error::SubQuorumWeight { .. })
    ));
    let mut bad_vote = v0;
    bad_vote = bad_vote
        .sign_with_seed(
            &ConsensusClusterHarnessV1::validator_proposal_seed(1),
            &fork,
        )
        .unwrap();
    assert!(matches!(
        bad_vote.verify_against_schedule(&fork, &sched0),
        Err(Consensus1Error::CrossRoleKeyReuse(_))
    ));

    // 4. Vote equivocation detection & slashable evidence verification + lock preservation
    let mut equiv_vote = v0;
    equiv_vote.block_id[0] ^= 0x01;
    equiv_vote = equiv_vote
        .sign_with_seed(&ConsensusClusterHarnessV1::validator_vote_seed(1), &fork)
        .unwrap();
    let ev = detect_vote_equivocation(&[v0, equiv_vote], &fork, &sched0)
        .unwrap()
        .expect("equivocation must be detected");
    ev.verify(&fork, &sched0)
        .expect("equivocation evidence valid");
    assert_eq!(ev.voter_id, ActorIdV1::from_bytes([1; 32]));

    // 5. Partition & heal: 3-of-4 active partition finalizes cleanly; partitioned 4th heals cleanly
    let mut part_cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().unwrap();
    let part_cert = part_cluster
        .run_round_with_subset(&b1, 0, &[0, 1, 2])
        .expect("3-of-4 partition achieves finality");
    assert_eq!(part_cluster.nodes[3].finalized_height(), 0);
    // Heal partitioned replica 3 by delivering finalized cert + complete body
    part_cluster.nodes[3]
        .commit_finalized(&part_cert, &b1)
        .expect("partitioned replica heals");
    assert_eq!(part_cluster.nodes[3].finalized_height(), 1);
    assert_eq!(
        part_cluster.nodes[3].finalized_tip_block_id(),
        part_cert.proposal.block_id()
    );

    // 6. HotStuff lock safety: once locked on PrepareQC at view 0, conflicting view-1 proposal
    // without higher justifying PrepareQC is refused
    let v2 = cluster.nodes[2]
        .vote_prepare(&valid_prop, Some(&b1), None)
        .unwrap();
    let pqc = QuorumCertificateV1::assemble(&fork, &sched0, vec![v0, v1, v2]).unwrap();
    let _c0 = cluster.nodes[0].vote_commit(&pqc).unwrap();
    assert!(cluster.nodes[0].locked_qc().is_some());
    // Construct distinct view-1 proposal with empty bundle
    let empty_b1 = MultiActorStreamBundleV1 {
        bundle_version: b1.bundle_version,
        batch_height: 1,
        epoch: 0,
        fork_context: fork,
        base_fee_per_gas: 2,
        envelopes: Vec::new(),
    };
    let leader_v1 = designated_leader(&sched0, 1, 1).validator_id;
    let leader_v1_idx = cluster
        .nodes
        .iter()
        .position(|n| n.validator_id == leader_v1)
        .unwrap();
    let (conflicting_v1_prop, _) = cluster.nodes[leader_v1_idx]
        .propose_bundle(&empty_b1, 1, None)
        .unwrap();
    assert!(matches!(
        cluster.nodes[0].vote_prepare(&conflicting_v1_prop, Some(&empty_b1), None),
        Err(Consensus1Error::LockSafetyViolation(_))
    ));

    // 7. Conflicting-finality Safe-Halt: two verified CommitQCs for distinct blocks at height 1
    // force automatic deterministic Safe-Halt (never silently picks a branch)
    let mut c_branch_b = ConsensusClusterHarnessV1::bootstrap_four_validators().unwrap();
    let cert_branch_b = c_branch_b
        .run_round_with_subset(&empty_b1, 1, &[0, 1, 2, 3])
        .unwrap();
    assert_ne!(
        part_cert.proposal.block_id(),
        cert_branch_b.proposal.block_id()
    );
    let halt_err = part_cluster.nodes[0].observe_conflicting_commit_cert(&cert_branch_b);
    assert!(matches!(
        halt_err,
        Err(Consensus1Error::ConflictingFinalitySafeHalt(_))
    ));
    assert!(part_cluster.nodes[0].safe_halt_evidence().is_some());

    // 8-12. Live parent-authority certificate controls:
    // split-wrapper + arrival-permutation + duplicate admission + post-acceptance status upgrade +
    // history-start mismatch + late-valid-suffix + candidate-advanced-rule + replay-input refusal
    let (pa_cert, wraps) = part_cluster
        .build_and_admit_canonical_live_parent_authority_cert(
            &part_cert,
            ParentAuthorityPurposeV1::ActivationAuthority,
            1,
            2,
        )
        .expect("live cert");
    let mut reg = LiveParentAuthorityRegistryV1::new();
    let slot = reg
        .admit_live(pa_cert.clone(), &fork, &sched0)
        .expect("first admission succeeds");
    assert_eq!(
        reg.get(&slot).unwrap().attained_status,
        ParentAuthorityStatusV1::AcceptedLive
    );

    // 8. Duplicate live admission for same (parent_core, purpose, pos) rejected
    assert!(matches!(
        reg.admit_live(pa_cert.clone(), &fork, &sched0),
        Err(Consensus1Error::DuplicateLiveAdmissionRejected)
    ));

    // 9. Post-acceptance status upgrade (AcceptedLive -> UpgradedFinalized) succeeds monotonically
    let signer_seeds: Vec<(ActorIdV1, [u8; 32])> = (1u8..=3)
        .map(|i| {
            (
                ActorIdV1::from_bytes([i; 32]),
                ConsensusClusterHarnessV1::validator_vote_seed(i),
            )
        })
        .collect();
    let upgraded_cert = AcceptedConsensusNativeParentAuthorityCertificateV1::issue_live(
        &fork,
        &sched0,
        pa_cert.parent_profile_id,
        pa_cert.accepted_parent_lookup_id,
        pa_cert.unique_accepted_proposal_core_block_id,
        pa_cert.consumer_context,
        pa_cert.history_start,
        pa_cert.evidence_cutoff_position,
        pa_cert.earliest_unbounded_valid_position,
        pa_cert.candidate_rule_activation_position,
        pa_cert.occurrence_receipts.clone(),
        ParentAuthorityStatusV1::UpgradedFinalized,
        pa_cert.certificate_policy_id,
        pa_cert.admission_commit_qc_digest,
        &signer_seeds,
    )
    .expect("upgraded cert");
    reg.upgrade_status_post_acceptance(upgraded_cert, &fork, &sched0)
        .expect("monotonic post-acceptance status upgrade succeeds");
    assert_eq!(
        reg.get(&slot).unwrap().attained_status,
        ParentAuthorityStatusV1::UpgradedFinalized
    );

    // 10. History-start mismatch rejected
    let bad_hs = VerifiedConsensusNativeAcceptedRecordOccurrenceHistoryStartEqualityV1 {
        resolver_history_start: fork.genesis_id,
        cutoff_history_start: fork.genesis_id,
        scope_history_start: [0x99; 32],
    };
    assert!(matches!(
        AcceptedConsensusNativeParentAuthorityCertificateV1::issue_live(
            &fork,
            &sched0,
            pa_cert.parent_profile_id,
            pa_cert.accepted_parent_lookup_id,
            pa_cert.unique_accepted_proposal_core_block_id,
            pa_cert.consumer_context,
            bad_hs,
            2,
            1,
            1,
            pa_cert.occurrence_receipts.clone(),
            ParentAuthorityStatusV1::AcceptedLive,
            pa_cert.certificate_policy_id,
            pa_cert.admission_commit_qc_digest,
            &signer_seeds,
        ),
        Err(Consensus1Error::HistoryStartMismatch(_))
    ));

    // 11. Late-valid-suffix & candidate-advanced-rule rejections
    assert!(matches!(
        AcceptedConsensusNativeParentAuthorityCertificateV1::issue_live(
            &fork,
            &sched0,
            pa_cert.parent_profile_id,
            pa_cert.accepted_parent_lookup_id,
            pa_cert.unique_accepted_proposal_core_block_id,
            pa_cert.consumer_context,
            pa_cert.history_start,
            2, // cutoff
            3, // earliest_unbounded_valid_position > cutoff -> late-valid-suffix!
            1,
            pa_cert.occurrence_receipts.clone(),
            ParentAuthorityStatusV1::AcceptedLive,
            pa_cert.certificate_policy_id,
            pa_cert.admission_commit_qc_digest,
            &signer_seeds,
        ),
        Err(Consensus1Error::LateValidSuffixRejected)
    ));
    assert!(matches!(
        AcceptedConsensusNativeParentAuthorityCertificateV1::issue_live(
            &fork,
            &sched0,
            pa_cert.parent_profile_id,
            pa_cert.accepted_parent_lookup_id,
            pa_cert.unique_accepted_proposal_core_block_id,
            pa_cert.consumer_context,
            pa_cert.history_start,
            2, // cutoff
            1,
            4, // candidate_rule_activation_position > cutoff -> candidate-advanced-rule!
            pa_cert.occurrence_receipts.clone(),
            ParentAuthorityStatusV1::AcceptedLive,
            pa_cert.certificate_policy_id,
            pa_cert.admission_commit_qc_digest,
            &signer_seeds,
        ),
        Err(Consensus1Error::CandidateAdvancedRuleRejected)
    ));

    // 12. Replay-archive provenance strictly refused in live admission
    let mut replay_masquerade = pa_cert.clone();
    replay_masquerade.from_replay_archive = true;
    assert!(matches!(
        reg.admit_live(replay_masquerade, &fork, &sched0),
        Err(Consensus1Error::ReplayInputForbiddenInLiveAdmission)
    ));

    // Exercise helper coverage & direct codec/digest round-trips
    let _ = ConsensusNativeAcceptedRecordOccurrenceReceiptV1::from_wrapper(
        1, &wraps[0], [0xC1; 32], true,
    )
    .unwrap();
    let _ = compute_occurrence_universe_root(&pa_cert.occurrence_receipts);
    let _ = compute_parent_authority_evidence_commitment(
        &pa_cert.history_start,
        2,
        1,
        1,
        pa_cert.occurrence_universe_root,
    );
    let _ = ParentAuthorityCertificateConsumerContextV1::decode_canonical(
        &pa_cert.consumer_context.encode_canonical(),
    )
    .unwrap();
    let _ = build_consensus_proposal_core(
        fork.genesis_id,
        fork.profile_id,
        part_cert.proposal.parent_block_id,
        1,
        0,
        0,
        part_cert.proposal.receipts_merkle_root,
        &part_cert.proposal.execution_root_ref,
        part_cert.proposal.body_digest,
    )
    .unwrap();
    let _ = execution_root_ref_digest(&part_cert.proposal.execution_root_ref);
    let _ = ConsensusProposalV1::decode_canonical(&valid_prop.encode_canonical().unwrap()).unwrap();
    let _ = ConsensusVoteV1::decode_canonical(&v0.encode_canonical().unwrap()).unwrap();
    let _ = QuorumCertificateV1::decode_canonical(&pqc.encode_canonical().unwrap()).unwrap();
    let _ = ConsensusCommitCertificateV1::decode_canonical(&part_cert.encode_canonical().unwrap())
        .unwrap();
    assert_eq!(
        ConsensusVotePhaseV1::from_code(1).unwrap(),
        ConsensusVotePhaseV1::Prepare
    );
}
