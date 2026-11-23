use std::time::Instant;

use bitblock::genesis_root_parity_1::{
    CanonicalDevnetGenesisFixtureV1, GenesisParityInputV1, verify_genesis_root_parity,
};
use bitblock::profile_1::build_canonical_devnet_profile_admission;
use bitblock::release_1::{
    AdmittedReleaseRecordV1, ReleaseAuthorizationSignDocEnvelopeV1,
    VulnerabilityWithdrawalStatusV1, build_canonical_devnet_admitted_release,
    verify_admitted_release_record,
};
use bitblock::release_r1::build_canonical_candidate_fixture;

fn median_us(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

fn main() {
    let fx = CanonicalDevnetGenesisFixtureV1::build().unwrap();
    let raw_pkg = fx.package.encode_canonical().unwrap();
    let raw_bnd = fx.bundle.encode_canonical().unwrap();
    let parity = verify_genesis_root_parity(&GenesisParityInputV1 {
        raw_genesis_package: &raw_pkg,
        raw_preimage_bundle: &raw_bnd,
        profile_scope: &fx.profile_scope,
        commit_domain_registry: &fx.commit_domain_registry,
        bound_manifest: &fx.bound_manifest,
        transition_spec: &fx.transition_spec,
        exec_registry: &fx.exec_registry,
    })
    .unwrap();
    let (_closure, manifest) = build_canonical_devnet_profile_admission(
        &fx.package,
        &fx.bound_manifest,
        &fx.transition_spec,
        &parity.record,
    )
    .unwrap();
    let (cand_in, cand_rcpt) = build_canonical_candidate_fixture(&fx.bound_manifest).unwrap();
    let (policy, record) = build_canonical_devnet_admitted_release(
        &fx.package,
        &manifest,
        &parity.record,
        &cand_in,
        &cand_rcpt,
    )
    .unwrap();

    let mut sign_us = Vec::with_capacity(41);
    for _ in 0..41 {
        let t0 = Instant::now();
        let _ = build_canonical_devnet_admitted_release(
            &fx.package,
            &manifest,
            &parity.record,
            &cand_in,
            &cand_rcpt,
        )
        .unwrap();
        sign_us.push(t0.elapsed().as_secs_f64() * 1e6);
    }

    let enc = record.encode_canonical().unwrap();
    let mut verify_us = Vec::with_capacity(41);
    for _ in 0..41 {
        let t0 = Instant::now();
        let dec = AdmittedReleaseRecordV1::decode_canonical(&enc).unwrap();
        let _ = verify_admitted_release_record(
            &dec,
            &fx.package,
            &manifest,
            &parity.record,
            &cand_in,
            &cand_rcpt,
            &policy,
            None,
        )
        .unwrap();
        verify_us.push(t0.elapsed().as_secs_f64() * 1e6);
    }

    let mut withdrawn = record.clone();
    withdrawn.core.vulnerability_status =
        VulnerabilityWithdrawalStatusV1::WithdrawnSecurityAdvisory;
    let mut sub_q = record.clone();
    sub_q.authorization.signatures.pop();

    let mut reject_us = Vec::with_capacity(41);
    for _ in 0..41 {
        let t0 = Instant::now();
        assert!(
            verify_admitted_release_record(
                &withdrawn,
                &fx.package,
                &manifest,
                &parity.record,
                &cand_in,
                &cand_rcpt,
                &policy,
                None,
            )
            .is_err()
        );
        assert!(
            verify_admitted_release_record(
                &sub_q,
                &fx.package,
                &manifest,
                &parity.record,
                &cand_in,
                &cand_rcpt,
                &policy,
                None,
            )
            .is_err()
        );
        reject_us.push(t0.elapsed().as_secs_f64() * 1e6);
    }

    let env = ReleaseAuthorizationSignDocEnvelopeV1 {
        sign_doc: record.authorization.sign_doc,
    };
    println!("CORE_TAG={}", record.core.tag().unwrap());
    println!(
        "SIGNDOC_TAG={}",
        record.authorization.sign_doc.tag().unwrap()
    );
    println!("AUTHDOC_TAG={}", env.tag().unwrap());
    println!("RECORD_TAG={}", record.tag().unwrap());
    println!("RECORD_BYTES={}", enc.len());
    println!("REL1_SIGN_US={:.2}", median_us(sign_us));
    println!("REL1_VERIFY_US={:.2}", median_us(verify_us));
    println!("REL1_REJECT_US={:.2}", median_us(reject_us));
}
