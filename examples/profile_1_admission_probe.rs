use std::time::Instant;

use bitblock::genesis_root_parity_1::{
    CanonicalDevnetGenesisFixtureV1, GenesisParityInputV1, verify_genesis_root_parity,
};
use bitblock::profile_1::{
    ProfileAdmissionManifestV1, ProfileClosureEvidenceSetV1, RoleGraphEdgeV1,
    build_canonical_devnet_profile_admission, canonical_launch_role_graph_edges,
    verify_role_aware_closure_acyclic,
};

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

    let (closure, manifest) = build_canonical_devnet_profile_admission(
        &fx.package,
        &fx.bound_manifest,
        &fx.transition_spec,
        &parity.record,
    )
    .unwrap();

    let mut admit_us = Vec::with_capacity(61);
    for _ in 0..61 {
        let t0 = Instant::now();
        let _ = build_canonical_devnet_profile_admission(
            &fx.package,
            &fx.bound_manifest,
            &fx.transition_spec,
            &parity.record,
        )
        .unwrap();
        admit_us.push(t0.elapsed().as_secs_f64() * 1e6);
    }

    let mut encode_us = Vec::with_capacity(61);
    for _ in 0..61 {
        let t0 = Instant::now();
        let ec = closure.encode_canonical().unwrap();
        let em = manifest.encode_canonical().unwrap();
        let _ = ProfileClosureEvidenceSetV1::decode_canonical(&ec).unwrap();
        let _ = ProfileAdmissionManifestV1::decode_canonical(&em).unwrap();
        let _ = closure.tag().unwrap();
        let _ = manifest.tag().unwrap();
        encode_us.push(t0.elapsed().as_secs_f64() * 1e6);
    }

    let mut cyc_edges = canonical_launch_role_graph_edges();
    cyc_edges.push(RoleGraphEdgeV1 {
        from_node: "ASSET-1".to_string(),
        from_role: "implementation".to_string(),
        to_node: "PROFILE-1".to_string(),
        to_role: "new-admission".to_string(),
    });
    let mut bad_m = manifest.encode_canonical().unwrap();
    let last = bad_m.len() - 1;
    bad_m[last] = 0;

    let mut reject_us = Vec::with_capacity(61);
    for _ in 0..61 {
        let t0 = Instant::now();
        assert!(verify_role_aware_closure_acyclic(&cyc_edges).is_err());
        assert!(ProfileAdmissionManifestV1::decode_canonical(&bad_m).is_err());
        reject_us.push(t0.elapsed().as_secs_f64() * 1e6);
    }

    println!("CLOSURE_TAG={}", closure.tag().unwrap());
    println!("MANIFEST_TAG={}", manifest.tag().unwrap());
    println!(
        "CLOSURE_BYTES={}",
        closure.encode_canonical().unwrap().len()
    );
    println!(
        "MANIFEST_BYTES={}",
        manifest.encode_canonical().unwrap().len()
    );
    println!("PROF1_ADMIT_US={:.2}", median_us(admit_us));
    println!("PROF1_ENCODE_US={:.2}", median_us(encode_us));
    println!("PROF1_REJECT_US={:.2}", median_us(reject_us));
}
