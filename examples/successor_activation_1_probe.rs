use std::hint::black_box;
use std::time::Instant;

use bitblock::successor_activation_1::{
    SuccessorPreflightCapsV1, build_canonical_devnet_successor_activation,
    preflight_successor_activation_bundle_bytes, verify_successor_activation_carrier,
};

fn main() {
    let s3 = build_canonical_devnet_successor_activation(true).expect("s3 build");
    let s0 = build_canonical_devnet_successor_activation(false).expect("s0 build");
    let raw_bundle = s3.bundle.encode_canonical().expect("raw bundle");
    let caps = SuccessorPreflightCapsV1::V1_DEFAULT;

    // Warmup
    let _ = preflight_successor_activation_bundle_bytes(&raw_bundle, caps).expect("preflight");
    let _ = verify_successor_activation_carrier(
        &s3.carrier,
        &s3.base_context,
        &s3.release_parent_cert,
        &s3.activation_parent_cert,
        caps,
    )
    .expect("verify");

    let n_preflight = 10_000u32;
    let t0 = Instant::now();
    for _ in 0..n_preflight {
        let r = preflight_successor_activation_bundle_bytes(black_box(&raw_bundle), caps)
            .expect("preflight");
        black_box(r);
    }
    let preflight_us = t0.elapsed().as_secs_f64() * 1e6 / f64::from(n_preflight);

    let n_verify = 200u32;
    let t1 = Instant::now();
    for _ in 0..n_verify {
        let r = verify_successor_activation_carrier(
            black_box(&s3.carrier),
            black_box(&s3.base_context),
            black_box(&s3.release_parent_cert),
            black_box(&s3.activation_parent_cert),
            caps,
        )
        .expect("verify");
        black_box(r);
    }
    let verify_us = t1.elapsed().as_secs_f64() * 1e6 / f64::from(n_verify);

    let mut bad_bundle = raw_bundle.clone();
    bad_bundle[2 + 32 * 7 + 3] ^= 0x01;
    let n_reject = 10_000u32;
    let t2 = Instant::now();
    for _ in 0..n_reject {
        let r = preflight_successor_activation_bundle_bytes(black_box(&bad_bundle), caps);
        assert!(r.is_err());
        let _ = black_box(r);
    }
    let reject_us = t2.elapsed().as_secs_f64() * 1e6 / f64::from(n_reject);

    println!("SA1_PREFLIGHT_US={preflight_us:.2}");
    println!("SA1_VERIFY_US={verify_us:.2}");
    println!("SA1_MUTANT_REJECT_US={reject_us:.2}");
    println!("SA1_S3_BUNDLE_BYTES={}", raw_bundle.len());
    println!(
        "SA1_S3_PREIMAGE_BYTES={}",
        s3.bundle.internal_canonical_preimage_bytes_u64
    );
    println!(
        "SA1_S0_PREIMAGE_BYTES={}",
        s0.bundle.internal_canonical_preimage_bytes_u64
    );
    println!("SA1_BUNDLE_TAG={}", s3.bundle.tag().expect("b tag"));
    println!(
        "SA1_SIGN_DOC_TAG={}",
        s3.authorization.sign_doc.tag().expect("sd tag")
    );
    println!("SA1_AUTH_TAG={}", s3.authorization.tag().expect("auth tag"));
    println!("SA1_CARRIER_TAG={}", s3.carrier.tag().expect("car tag"));
    println!(
        "SA1_S0_CARRIER_TAG={}",
        s0.carrier.tag().expect("s0 car tag")
    );
}
