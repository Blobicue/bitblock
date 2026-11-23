//! Release benchmark & audit probe for Wave 30 (`CHAIN-LAUNCH-1@base`, `PS-091`).
//!
//! Run on merged `origin/main` with:
//! `cargo run --release --features protocol-research --example chain_launch_1_package_probe`

use std::time::Instant;

use bitblock::chain_launch_1::{
    LaunchPreflightCapsV1, build_canonical_devnet_chain_launch,
    preflight_launch_closure_bundle_bytes, verify_chain_launch_package,
};

fn median_us(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.total_cmp(b));
    samples[samples.len() / 2]
}

fn main() {
    let s3 = build_canonical_devnet_chain_launch(true).expect("S=3 launch build");
    let s0 = build_canonical_devnet_chain_launch(false).expect("S=0 launch build");
    let caps = LaunchPreflightCapsV1::V1_DEFAULT;
    let g_len = s3.package.genesis_package_bytes.len() as u64;

    // 1. Streaming zero-allocation preflight latency (median of 500 runs).
    let mut pf_samples = Vec::with_capacity(500);
    for _ in 0..500 {
        let t0 = Instant::now();
        let pf = preflight_launch_closure_bundle_bytes(
            &s3.package.launch_closure_bundle_bytes,
            g_len,
            caps,
        )
        .expect("preflight");
        std::hint::black_box(pf);
        pf_samples.push(t0.elapsed().as_secs_f64() * 1e6);
    }

    // 2. Full end-to-end S=3 verification latency (preflight + DAG + RELEASE-1 + 3-of-5 Ed25519, median of 150 runs).
    let mut ver_samples = Vec::with_capacity(150);
    for _ in 0..150 {
        let t0 = Instant::now();
        let out =
            verify_chain_launch_package(&s3.package, &s3.release_policy, &s3.launch_policy, caps)
                .expect("verify");
        std::hint::black_box(out);
        ver_samples.push(t0.elapsed().as_secs_f64() * 1e6);
    }

    // 3. Fail-closed preflight rejection latency on rank-zero genesis mutant (median of 500 runs).
    let mut bad_bundle = s3.package.launch_closure_bundle_bytes.clone();
    bad_bundle[2..6].copy_from_slice(&11u32.to_be_bytes());
    let mut rej_samples = Vec::with_capacity(500);
    for _ in 0..500 {
        let t0 = Instant::now();
        let err = preflight_launch_closure_bundle_bytes(&bad_bundle, g_len, caps).unwrap_err();
        std::hint::black_box(err);
        rej_samples.push(t0.elapsed().as_secs_f64() * 1e6);
    }

    println!("CHL1_PREFLIGHT_US={:.2}", median_us(pf_samples));
    println!("CHL1_VERIFY_S3_US={:.2}", median_us(ver_samples));
    println!("CHL1_REJECT_US={:.2}", median_us(rej_samples));
    println!(
        "CHL1_S3_INTERNAL_ARTIFACTS={}",
        s3.bundle.internal_artifact_count_u32
    );
    println!(
        "CHL1_S3_COMPLETE_ARTIFACTS={}",
        s3.bundle.complete_artifact_count_u32
    );
    println!(
        "CHL1_S3_COMPLETE_EDGES={}",
        s3.bundle.complete_dependency_edge_count_u32
    );
    println!(
        "CHL1_S3_INTERNAL_BYTES={}",
        s3.bundle.internal_canonical_preimage_bytes_u64
    );
    println!(
        "CHL1_S3_EXTERNAL_BYTES={}",
        s3.bundle.external_root_canonical_preimage_bytes_u64
    );
    println!(
        "CHL1_S3_TOTAL_PREIMAGE_BYTES={}",
        s3.bundle.total_canonical_preimage_bytes_u64
    );
    println!(
        "CHL1_S3_BUNDLE_WIRE_BYTES={}",
        s3.package.launch_closure_bundle_bytes.len()
    );
    println!(
        "CHL1_S3_PACKAGE_WIRE_BYTES={}",
        s3.package.encode_canonical().unwrap().len()
    );
    println!(
        "CHL1_S0_COMPLETE_ARTIFACTS={}",
        s0.bundle.complete_artifact_count_u32
    );
    println!(
        "CHL1_S0_COMPLETE_EDGES={}",
        s0.bundle.complete_dependency_edge_count_u32
    );
    println!(
        "CHL1_S0_TOTAL_PREIMAGE_BYTES={}",
        s0.bundle.total_canonical_preimage_bytes_u64
    );
    println!("CHL1_S3_BUNDLE_TAG={}", s3.bundle.tag().unwrap());
    println!(
        "CHL1_S3_MANIFEST_TAG={}",
        s3.package.manifest.tag().unwrap()
    );
    println!(
        "CHL1_S3_AUTH_TAG={}",
        s3.package.authorization.tag().unwrap()
    );
    println!("CHL1_S3_PACKAGE_TAG={}", s3.package.tag().unwrap());
    println!("CHL1_S0_PACKAGE_TAG={}", s0.package.tag().unwrap());
}
