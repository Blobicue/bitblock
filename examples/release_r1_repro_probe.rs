use std::time::Instant;

use bitblock::consensus_1::ConsensusClusterHarnessV1;
use bitblock::release_r1::{
    BuilderAttestationV1, CandidateReleaseReceiptV1, PINNED_FRAMEWORK_MANIFEST_SHA256,
    build_canonical_candidate_fixture, verify_candidate_reproducibility,
};

fn p50_us(samples: &mut [f64]) -> f64 {
    samples.sort_by(|a, b| a.total_cmp(b));
    samples[samples.len() / 2]
}

fn main() {
    let cluster = ConsensusClusterHarnessV1::bootstrap_four_validators().unwrap();
    let bound = &cluster.fixture.bound_manifest;
    let (inputs, rcpt) = build_canonical_candidate_fixture(bound).unwrap();
    let b1 = BuilderAttestationV1 {
        builder_id: 1,
        native_artifact_sha256: rcpt.native_artifact_sha256,
        native_artifact_len: rcpt.native_artifact_len,
        wasm_artifact_sha256: rcpt.wasm_artifact_sha256,
        wasm_artifact_len: rcpt.wasm_artifact_len,
    };
    let b2 = BuilderAttestationV1 {
        builder_id: 2,
        ..b1
    };

    // Benchmark 1: PD-05 BoundProfileManifestV1 + dual-builder verification
    let mut ver_samples = Vec::with_capacity(31);
    for _ in 0..31 {
        let t0 = Instant::now();
        let _ = verify_candidate_reproducibility(
            bound,
            &inputs,
            PINNED_FRAMEWORK_MANIFEST_SHA256,
            &[b1, b2],
        )
        .unwrap();
        ver_samples.push(t0.elapsed().as_secs_f64() * 1e6);
    }

    // Benchmark 2: canonical encode + decode + ContentTag seal of both carriers
    let mut enc_samples = Vec::with_capacity(31);
    for _ in 0..31 {
        let t0 = Instant::now();
        let _ = inputs.tag().unwrap();
        let raw = rcpt.encode_canonical().unwrap();
        let _ = CandidateReleaseReceiptV1::decode_canonical(&raw).unwrap();
        let _ = rcpt.tag().unwrap();
        enc_samples.push(t0.elapsed().as_secs_f64() * 1e6);
    }

    // Benchmark 3: fail-closed divergence + admitted_release_authority=true rejection
    let mut b2_div = b2;
    b2_div.wasm_artifact_sha256[0] ^= 0xFF;
    let mut bad_adm = rcpt.encode_canonical().unwrap();
    bad_adm[CandidateReleaseReceiptV1::WIRE_LEN - 1] = 1;
    let mut rej_samples = Vec::with_capacity(31);
    for _ in 0..31 {
        let t0 = Instant::now();
        assert!(
            verify_candidate_reproducibility(
                bound,
                &inputs,
                PINNED_FRAMEWORK_MANIFEST_SHA256,
                &[b1, b2_div]
            )
            .is_err()
        );
        assert!(CandidateReleaseReceiptV1::decode_canonical(&bad_adm).is_err());
        rej_samples.push(t0.elapsed().as_secs_f64() * 1e6);
    }

    println!("RELR1_VERIFY_US={:.2}", p50_us(&mut ver_samples));
    println!("RELR1_ENCODE_US={:.2}", p50_us(&mut enc_samples));
    println!("RELR1_REJECT_US={:.2}", p50_us(&mut rej_samples));
    println!("RELR1_INPUTS_TAG={}", inputs.tag().unwrap());
    println!("RELR1_RCPT_TAG={}", rcpt.tag().unwrap());
}
