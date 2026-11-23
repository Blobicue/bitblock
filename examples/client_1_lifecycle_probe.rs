//! Release benchmark probe for `CLIENT-1` (`PS-057`) producing merged-head timing metrics for
//! `client_1_finality_reorg_chart.png`:
//! - Finalized `CommitQC` + `ReceiptMerkleProofV1` + state-proof verification,
//! - Pre-finality `PrepareQC` fork/reorg mint/burn/ownership atomic rollback,
//! - Canonical `ClientVerifiedStateViewV1` + `ClientTxLifecycleReceiptV1` emission.

use std::collections::BTreeMap;
use std::time::Instant;

use bitblock::asset_1::EpisodeOwnershipValueV1;
use bitblock::auth_commit_1::{DomainIdV1, DomainPatriciaTrieV1};
use bitblock::client_1::{ClientStateTrackerV1, snapshot_cluster_finalized_assets};
use bitblock::consensus_1::ConsensusClusterHarnessV1;
use bitblock::multi_1::MultiActorStreamBundleV1;
use bitblock::sequencer_1::ReceiptMerkleProofV1;
use bitblock::world::Coord;

fn main() {
    let base_cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().expect("bootstrap");
    let pre_wr = base_cluster.nodes[0]
        .harness()
        .snapshot()
        .expect("snap")
        .world_root;
    let bundle = base_cluster
        .fixture
        .canonical_mixed_race_bundle(1, pre_wr)
        .expect("bundle");

    // Build finalized Branch A (winning CommitQC)
    let mut cluster_a = base_cluster.clone();
    let cert_a = cluster_a
        .run_round_with_subset(&bundle, 0, &[0, 1, 2, 3])
        .expect("round a");
    let rcpt_a = cluster_a.nodes[0]
        .finalized_receipt_at(1)
        .expect("rcpt a")
        .clone();
    let coords = [Coord::new(3, 0, 0), Coord::new(4, 0, 0)];
    let (bals_a, eps_a) = snapshot_cluster_finalized_assets(&cluster_a, &coords);

    // Build competing provisional Branch B (PrepareQC-only empty bundle at view 1)
    let mut cluster_b = base_cluster.clone();
    let empty_b = MultiActorStreamBundleV1 {
        bundle_version: bundle.bundle_version,
        batch_height: 1,
        epoch: 0,
        fork_context: base_cluster.fixture.fork_context,
        base_fee_per_gas: 2,
        envelopes: Vec::new(),
    };
    let cert_b = cluster_b
        .run_round_with_subset(&empty_b, 1, &[0, 1, 2, 3])
        .expect("round b");
    let rcpt_b = cluster_b.nodes[0]
        .finalized_receipt_at(1)
        .expect("rcpt b")
        .clone();

    // Build real DomainPatriciaProofV1 over NativeBalance domain
    let mut bal_trie = DomainPatriciaTrieV1::empty(DomainIdV1::NativeBalance);
    for (aid, bal) in &bals_a {
        bal_trie.upsert(*aid.as_bytes(), bal.to_be_bytes().to_vec());
    }
    let bal_root = bal_trie.root();
    let bal_key = *base_cluster.fixture.actor_a.as_bytes();
    let bal_proof = bal_trie.prove(&bal_key).expect("prove");
    let rcpt_proof = ReceiptMerkleProofV1::build(&rcpt_a.step_outcomes, 0).expect("rproof");

    // 1. Finalized CommitQC + ReceiptMerkleProofV1 + DomainPatriciaProofV1 verification
    let iters = 100u32;
    let t0 = Instant::now();
    for _ in 0..iters {
        let mut c = ClientStateTrackerV1::from_cluster(&base_cluster).expect("cli");
        let _ = c.submit_tx(&bundle.envelopes[1], 10).expect("sub");
        c.apply_finalized_commit(
            &cert_a,
            Some(&bundle),
            Some(&rcpt_a),
            bals_a.clone(),
            eps_a.clone(),
            &[0],
        )
        .expect("fin");
        c.verify_finalized_receipt_proof(&rcpt_a.step_outcomes[0], &rcpt_proof)
            .expect("rproof");
        let _ = c
            .verify_finalized_state_proof(DomainIdV1::NativeBalance, bal_root, &bal_key, &bal_proof)
            .expect("sproof");
    }
    let fin_us = t0.elapsed().as_secs_f64() * 1e6 / f64::from(iters);

    // 2. Provisional PrepareQC -> Competing Finalized CommitQC atomic reorg rollback
    let mut prov_eps = BTreeMap::new();
    prov_eps.insert(
        Coord::new(9, 9, 9),
        EpisodeOwnershipValueV1 {
            owner: base_cluster.fixture.actor_b,
            mint_episode_id: [0xEE; 32],
            locked_deposit: 1_000_000,
        },
    );
    let t1 = Instant::now();
    for _ in 0..iters {
        let mut c = ClientStateTrackerV1::from_cluster(&base_cluster).expect("cli");
        let _ = c.submit_tx(&bundle.envelopes[1], 10).expect("sub");
        c.observe_provisional_candidate(
            &cert_a.proposal,
            &cert_a.prepare_qc,
            Some(&bundle),
            Some(&rcpt_a),
            bals_a.clone(),
            prov_eps.clone(),
        )
        .expect("prov");
        let rb = c
            .apply_finalized_commit(
                &cert_b,
                Some(&empty_b),
                Some(&rcpt_b),
                BTreeMap::new(),
                BTreeMap::new(),
                &[],
            )
            .expect("reorg");
        assert!(rb);
    }
    let reorg_us = t1.elapsed().as_secs_f64() * 1e6 / f64::from(iters);

    // 3. Verified state view + lifecycle receipt canonical encoding & tagging
    let mut c_ref = ClientStateTrackerV1::from_cluster(&base_cluster).expect("cli");
    let td = c_ref.submit_tx(&bundle.envelopes[1], 10).expect("sub");
    c_ref
        .apply_finalized_commit(&cert_a, Some(&bundle), Some(&rcpt_a), bals_a, eps_a, &[0])
        .expect("fin");
    let t2 = Instant::now();
    for _ in 0..iters {
        let v = c_ref.verified_state_view();
        let lr = c_ref.tx_lifecycle_receipt(td).expect("lr");
        std::hint::black_box((v.tag().expect("vt"), lr.tag().expect("lt")));
    }
    let proof_us = t2.elapsed().as_secs_f64() * 1e6 / f64::from(iters);

    println!("CLIENT_1_FIN_US={fin_us:.2}");
    println!("CLIENT_1_REORG_US={reorg_us:.2}");
    println!("CLIENT_1_PROOF_US={proof_us:.2}");
    println!(
        "CLIENT_1_VIEW_TAG={}",
        c_ref.verified_state_view().tag().expect("vt")
    );
    println!(
        "CLIENT_1_LRCPT_TAG={}",
        c_ref
            .tx_lifecycle_receipt(td)
            .expect("lr")
            .tag()
            .expect("lt")
    );
}
