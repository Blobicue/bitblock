//! Release multi-actor permutation & race benchmark probe for Wave 19 (`MULTI-1`, `PS-032`).

use std::time::Instant;

use bitblock::multi_1::{CanonicalMultiActorFixtureV1, verify_permutation_invariance};

fn main() {
    let fix = CanonicalMultiActorFixtureV1::build().unwrap();
    let base_harness = fix.into_harness().unwrap();
    let pre_wr = base_harness.snapshot().unwrap().world_root;
    let bundle = fix.canonical_mixed_race_bundle(1, pre_wr).unwrap();

    // Build all 24 (4!) permutations of first 4 envelopes
    let mut perms = Vec::with_capacity(24);
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
                let mut pb = bundle.clone();
                pb.envelopes = vec![
                    bundle.envelopes[a],
                    bundle.envelopes[b],
                    bundle.envelopes[c],
                    bundle.envelopes[d],
                    bundle.envelopes[4],
                    bundle.envelopes[5],
                    bundle.envelopes[6],
                    bundle.envelopes[7],
                ];
                perms.push(pb);
            }
        }
    }

    let res = verify_permutation_invariance(&base_harness, &perms).unwrap();

    let mut t_sort = Vec::with_capacity(100);
    let mut t_fold = Vec::with_capacity(50);
    for _ in 0..100 {
        let a = Instant::now();
        let _ = bundle.canonical_sorted_envelopes().unwrap();
        t_sort.push(a.elapsed().as_secs_f64() * 1e6);
    }
    for _ in 0..50 {
        let mut h = base_harness.clone();
        let b = Instant::now();
        let _ = h.fold_stream_bundle(&bundle).unwrap();
        t_fold.push(b.elapsed().as_secs_f64() * 1e6);
    }
    let p_start = Instant::now();
    let _ = verify_permutation_invariance(&base_harness, &perms).unwrap();
    let perm_us = p_start.elapsed().as_secs_f64() * 1e6;

    t_sort.sort_by(f64::total_cmp);
    t_fold.sort_by(f64::total_cmp);

    println!("BUNDLE_TAG={}", bundle.tag().unwrap());
    println!("RCPT_TAG={}", res.fold_receipt.tag().unwrap());
    println!("COMP_TAG={}", res.post_composition.tag().unwrap());
    println!("SORT_P50_US={:.2}", t_sort[50]);
    println!("FOLD_P50_US={:.2}", t_fold[25]);
    println!("PERM24_US={:.2}", perm_us);
}
