//! Release 4-way genesis-to-root parity benchmark probe for Wave 18
//! (`GENESIS-ROOT-PARITY-1`, `PS-031`).

use std::time::Instant;

use bitblock::genesis_root_parity_1::{
    CanonicalDevnetGenesisFixtureV1, GenesisParityInputV1, build_cold_genesis_snapshot,
    build_executor_genesis_snapshot, build_incremental_genesis_snapshot,
    verify_genesis_root_parity,
};

fn hex32(b: [u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for byte in b {
        use std::fmt::Write;
        let _ = write!(&mut s, "{byte:02x}");
    }
    s
}

fn main() {
    let fix = CanonicalDevnetGenesisFixtureV1::build().unwrap();
    let raw_pkg = fix.package.encode_canonical().unwrap();
    let raw_bundle = fix.bundle.encode_canonical().unwrap();
    let input = GenesisParityInputV1 {
        raw_genesis_package: &raw_pkg,
        raw_preimage_bundle: &raw_bundle,
        profile_scope: &fix.profile_scope,
        commit_domain_registry: &fix.commit_domain_registry,
        bound_manifest: &fix.bound_manifest,
        transition_spec: &fix.transition_spec,
        exec_registry: &fix.exec_registry,
    };
    let out = verify_genesis_root_parity(&input).unwrap();
    let pid = fix.bound_manifest.profile_id;

    let mut t_cold = Vec::with_capacity(50);
    let mut t_incr = Vec::with_capacity(50);
    let mut t_exec = Vec::with_capacity(50);
    for _ in 0..50 {
        let a = Instant::now();
        let (s1, _) = build_cold_genesis_snapshot(&fix.package, &fix.bundle, pid).unwrap();
        t_cold.push(a.elapsed().as_secs_f64() * 1e6);

        let b = Instant::now();
        let s2 = build_incremental_genesis_snapshot(&fix.package, &fix.bundle, pid).unwrap();
        t_incr.push(b.elapsed().as_secs_f64() * 1e6);

        let c = Instant::now();
        let s3 =
            build_executor_genesis_snapshot(&fix.package, &fix.bundle, pid, &fix.transition_spec)
                .unwrap();
        t_exec.push(c.elapsed().as_secs_f64() * 1e6);
        assert_eq!(s1, s2);
        assert_eq!(s1, s3);
    }
    t_cold.sort_by(f64::total_cmp);
    t_incr.sort_by(f64::total_cmp);
    t_exec.sort_by(f64::total_cmp);

    println!("PKG_TAG={}", fix.package.tag().unwrap());
    println!("BUNDLE_TAG={}", fix.bundle.tag().unwrap());
    println!("COMP_TAG={}", out.composition.tag().unwrap());
    println!("RECORD_TAG={}", out.record.tag().unwrap());
    println!(
        "PARITY_RECORD_ID={}",
        hex32(out.record.parity_record_id().unwrap())
    );
    println!("APP_ROOT={}", hex32(out.composition.application_root));
    println!("EXEC_ROOT={}", hex32(out.composition.execution_state_root));
    println!("COLD_P50_US={:.2}", t_cold[25]);
    println!("INCR_P50_US={:.2}", t_incr[25]);
    println!("EXEC_P50_US={:.2}", t_exec[25]);
}
