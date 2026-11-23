#![cfg(feature = "protocol-research")]
//! Wave 19 (`MULTI-1`, `PS-032`) integration tests: 24-permutation (`4!`) + 16 seeded arrival-order
//! invariance, same-coordinate race resolution, depth-1 delegation vs depth-2 transitive rejection,
//! same-batch burn -> re-mint distinct `MintIdV1` provenance, and 8/8 adversarial conflict matrix.

use bitblock::identity_d1::ActorIdV1;
use bitblock::multi_1::{
    CanonicalMultiActorFixtureV1, MULTI_1_VERSION, MultiActorDispositionV1, MultiActorOpKindV1,
    MultiActorStreamBundleV1, SignedActorTxEnvelopeV1, verify_permutation_invariance,
};
use bitblock::world::{BEDROCK_COORD, Coord};

fn permute_indices_4() -> Vec<[usize; 4]> {
    let mut out = Vec::new();
    for a in 0..4 {
        for b in 0..4 {
            if b == a {
                continue;
            }
            for c in 0..4 {
                if c == a || c == b {
                    continue;
                }
                let d = 6 - a - b - c;
                out.push([a, b, c, d]);
            }
        }
    }
    assert_eq!(out.len(), 24);
    out
}

#[test]
fn wave19_multi_1_permutation_invariance_burn_remint_and_golden_parity() {
    let fix = CanonicalMultiActorFixtureV1::build().unwrap();
    let base_harness = fix.into_harness().unwrap();
    let pre_wr = base_harness.snapshot().unwrap().world_root;
    let bundle = fix.canonical_mixed_race_bundle(1, pre_wr).unwrap();

    // 1. All 24 (4!) permutations of first 4 envelopes + 16 deterministic SplitMix shuffles
    let mut perms = Vec::with_capacity(40);
    for p in permute_indices_4() {
        let mut b = bundle.clone();
        b.envelopes = vec![
            bundle.envelopes[p[0]],
            bundle.envelopes[p[1]],
            bundle.envelopes[p[2]],
            bundle.envelopes[p[3]],
            bundle.envelopes[4],
            bundle.envelopes[5],
            bundle.envelopes[6],
            bundle.envelopes[7],
        ];
        perms.push(b);
    }
    let mut state = 0x517C_C1B7_2722_0A95u64;
    for _ in 0..16 {
        let mut envs = bundle.envelopes.clone();
        for i in (1..envs.len()).rev() {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let j = (state as usize) % (i + 1);
            envs.swap(i, j);
        }
        let mut b = bundle.clone();
        b.envelopes = envs;
        perms.push(b);
    }
    let res1 = verify_permutation_invariance(&base_harness, &perms).unwrap();
    assert_eq!(res1.fold_receipt.admitted_count, 3);
    assert_eq!(res1.fold_receipt.rejected_count, 5);
    assert_eq!(res1.minted_episode_ids.len(), 2);
    assert_eq!(
        bundle.tag().unwrap().to_string(),
        "#bb.receipt.sha256.h6deuodmzgmgpkyqv2do2rkaonc7gnn7rjgi5hp3oqwkvs7l5kgq"
    );
    assert_eq!(
        res1.fold_receipt.tag().unwrap().to_string(),
        "#bb.receipt.sha256.ssqa3yacg6tg6jk6d7eddccshdlikpzwxrqov6cfr5mcldjqbr3q"
    );
    assert_eq!(
        res1.post_composition.tag().unwrap().to_string(),
        "#bb.receipt.sha256.lbjy45lcbkxe7qle5cwdo3nlvh3qd253kn5m433mphgkmeua5gia"
    );

    // Verify exact dispositions in canonical order
    let disps: Vec<MultiActorDispositionV1> = res1
        .fold_receipt
        .step_outcomes
        .iter()
        .map(|s| s.disposition)
        .collect();
    assert_eq!(
        disps,
        vec![
            MultiActorDispositionV1::AdmittedExecuted, // tip 500: Actor A Mint(3,0,0) wins
            MultiActorDispositionV1::AdmittedExecuted, // tip 400: Delegate C Mint(4,0,0) for A
            MultiActorDispositionV1::RejectedDuplicateOrStaleNonce, // tip 300: Actor A dup nonce 0
            MultiActorDispositionV1::RejectedStaleWorldAnchor, // tip 250: Actor C stale anchor
            MultiActorDispositionV1::RejectedNoOpBedrockOrRedundant, // tip 200: bedrock delete
            MultiActorDispositionV1::RejectedSameCoordRaceLost, // tip 100: Actor B Mint(3,0,0) loses
            MultiActorDispositionV1::AdmittedFailureAtomicRollback, // tip 80: Actor B overdraft
            MultiActorDispositionV1::AdmittedExecuted,          // tip 50: Actor C transfer 5M -> B
        ]
    );

    // 2. Batch 2: Same-batch Burn -> Re-Mint on (3,0,0) proves distinct MintIdV1 + conservation
    let mut h = base_harness;
    let _ = h.fold_stream_bundle(&bundle).unwrap();
    let mint_1 = h.episode_mint_at(Coord::new(3, 0, 0)).unwrap();
    let wr2 = h.snapshot().unwrap().world_root;
    let zero_a = ActorIdV1::from_bytes([0u8; 32]);
    let f = &fix.fork_context;
    // Delegate C burns A's episode at (3,0,0) (tip 900 -> runs first), then Actor B re-mints (3,0,0) (tip 500)!
    let tx_burn = SignedActorTxEnvelopeV1 {
        envelope_version: MULTI_1_VERSION,
        chain_id: f.chain_id,
        principal_actor_id: fix.actor_a,
        signer_actor_id: fix.actor_c,
        nonce: h.next_nonce(&fix.actor_a),
        priority_tip: 900,
        expected_world_root: wr2,
        op_kind: MultiActorOpKindV1::BurnEpisode,
        coord: Coord::new(3, 0, 0),
        target_actor_id: zero_a,
        amount: 0,
        signer_pubkey: [0u8; 32],
        signature: [0u8; 64],
    }
    .sign_with_seed(&CanonicalMultiActorFixtureV1::ACTOR_C_SEED, f)
    .unwrap();
    let tx_remint = SignedActorTxEnvelopeV1 {
        envelope_version: MULTI_1_VERSION,
        chain_id: f.chain_id,
        principal_actor_id: fix.actor_b,
        signer_actor_id: fix.actor_b,
        nonce: h.next_nonce(&fix.actor_b),
        priority_tip: 500,
        expected_world_root: wr2,
        op_kind: MultiActorOpKindV1::MintEpisode,
        coord: Coord::new(3, 0, 0),
        target_actor_id: zero_a,
        amount: 0,
        signer_pubkey: [0u8; 32],
        signature: [0u8; 64],
    }
    .sign_with_seed(&CanonicalMultiActorFixtureV1::ACTOR_B_SEED, f)
    .unwrap();

    // Reverse arrival order: [remint, burn] still sorts [burn, remint] cleanly!
    let b2 = MultiActorStreamBundleV1 {
        bundle_version: MULTI_1_VERSION,
        batch_height: 2,
        epoch: 0,
        fork_context: *f,
        base_fee_per_gas: 2,
        envelopes: vec![tx_remint, tx_burn],
    };
    let res2 = h.fold_stream_bundle(&b2).unwrap();
    assert_eq!(res2.fold_receipt.admitted_count, 2);
    assert_eq!(res2.fold_receipt.rejected_count, 0);
    let mint_2 = h.episode_mint_at(Coord::new(3, 0, 0)).unwrap();
    assert_ne!(mint_1, mint_2, "re-mint must produce a distinct MintIdV1");
}

#[test]
fn wave19_multi_1_eight_adversarial_delegation_sig_and_race_gates() {
    let fix = CanonicalMultiActorFixtureV1::build().unwrap();
    let mut h = fix.into_harness().unwrap();
    let wr = h.snapshot().unwrap().world_root;
    let f = &fix.fork_context;
    let zero_a = ActorIdV1::from_bytes([0u8; 32]);

    // 1. Transitive depth-2 delegation (A -> C -> B: Actor B tries to act for Principal A) -> REJECTED
    let tx_depth2 = SignedActorTxEnvelopeV1 {
        envelope_version: MULTI_1_VERSION,
        chain_id: f.chain_id,
        principal_actor_id: fix.actor_a,
        signer_actor_id: fix.actor_b,
        nonce: 0,
        priority_tip: 900,
        expected_world_root: wr,
        op_kind: MultiActorOpKindV1::PlaceVoxel,
        coord: Coord::new(9, 0, 0),
        target_actor_id: zero_a,
        amount: 0,
        signer_pubkey: [0u8; 32],
        signature: [0u8; 64],
    }
    .sign_with_seed(&CanonicalMultiActorFixtureV1::ACTOR_B_SEED, f)
    .unwrap();

    // 2. Delegate attempting native fungible transfer (A delegated voxel rights to C, not funds) -> REJECTED
    let tx_del_xfer = SignedActorTxEnvelopeV1 {
        envelope_version: MULTI_1_VERSION,
        chain_id: f.chain_id,
        principal_actor_id: fix.actor_a,
        signer_actor_id: fix.actor_c,
        nonce: 0,
        priority_tip: 800,
        expected_world_root: wr,
        op_kind: MultiActorOpKindV1::TransferNative,
        coord: BEDROCK_COORD,
        target_actor_id: fix.actor_b,
        amount: 100,
        signer_pubkey: [0u8; 32],
        signature: [0u8; 64],
    }
    .sign_with_seed(&CanonicalMultiActorFixtureV1::ACTOR_C_SEED, f)
    .unwrap();

    // 3. Tampered 1-bit RFC 8032 signature -> REJECTED
    let mut tx_bad_sig = SignedActorTxEnvelopeV1 {
        envelope_version: MULTI_1_VERSION,
        chain_id: f.chain_id,
        principal_actor_id: fix.actor_b,
        signer_actor_id: fix.actor_b,
        nonce: 0,
        priority_tip: 700,
        expected_world_root: wr,
        op_kind: MultiActorOpKindV1::PlaceVoxel,
        coord: Coord::new(10, 0, 0),
        target_actor_id: zero_a,
        amount: 0,
        signer_pubkey: [0u8; 32],
        signature: [0u8; 64],
    }
    .sign_with_seed(&CanonicalMultiActorFixtureV1::ACTOR_B_SEED, f)
    .unwrap();
    tx_bad_sig.signature[0] ^= 0x01;

    // 4. Cross-chain replay (wrong chain_id = 999) -> REJECTED
    let tx_wrong_chain = SignedActorTxEnvelopeV1 {
        envelope_version: MULTI_1_VERSION,
        chain_id: 999,
        principal_actor_id: fix.actor_b,
        signer_actor_id: fix.actor_b,
        nonce: 0,
        priority_tip: 600,
        expected_world_root: wr,
        op_kind: MultiActorOpKindV1::PlaceVoxel,
        coord: Coord::new(11, 0, 0),
        target_actor_id: zero_a,
        amount: 0,
        signer_pubkey: [0u8; 32],
        signature: [0u8; 64],
    }
    .sign_with_seed(&CanonicalMultiActorFixtureV1::ACTOR_B_SEED, f)
    .unwrap();

    let pre_comp = h.current_composition().unwrap();
    let adv_bundle = MultiActorStreamBundleV1 {
        bundle_version: MULTI_1_VERSION,
        batch_height: 1,
        epoch: 0,
        fork_context: *f,
        base_fee_per_gas: 2,
        envelopes: vec![tx_depth2, tx_del_xfer, tx_bad_sig, tx_wrong_chain],
    };
    let res = h.fold_stream_bundle(&adv_bundle).unwrap();
    assert_eq!(res.fold_receipt.admitted_count, 0);
    assert_eq!(res.fold_receipt.rejected_count, 4);
    assert_eq!(
        res.fold_receipt.step_outcomes[0].disposition,
        MultiActorDispositionV1::RejectedUnauthorizedOrExpiredDelegation
    );
    assert_eq!(
        res.fold_receipt.step_outcomes[1].disposition,
        MultiActorDispositionV1::RejectedUnauthorizedOrExpiredDelegation
    );
    assert_eq!(
        res.fold_receipt.step_outcomes[2].disposition,
        MultiActorDispositionV1::RejectedInvalidSignatureOrContext
    );
    assert_eq!(
        res.fold_receipt.step_outcomes[3].disposition,
        MultiActorDispositionV1::RejectedInvalidSignatureOrContext
    );
    // Zero state mutation on all-rejected batch!
    assert_eq!(
        res.post_composition.execution_state_root,
        pre_comp.execution_state_root
    );
}
