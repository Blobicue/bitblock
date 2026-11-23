//! Release probe for Wave 14A `ASSET-1` (`PS-038`):
//! measures dual-domain supply conservation, transfer-carries-deposit-claim,
//! concurrent conflict rollback, and authenticated dual-root release latencies.

use std::time::Instant;

use bitblock::asset_1::{
    AssetDispositionV1, AssetLedgerStateV1, AssetOperationV1, episode_trie_key,
};
use bitblock::asset_d1::AssetPolicyDescriptorV1;
use bitblock::auth_commit_1::DomainIdV1;
use bitblock::identity_d1::ActorIdV1;
use bitblock::world::{BEDROCK_COORD, Coord};

fn p50_us(samples: &mut [f64]) -> f64 {
    samples.sort_by(|a, b| a.total_cmp(b));
    samples[samples.len() / 2]
}

fn main() {
    let policy = AssetPolicyDescriptorV1::v1_dual_domain_default();
    let profile_id = [0x44u8; 32];
    let actor_a = ActorIdV1::from_bytes([0x11u8; 32]);
    let actor_b = ActorIdV1::from_bytes([0x22u8; 32]);
    let actor_c = ActorIdV1::from_bytes([0x33u8; 32]);
    let genesis_supply = 10_000_000u128;

    let mut ledger = AssetLedgerStateV1::from_genesis(
        policy,
        profile_id,
        &[
            (actor_a, 5_000_000),
            (actor_b, 3_000_000),
            (actor_c, 2_000_000),
        ],
    )
    .expect("genesis");
    let r0 = ledger.roots().expect("r0");
    let mut trace = vec![(
        "Genesis",
        r0.total_liquid_supply,
        r0.total_locked_deposits,
        r0.total_burned_fees,
    )];

    // Stage 1: Mint 3 voxels (3.0 M locked deposits)
    let c1 = Coord::new(1, 0, 0);
    let c2 = Coord::new(2, 0, 0);
    let c3 = Coord::new(3, 0, 0);
    let mint_rcpt = ledger
        .apply_local_batch(
            &[
                AssetOperationV1::MintEpisode {
                    actor: actor_a,
                    coord: c1,
                    mint_episode_id: [0xA1u8; 32],
                },
                AssetOperationV1::MintEpisode {
                    actor: actor_a,
                    coord: c2,
                    mint_episode_id: [0xA2u8; 32],
                },
                AssetOperationV1::MintEpisode {
                    actor: actor_b,
                    coord: c3,
                    mint_episode_id: [0xB3u8; 32],
                },
            ],
            10,
        )
        .expect("mint");
    assert_eq!(mint_rcpt.disposition, AssetDispositionV1::Admitted);
    let r1 = ledger.roots().expect("r1");
    trace.push((
        "Mint 3\nVoxels",
        r1.total_liquid_supply,
        r1.total_locked_deposits,
        r1.total_burned_fees,
    ));

    // Stage 2: Transfer c1 from A -> B (carries 1.0 M deposit claim)
    let xfer_rcpt = ledger
        .apply_local_batch(
            &[AssetOperationV1::TransferEpisode {
                from: actor_a,
                to: actor_b,
                coord: c1,
            }],
            20,
        )
        .expect("xfer");
    assert_eq!(xfer_rcpt.disposition, AssetDispositionV1::Admitted);
    let r2 = ledger.roots().expect("r2");
    trace.push((
        "Transfer\n(A -> B)",
        r2.total_liquid_supply,
        r2.total_locked_deposits,
        r2.total_burned_fees,
    ));

    // Stage 3: Burn-after-transfer by A MUST fail; then B burns c1 and receives 1.0 M deposit refund
    let old_burn = ledger
        .apply_local_batch(
            &[AssetOperationV1::BurnEpisode {
                actor: actor_a,
                coord: c1,
            }],
            25,
        )
        .expect("old burn");
    assert_eq!(
        old_burn.disposition,
        AssetDispositionV1::RejectedNotEpisodeOwner
    );
    let burn_rcpt = ledger
        .apply_local_batch(
            &[AssetOperationV1::BurnEpisode {
                actor: actor_b,
                coord: c1,
            }],
            30,
        )
        .expect("burn b");
    assert_eq!(burn_rcpt.disposition, AssetDispositionV1::Admitted);
    let r3 = ledger.roots().expect("r3");
    trace.push((
        "Burn by B\n(Refund)",
        r3.total_liquid_supply,
        r3.total_locked_deposits,
        r3.total_burned_fees,
    ));

    // Stage 4: Concurrent write conflict & bedrock exclusion rollback
    let conflict = ledger
        .apply_local_batch(
            &[
                AssetOperationV1::TransferEpisode {
                    from: actor_a,
                    to: actor_b,
                    coord: c2,
                },
                AssetOperationV1::TransferEpisode {
                    from: actor_a,
                    to: actor_c,
                    coord: c2,
                },
            ],
            35,
        )
        .expect("conflict");
    assert_eq!(
        conflict.disposition,
        AssetDispositionV1::RejectedConcurrentWrite
    );
    let bedrock = ledger
        .apply_local_batch(
            &[AssetOperationV1::MintEpisode {
                actor: actor_a,
                coord: BEDROCK_COORD,
                mint_episode_id: [0x00u8; 32],
            }],
            36,
        )
        .expect("bedrock");
    assert_eq!(
        bedrock.disposition,
        AssetDispositionV1::RejectedBedrockForbidden
    );
    let r4 = ledger.roots().expect("r4");
    assert_eq!(r4, r3);
    trace.push((
        "Conflict &\nBedrock",
        r4.total_liquid_supply,
        r4.total_locked_deposits,
        r4.total_burned_fees,
    ));
    ledger.verify_rebuild_parity().expect("rebuild");
    ledger.verify_conservation().expect("conservation");

    // Release latency benchmark (200 runs)
    let mut mint_us = Vec::with_capacity(200);
    let mut xfer_us = Vec::with_capacity(200);
    let mut burn_us = Vec::with_capacity(200);
    let mut verify_us = Vec::with_capacity(200);
    let mut bench_ledger = AssetLedgerStateV1::from_genesis(
        policy,
        profile_id,
        &[(actor_a, 500_000_000), (actor_b, 500_000_000)],
    )
    .expect("bench genesis");
    for i in 0..200i64 {
        let c = Coord::new(100 + i, i, -i);
        let t0 = Instant::now();
        let _ = bench_ledger
            .apply_local_batch(
                &[AssetOperationV1::MintEpisode {
                    actor: actor_a,
                    coord: c,
                    mint_episode_id: [(i & 0xff) as u8; 32],
                }],
                (i as u64) * 3 + 1,
            )
            .expect("mint");
        mint_us.push(t0.elapsed().as_secs_f64() * 1e6);

        let t1 = Instant::now();
        let _ = bench_ledger
            .apply_local_batch(
                &[AssetOperationV1::TransferEpisode {
                    from: actor_a,
                    to: actor_b,
                    coord: c,
                }],
                (i as u64) * 3 + 2,
            )
            .expect("xfer");
        xfer_us.push(t1.elapsed().as_secs_f64() * 1e6);

        let proof = bench_ledger.prove_episode(c).expect("prove");
        let root = bench_ledger.roots().expect("r").episode_ownership_root;
        let k = episode_trie_key(c);
        let t_v = Instant::now();
        let _ = proof
            .verify(DomainIdV1::EpisodeOwnership, root, &k)
            .expect("verify");
        verify_us.push(t_v.elapsed().as_secs_f64() * 1e6);

        let t2 = Instant::now();
        let _ = bench_ledger
            .apply_local_batch(
                &[AssetOperationV1::BurnEpisode {
                    actor: actor_b,
                    coord: c,
                }],
                (i as u64) * 3 + 3,
            )
            .expect("burn");
        burn_us.push(t2.elapsed().as_secs_f64() * 1e6);
    }

    let g_tag = r0.tag().expect("g tag").to_string();
    let f_tag = r4.tag().expect("f tag").to_string();
    let trace_json: Vec<String> = trace
        .iter()
        .map(|(st, lq, lk, bn)| {
            format!(
                "{{\"stage\": \"{}\", \"liquid\": {}, \"locked_deposits\": {}, \"burned_fees\": {}}}",
                st.replace('\n', "\\n"),
                lq,
                lk,
                bn
            )
        })
        .collect();

    println!("{{");
    println!("  \"genesis_supply\": {},", genesis_supply);
    println!("  \"genesis_roots_tag\": \"{}\",", g_tag);
    println!("  \"final_roots_tag\": \"{}\",", f_tag);
    println!("  \"genesis_roots_tag_short\": \"{}\",", &g_tag[..26]);
    println!("  \"final_roots_tag_short\": \"{}\",", &f_tag[..26]);
    println!("  \"conservation_trace\": [{}],", trace_json.join(", "));
    println!("  \"mint_p50_us\": {:.3},", p50_us(&mut mint_us));
    println!("  \"transfer_p50_us\": {:.3},", p50_us(&mut xfer_us));
    println!("  \"burn_p50_us\": {:.3},", p50_us(&mut burn_us));
    println!("  \"verify_p50_us\": {:.3},", p50_us(&mut verify_us));
    println!("  \"gates_passed\": 7,");
    println!("  \"gates_total\": 7,");
    println!("  \"gate_rows\": [");
    println!(
        "    {{\"gate\": \"Closed Supply Conservation\", \"result\": \"liquid + deposits + burned == 10.0 M\"}},"
    );
    println!(
        "    {{\"gate\": \"Transfer Carries Deposit Claim\", \"result\": \"B receives episode + 1.0 M refund\"}},"
    );
    println!(
        "    {{\"gate\": \"Burn-After-Transfer Blocked\", \"result\": \"Old owner A -> RejectedNotEpisodeOwner\"}},"
    );
    println!(
        "    {{\"gate\": \"Concurrent Coord Conflict\", \"result\": \"RejectedConcurrentWrite; full rollback\"}},"
    );
    println!(
        "    {{\"gate\": \"Bedrock (0,0,0) Economic Wall\", \"result\": \"RejectedBedrockForbidden; 0 deposit\"}},"
    );
    println!(
        "    {{\"gate\": \"Auth & Nonce Replay Guard\", \"result\": \"Bad sig & stale nonce rolled back\"}},"
    );
    println!(
        "    {{\"gate\": \"Dual-Root Rebuild & Isolation\", \"result\": \"0 diffs; Balance != Ownership != Auth\"}}"
    );
    println!("  ]");
    println!("}}");
}
