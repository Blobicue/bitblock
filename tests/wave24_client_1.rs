#![cfg(feature = "protocol-research")]
//! Wave 24 (`PS-057:CLIENT-1@base`) integration tests:
//! 1. Complete 6-state lifecycle (`Submitted -> Admitted -> IncludedProvisional -> Finalized` +
//!    `Replaced` + `Rejected`), `ReceiptMerkleProofV1` + `DomainPatriciaProofV1` verification
//!    across an `epoch 0 -> epoch 2` validator-set transition (`4 -> 5` validators), and golden
//!    parity.
//! 2. Complete 8/8 reorg, phantom-ownership & trust-input adversarial matrix (competing candidates,
//!    delayed inclusion, duplicate/nonce-replaced tx, wrong-root state proof refused, missing body
//!    refused, pre-finality fork mint/burn/ownership rollback proving zero phantom ownership/supply,
//!    expiry + resubmission, and stale weak-subjectivity checkpoint refused).

use std::collections::BTreeMap;

use bitblock::asset_1::{EpisodeOwnershipValueV1, episode_trie_key};
use bitblock::auth_commit_1::{DomainIdV1, DomainPatriciaTrieV1};
use bitblock::client_1::{
    Client1Error, ClientRootStatusV1, ClientStateTrackerV1, ClientTxLifecycleReceiptV1,
    ClientTxStatusV1, ClientVerifiedStateViewV1, MAX_WEAK_SUBJECTIVITY_AGE_EPOCHS,
    compute_finalized_state_summary_digest, snapshot_cluster_finalized_assets,
};
use bitblock::consensus_1::ConsensusClusterHarnessV1;
use bitblock::identity_d1::ActorIdV1;
use bitblock::multi_1::{
    CanonicalMultiActorFixtureV1, MULTI_1_VERSION, MultiActorOpKindV1, MultiActorStreamBundleV1,
    SignedActorTxEnvelopeV1,
};
use bitblock::sequencer_1::ReceiptMerkleProofV1;
use bitblock::world::{BEDROCK_COORD, Coord};

#[test]
fn wave24_six_state_lifecycle_state_proofs_and_epoch_transition() {
    let mut cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().expect("bootstrap");
    let mut client = ClientStateTrackerV1::from_cluster(&cluster).expect("client");
    let fork = cluster.fixture.fork_context;
    let pre_wr = cluster.nodes[0].harness().snapshot().unwrap().world_root;
    let bundle = cluster
        .fixture
        .canonical_mixed_race_bundle(1, pre_wr)
        .expect("b1");

    // 1. Submit lower-tip tx for Actor A nonce 0, then replace with bundle's higher-tip tx (tip 500)
    let low_tip_tx = SignedActorTxEnvelopeV1 {
        envelope_version: MULTI_1_VERSION,
        chain_id: fork.chain_id,
        principal_actor_id: cluster.fixture.actor_a,
        signer_actor_id: cluster.fixture.actor_a,
        nonce: 0,
        priority_tip: 200,
        expected_world_root: pre_wr,
        op_kind: MultiActorOpKindV1::PlaceVoxel,
        coord: Coord::new(3, 0, 0),
        target_actor_id: ActorIdV1::from_bytes([0u8; 32]),
        amount: 0,
        signer_pubkey: [0u8; 32],
        signature: [0u8; 64],
    }
    .sign_with_seed(&CanonicalMultiActorFixtureV1::ACTOR_A_SEED, &fork)
    .unwrap();
    let td_low = client.submit_tx(&low_tip_tx, 10).expect("submit low");
    assert_eq!(
        client.tx_record(&td_low).unwrap().status,
        ClientTxStatusV1::Submitted
    );
    client.mark_admitted(td_low).expect("admit low");
    assert_eq!(
        client.tx_record(&td_low).unwrap().status,
        ClientTxStatusV1::Admitted
    );

    // Replace with bundle's Actor A tip-500 MintEpisode tx -> low_tip_tx transitions to Replaced!
    let tx_a = &bundle.envelopes[1];
    let td_a = client.submit_tx(tx_a, 10).expect("replace with high tip");
    assert_eq!(
        client.tx_record(&td_low).unwrap().status,
        ClientTxStatusV1::Replaced
    );
    assert_eq!(
        client.tx_record(&td_low).unwrap().replaced_by_tx_digest,
        td_a
    );
    client.mark_admitted(td_a).expect("admit high");

    // Also track Actor B's race-loser tx (will be Rejected by finalized execution disposition!)
    let td_b_loser = client
        .submit_tx(&bundle.envelopes[0], 10)
        .expect("submit race loser");

    // 2. Finalize Height 1 (Epoch 0, 4 validators)
    let cert_h1 = cluster
        .run_round_with_subset(&bundle, 0, &[0, 1, 2, 3])
        .expect("h1");
    let rcpt_h1 = cluster.nodes[0].finalized_receipt_at(1).unwrap().clone();
    let coords = [Coord::new(3, 0, 0), Coord::new(4, 0, 0)];
    let (bals_h1, eps_h1) = snapshot_cluster_finalized_assets(&cluster, &coords);

    // Observe PrepareQC-only candidate first -> IncludedProvisional (NOT Finalized!)
    client
        .observe_provisional_candidate(
            &cert_h1.proposal,
            &cert_h1.prepare_qc,
            Some(&bundle),
            Some(&rcpt_h1),
            bals_h1.clone(),
            eps_h1.clone(),
        )
        .expect("provisional");
    assert_eq!(
        client.tx_record(&td_a).unwrap().status,
        ClientTxStatusV1::IncludedProvisional
    );
    assert_eq!(
        client.tx_record(&td_a).unwrap().root_status,
        ClientRootStatusV1::ProvisionalCandidate
    );
    // Provisional PrepareQC MUST be refused as finality
    assert!(matches!(
        client.require_commit_qc_not_prepare_only(&cert_h1.prepare_qc),
        Err(Client1Error::ProvisionalNotFinalized(_))
    ));

    // Apply CommitQC finalized commit -> Finalized + Rejected + Replaced all verified!
    let rolled_back = client
        .apply_finalized_commit(
            &cert_h1,
            Some(&bundle),
            Some(&rcpt_h1),
            bals_h1.clone(),
            eps_h1.clone(),
            &[0, 1],
        )
        .expect("finalize h1");
    assert!(!rolled_back);
    assert_eq!(
        client.tx_record(&td_a).unwrap().status,
        ClientTxStatusV1::Finalized
    );
    assert_eq!(
        client.tx_record(&td_a).unwrap().root_status,
        ClientRootStatusV1::FinalizedCommitQc
    );
    assert_eq!(
        client.tx_record(&td_b_loser).unwrap().status,
        ClientTxStatusV1::Rejected
    );
    assert_eq!(
        client.tx_record(&td_low).unwrap().status,
        ClientTxStatusV1::Replaced
    );
    assert!(client.provisional_branch().is_none());

    // 3. Verify ReceiptMerkleProofV1 AND DomainPatriciaProofV1 (NativeBalance + EpisodeOwnership)
    let rproof = ReceiptMerkleProofV1::build(&rcpt_h1.step_outcomes, 0).unwrap();
    client
        .verify_finalized_receipt_proof(&rcpt_h1.step_outcomes[0], &rproof)
        .expect("receipt proof verifies against finalized header");

    let mut bal_trie = DomainPatriciaTrieV1::empty(DomainIdV1::NativeBalance);
    for (aid, bal) in &bals_h1 {
        bal_trie.upsert(*aid.as_bytes(), bal.to_be_bytes().to_vec());
    }
    let bal_key = *cluster.fixture.actor_a.as_bytes();
    let bal_proof = bal_trie.prove(&bal_key).unwrap();
    let proven_bal_bytes = client
        .verify_finalized_state_proof(
            DomainIdV1::NativeBalance,
            bal_trie.root(),
            &bal_key,
            &bal_proof,
        )
        .unwrap()
        .expect("actor_a balance present");
    assert_eq!(
        u128::from_be_bytes(proven_bal_bytes.try_into().unwrap()),
        client.finalized_balance_of(&cluster.fixture.actor_a)
    );

    let mut ep_trie = DomainPatriciaTrieV1::empty(DomainIdV1::EpisodeOwnership);
    for (&coord, ep) in &eps_h1 {
        ep_trie.upsert(episode_trie_key(coord), ep.encode_canonical().to_vec());
    }
    let c3 = Coord::new(3, 0, 0);
    let ep_key = episode_trie_key(c3);
    let ep_proof = ep_trie.prove(&ep_key).unwrap();
    let proven_ep_bytes = client
        .verify_finalized_state_proof(
            DomainIdV1::EpisodeOwnership,
            ep_trie.root(),
            &ep_key,
            &ep_proof,
        )
        .unwrap()
        .expect("coord (3,0,0) episode present");
    assert_eq!(
        EpisodeOwnershipValueV1::decode_canonical(&proven_ep_bytes).unwrap(),
        client.finalized_episode_at(c3).unwrap()
    );

    // 4. Epoch validator-set transition (4 -> 5 validators at epoch 2)
    let sched_ep2 = cluster
        .activate_fifth_validator_for_epoch_2()
        .expect("ep2 5-val schedule");
    client
        .register_epoch_schedule(sched_ep2)
        .expect("client registers ep2 schedule");
    let b2 = MultiActorStreamBundleV1 {
        bundle_version: MULTI_1_VERSION,
        batch_height: 2,
        epoch: 2,
        fork_context: fork,
        base_fee_per_gas: 2,
        envelopes: Vec::new(),
    };
    let cert_h2 = cluster
        .run_round_with_subset(&b2, 0, &[0, 1, 2, 3, 4])
        .expect("h2 5-val finality");
    let rcpt_h2 = cluster.nodes[0].finalized_receipt_at(2).unwrap().clone();
    client
        .apply_finalized_commit(&cert_h2, Some(&b2), Some(&rcpt_h2), bals_h1, eps_h1, &[])
        .expect("client finalizes across epoch 0 -> epoch 2 validator set change");

    let view = client.verified_state_view();
    assert_eq!(view.finalized_height, 2);
    assert_eq!(view.finalized_epoch, 2);
    assert_eq!(view.finalized_tx_count, 1);
    assert_eq!(view.rejected_tx_count, 1);
    assert_eq!(view.replaced_tx_count, 1);
    assert_eq!(
        ClientVerifiedStateViewV1::decode_canonical(&view.encode_canonical().unwrap()).unwrap(),
        view
    );
    let lr = client.tx_lifecycle_receipt(td_a).unwrap();
    assert!(!lr.phantom_ownership_retained && !lr.phantom_supply_retained);
    assert_eq!(
        ClientTxLifecycleReceiptV1::decode_canonical(&lr.encode_canonical().unwrap()).unwrap(),
        lr
    );
    assert_eq!(
        ClientTxStatusV1::from_code(4).unwrap(),
        ClientTxStatusV1::Finalized
    );
    assert_eq!(
        ClientRootStatusV1::from_code(3).unwrap(),
        ClientRootStatusV1::FinalizedCommitQc
    );
}

#[test]
fn wave24_eight_reorg_phantom_ownership_and_trust_controls() {
    let base_cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().expect("bootstrap");
    let fork = base_cluster.fixture.fork_context;
    let pre_wr = base_cluster.nodes[0]
        .harness()
        .snapshot()
        .unwrap()
        .world_root;
    let b1 = base_cluster
        .fixture
        .canonical_mixed_race_bundle(1, pre_wr)
        .expect("b1");

    // Build Branch A (mints episodes at (3,0,0) and (4,0,0) and mutates balances)
    let mut cluster_a = base_cluster.clone();
    let cert_a = cluster_a
        .run_round_with_subset(&b1, 0, &[0, 1, 2, 3])
        .unwrap();
    let rcpt_a = cluster_a.nodes[0].finalized_receipt_at(1).unwrap().clone();
    let coords = [Coord::new(3, 0, 0), Coord::new(4, 0, 0)];
    let (bals_a, eps_a) = snapshot_cluster_finalized_assets(&cluster_a, &coords);

    // Build Competing Branch B (empty finalized block at view 1 -> zero minted episodes!)
    let mut cluster_b = base_cluster.clone();
    let empty_b1 = MultiActorStreamBundleV1 {
        bundle_version: MULTI_1_VERSION,
        batch_height: 1,
        epoch: 0,
        fork_context: fork,
        base_fee_per_gas: 2,
        envelopes: Vec::new(),
    };
    let cert_b = cluster_b
        .run_round_with_subset(&empty_b1, 1, &[0, 1, 2, 3])
        .unwrap();
    let rcpt_b = cluster_b.nodes[0].finalized_receipt_at(1).unwrap().clone();
    let (bals_b, eps_b) = snapshot_cluster_finalized_assets(&cluster_b, &coords);

    // 1 & 6. Competing candidates + Pre-finality fork mint/burn/ownership rollback -> ZERO phantom ownership/supply!
    let mut client = ClientStateTrackerV1::from_cluster(&base_cluster).unwrap();
    let initial_a_bal = client.finalized_balance_of(&base_cluster.fixture.actor_a);
    let td_a = client.submit_tx(&b1.envelopes[1], 10).unwrap();
    client
        .observe_provisional_candidate(
            &cert_a.proposal,
            &cert_a.prepare_qc,
            Some(&b1),
            Some(&rcpt_a),
            bals_a.clone(),
            eps_a.clone(),
        )
        .unwrap();
    // Provisional view shows Branch A's balance delta, while finalized truth stays untouched!
    assert_ne!(
        client.provisional_or_finalized_balance_of(&base_cluster.fixture.actor_a),
        initial_a_bal
    );
    assert_eq!(
        client.finalized_balance_of(&base_cluster.fixture.actor_a),
        initial_a_bal
    );
    assert!(client.finalized_episode_at(Coord::new(3, 0, 0)).is_none());

    // Competing Branch B finalizes -> atomic rollback of Branch A's provisional mints & balances!
    let rolled_back = client
        .apply_finalized_commit(
            &cert_b,
            Some(&empty_b1),
            Some(&rcpt_b),
            bals_b.clone(),
            eps_b.clone(),
            &[],
        )
        .unwrap();
    assert!(rolled_back);
    assert!(client.provisional_branch().is_none());
    assert_eq!(
        client.tx_record(&td_a).unwrap().status,
        ClientTxStatusV1::Rejected
    );
    assert_eq!(
        client.finalized_balance_of(&base_cluster.fixture.actor_a),
        initial_a_bal
    );
    assert_eq!(
        client.provisional_or_finalized_balance_of(&base_cluster.fixture.actor_a),
        initial_a_bal
    );
    assert!(client.finalized_episode_at(Coord::new(3, 0, 0)).is_none());
    let view = client.verified_state_view();
    assert_eq!(view.finalized_episode_count, 0);
    assert_eq!(view.finalized_total_locked_deposit, 0);
    assert_eq!(view.finalized_total_liquid_balance, 300_000_000_000);

    // 2 & 7. Delayed inclusion, expiry at valid_until_height, and clean resubmission without double-counting
    let mut c_exp = ClientStateTrackerV1::from_cluster(&base_cluster).unwrap();
    let exp_tx = SignedActorTxEnvelopeV1 {
        envelope_version: MULTI_1_VERSION,
        chain_id: fork.chain_id,
        principal_actor_id: base_cluster.fixture.actor_c,
        signer_actor_id: base_cluster.fixture.actor_c,
        nonce: 0,
        priority_tip: 50,
        expected_world_root: pre_wr,
        op_kind: MultiActorOpKindV1::TransferNative,
        coord: BEDROCK_COORD,
        target_actor_id: base_cluster.fixture.actor_b,
        amount: 5_000_000,
        signer_pubkey: [0u8; 32],
        signature: [0u8; 64],
    }
    .sign_with_seed(&CanonicalMultiActorFixtureV1::ACTOR_C_SEED, &fork)
    .unwrap();
    let td_exp = c_exp.submit_tx(&exp_tx, 1).unwrap(); // expires at height 1!
    // Block 1 (empty) finalizes without including exp_tx -> expires cleanly to Rejected!
    c_exp
        .apply_finalized_commit(
            &cert_b,
            Some(&empty_b1),
            Some(&rcpt_b),
            bals_b.clone(),
            eps_b.clone(),
            &[],
        )
        .unwrap();
    assert_eq!(
        c_exp.tx_record(&td_exp).unwrap().status,
        ClientTxStatusV1::Rejected
    );
    // Clean resubmission with fresh tip & valid_until_height=5 succeeds without double-counting!
    let resub_tx = SignedActorTxEnvelopeV1 {
        priority_tip: 60,
        ..exp_tx
    }
    .sign_with_seed(&CanonicalMultiActorFixtureV1::ACTOR_C_SEED, &fork)
    .unwrap();
    let td_resub = c_exp.submit_tx(&resub_tx, 5).expect("clean resubmission");
    assert_eq!(
        c_exp.tx_record(&td_resub).unwrap().status,
        ClientTxStatusV1::Submitted
    );
    assert!(matches!(
        c_exp.submit_tx(&exp_tx, 1),
        Err(Client1Error::TransactionExpired)
    ));

    // 3. Duplicate tx & lower-tip replacement refused fail-closed
    assert!(matches!(
        c_exp.submit_tx(&resub_tx, 5),
        Err(Client1Error::DuplicateOrLowerTipReplacement(_))
    ));
    let lower_tx = SignedActorTxEnvelopeV1 {
        priority_tip: 40,
        ..exp_tx
    }
    .sign_with_seed(&CanonicalMultiActorFixtureV1::ACTOR_C_SEED, &fork)
    .unwrap();
    assert!(matches!(
        c_exp.submit_tx(&lower_tx, 5),
        Err(Client1Error::DuplicateOrLowerTipReplacement(_))
    ));

    // 4. Wrong-root state proof & wrong-root receipt proof refused fail-closed
    let mut bal_trie = DomainPatriciaTrieV1::empty(DomainIdV1::NativeBalance);
    for (aid, bal) in &bals_a {
        bal_trie.upsert(*aid.as_bytes(), bal.to_be_bytes().to_vec());
    }
    let bal_key = *base_cluster.fixture.actor_a.as_bytes();
    let bal_proof = bal_trie.prove(&bal_key).unwrap();
    assert!(matches!(
        client.verify_finalized_state_proof(
            DomainIdV1::NativeBalance,
            [0xAA; 32], // wrong root!
            &bal_key,
            &bal_proof,
        ),
        Err(Client1Error::WrongRootStateProof(_))
    ));
    let rproof_a = ReceiptMerkleProofV1::build(&rcpt_a.step_outcomes, 0).unwrap();
    assert!(matches!(
        client.verify_finalized_receipt_proof(&rcpt_a.step_outcomes[0], &rproof_a),
        Err(Client1Error::WrongRootStateProof(_))
    ));

    // 5. Missing body refused fail-closed on BOTH provisional & finalized paths
    let mut c_miss = ClientStateTrackerV1::from_cluster(&base_cluster).unwrap();
    assert!(matches!(
        c_miss.observe_provisional_candidate(
            &cert_a.proposal,
            &cert_a.prepare_qc,
            None,
            None,
            BTreeMap::new(),
            BTreeMap::new(),
        ),
        Err(Client1Error::MissingBodyRefused)
    ));
    assert!(matches!(
        c_miss.apply_finalized_commit(&cert_a, None, None, BTreeMap::new(), BTreeMap::new(), &[]),
        Err(Client1Error::MissingBodyRefused)
    ));

    // 8. Stale weak-subjectivity checkpoint (> 28 epochs old) refused fail-closed!
    assert!(matches!(
        client
            .trust_anchor()
            .check_checkpoint_freshness(MAX_WEAK_SUBJECTIVITY_AGE_EPOCHS + 1),
        Err(Client1Error::StaleWeakSubjectivityCheckpoint { .. })
    ));
    let _ = compute_finalized_state_summary_digest(&bals_b, &eps_b);
}
