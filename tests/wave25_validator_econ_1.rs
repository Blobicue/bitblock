#![cfg(feature = "protocol-research")]

use bitblock::consensus_1::{ConsensusClusterHarnessV1, VoteEquivocationEvidenceV1};
use bitblock::economic_state_1::EconomicDispositionV1;
use bitblock::identity_d1::ActorIdV1;
use bitblock::validator_econ_1::{
    ALL_SEVEN_ATTACK_VECTORS_MASK, MonitoredLossConditionV1, ValidatorEcon1Error,
    ValidatorEconActionV1, ValidatorEconAdapterReceiptV1, ValidatorEconAdapterV1,
    ValidatorEconAdversarialAssayV1, nakamoto_lower_bounds, reject_occupancy_token_as_stake,
    run_canonical_validator_econ_adversarial_assay, sign_consensus_vote_for_sim,
    verify_weak_subjectivity_key_freshness,
};
use bitblock::world::Coord;

#[test]
fn wave25_consensus_to_economic_receipt_correspondence_and_slashing() {
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
    let policy = cluster.fixture.bundle.validator_econ_policy;
    let adapter = ValidatorEconAdapterV1::new(cluster.fixture.fork_context, policy).unwrap();

    // 1. Read-only schedule weight correspondence audit against ECONOMIC-STATE-1
    let audit = adapter
        .audit_consensus_schedule_weights(&cert, &sched0, &cluster.economic_state)
        .unwrap();
    assert_eq!(
        audit.action_kind,
        ValidatorEconActionV1::WeightScheduleAudit
    );
    assert_eq!(audit.validator_count, 4);
    assert_eq!(audit.total_active_weight, 400_000_000_000);
    assert!(!audit.produces_execution_root_domain);
    assert!(!audit.block_token_used_as_stake);

    // 2. Pro-rata epoch rewards with integer-floor dust = 3 unissued
    let mut econ = cluster.economic_state.clone();
    let (rew_adapter, rew_econ) = adapter
        .apply_epoch_rewards_for_finalized_block(&cert, &sched0, &mut econ, 999_999_999, 1)
        .unwrap();
    assert_eq!(rew_adapter.reward_or_slash_amount, 999_999_996);
    assert_eq!(rew_adapter.reward_dust_unissued, 3);
    assert_eq!(rew_econ.disposition, EconomicDispositionV1::Admitted);
    econ.verify_conservation().unwrap();
    econ.verify_rebuild_parity().unwrap();

    // 3. Slash-and-flee defeat: Validator 1 unbonds 20B, then submits conflicting Prepare votes.
    //    50% burns across BOTH active (80B) AND unbonding (20B) = 50B burned + permanent jail!
    let v1 = ActorIdV1::from_bytes([1u8; 32]);
    let unb = econ.request_unbond(v1, 20_000_000_000, 1).unwrap();
    assert_eq!(unb.disposition, EconomicDispositionV1::Admitted);
    let vote_a = sign_consensus_vote_for_sim(
        &cluster.fixture.fork_context,
        &sched0,
        &cert,
        1,
        2,
        [0xAA; 32],
    )
    .unwrap();
    let vote_b = sign_consensus_vote_for_sim(
        &cluster.fixture.fork_context,
        &sched0,
        &cert,
        1,
        2,
        [0xBB; 32],
    )
    .unwrap();
    let ev = VoteEquivocationEvidenceV1 {
        voter_id: v1,
        first_vote: vote_a,
        second_vote: vote_b,
    };
    let (eq_adapter, eq_econ) = adapter
        .apply_verified_equivocation_slash(&cert, &sched0, &ev, &mut econ, 1)
        .unwrap();
    assert_eq!(
        eq_adapter.action_kind,
        ValidatorEconActionV1::EquivocationSlash
    );
    assert_eq!(eq_adapter.reward_or_slash_amount, 50_000_000_000);
    assert_eq!(eq_econ.amount_delta, 50_000_000_000);
    assert!(econ.validator_record(&v1).unwrap().jailed);
    assert_eq!(
        econ.validator_record(&v1).unwrap().bonded_stake,
        40_000_000_000
    );
    assert_eq!(econ.eligible_validators().len(), 3);
    econ.verify_conservation().unwrap();

    // 4. Sustained inactivity slash (1% burn) + Domain 6 Patricia inclusion proof parity
    let v2 = ActorIdV1::from_bytes([2u8; 32]);
    let (inact_adapter, inact_econ) = adapter
        .apply_inactivity_slash(&cert, &sched0, v2, 16, 10, &mut econ, 1)
        .unwrap();
    assert_eq!(inact_adapter.reward_or_slash_amount, 1_000_000_000);
    assert_eq!(inact_econ.amount_delta, 1_000_000_000);
    let root_c = econ.root_commitment().unwrap();
    let proof_v2 = econ.prove_validator(&v2).unwrap();
    let verified = proof_v2
        .verify(
            bitblock::auth_commit_1::DomainIdV1::EconomicStake,
            root_c.economic_trie_root,
            v2.as_bytes(),
        )
        .unwrap();
    assert!(verified.is_some());
}

#[test]
fn wave25_seven_vector_adversarial_assay_and_negative_controls() {
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
    let policy = cluster.fixture.bundle.validator_econ_policy;
    let adapter = ValidatorEconAdapterV1::new(cluster.fixture.fork_context, policy).unwrap();

    // Full 7-vector adversarial assay
    let (assay, outcomes) =
        run_canonical_validator_econ_adversarial_assay(&cluster, &cert).unwrap();
    assert_eq!(assay.passed_attack_mask, ALL_SEVEN_ATTACK_VECTORS_MASK);
    assert_eq!(outcomes.len(), 7);
    assert!(outcomes.iter().all(|o| o.passed));
    assert_eq!(
        outcomes[0].loss_condition,
        Some(MonitoredLossConditionV1::BriberyExceedsSlashFloor)
    );
    assert_eq!(
        outcomes[1].loss_condition,
        Some(MonitoredLossConditionV1::CensorshipCartelAboveOneThird)
    );
    assert_eq!(assay.min_nakamoto_halt, 2);
    assert_eq!(assay.min_nakamoto_safety, 3);
    assert_eq!(assay.min_safety_attack_burn_bps, 1_667);

    // Negative control 1: block occupancy token MUST NEVER become validator stake
    assert!(matches!(
        reject_occupancy_token_as_stake(Coord::new(3, 0, 0), [0x55; 32], 1_000_000),
        Err(ValidatorEcon1Error::BlockTokenStakeAliasingForbidden { .. })
    ));

    // Negative control 2: produces_execution_root_domain=true wire tamper rejected fail-closed
    let audit = adapter
        .audit_consensus_schedule_weights(&cert, &sched0, &cluster.economic_state)
        .unwrap();
    let mut raw_dom = audit.encode_canonical().unwrap();
    raw_dom[ValidatorEconAdapterReceiptV1::WIRE_LEN - 2] = 1;
    assert!(matches!(
        ValidatorEconAdapterReceiptV1::decode_canonical(&raw_dom),
        Err(ValidatorEcon1Error::ExecutionRootDomainForbidden)
    ));

    // Negative control 3: block_token_used_as_stake=true wire tamper rejected fail-closed
    let mut raw_tok = audit.encode_canonical().unwrap();
    raw_tok[ValidatorEconAdapterReceiptV1::WIRE_LEN - 1] = 1;
    assert!(ValidatorEconAdapterReceiptV1::decode_canonical(&raw_tok).is_err());

    // Negative control 4: assay with missing attack vector (mask != 0x7F) rejected fail-closed
    let mut raw_assay = assay.encode_canonical().unwrap();
    raw_assay[196 - 32 - 1 - 4 - 1] = 0x3F;
    assert!(ValidatorEconAdversarialAssayV1::decode_canonical(&raw_assay).is_err());

    // Negative control 5: wrong ExecutionRootRef / wrong fork context rejected
    let mut bad_cert = cert.clone();
    bad_cert.proposal.execution_root_ref.profile_id = [0xEE; 32];
    assert!(
        adapter
            .audit_consensus_schedule_weights(&bad_cert, &sched0, &cluster.economic_state)
            .is_err()
    );

    // Negative control 6: non-conflicting same-block votes rejected as equivocation evidence
    let v1 = ActorIdV1::from_bytes([1u8; 32]);
    let same_vote = sign_consensus_vote_for_sim(
        &cluster.fixture.fork_context,
        &sched0,
        &cert,
        1,
        2,
        [0x11; 32],
    )
    .unwrap();
    let bogus_ev = VoteEquivocationEvidenceV1 {
        voter_id: v1,
        first_vote: same_vote,
        second_vote: same_vote,
    };
    let mut econ_tmp = cluster.economic_state.clone();
    assert!(
        adapter
            .apply_verified_equivocation_slash(&cert, &sched0, &bogus_ev, &mut econ_tmp, 1)
            .is_err()
    );

    // Negative control 7: CE-15 concentration ceiling > 3333 bps & long-range stale epoch rejected
    assert!(nakamoto_lower_bounds(3_334).is_err());
    assert!(matches!(
        verify_weak_subjectivity_key_freshness(0, 22, &policy),
        Err(ValidatorEcon1Error::LongRangeUnbondedKeyRejected { .. })
    ));

    // Negative control 8: consensus schedule weight drift from ECONOMIC-STATE-1 rejected
    let mut drifted_sched = sched0.clone();
    drifted_sched.validators[0].effective_weight += 1;
    assert!(
        adapter
            .audit_consensus_schedule_weights(&cert, &drifted_sched, &cluster.economic_state)
            .is_err()
    );
}
