#![cfg(feature = "protocol-research")]

use std::collections::BTreeSet;
use std::path::Path;

use bitblock::testnet_1::{
    TESTNET_1_CLUSTER_CONFIG_SCHEMA, TESTNET_1_COVERAGE_REPORT_SCHEMA, TESTNET_FAULT_MASK_ALL_10,
    Testnet1Error, TestnetGateDispositionV1, TestnetNodeRoleV1,
    build_canonical_devnet_testnet_fixture, testnet_proc::run_live_six_process_testnet,
    validate_testnet_boot_pin,
};

#[test]
fn wave33_testnet_1_live_six_os_process_adversarial_cluster_and_ce44_upgrade() {
    let node_bin = Path::new(env!("CARGO_BIN_EXE_bitblock-testnet-node"));
    let work_dir = std::env::temp_dir().join(format!("bb_wave33_testnet_1_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work_dir);

    let run = run_live_six_process_testnet(node_bin, &work_dir).expect("live 6-process testnet");
    let _ = std::fs::remove_dir_all(&work_dir);

    // 6 distinct live OS PIDs + 7th distinct OS PID after hard Child::kill() crash recovery!
    let mut all_pids: BTreeSet<u32> = run.initial_os_pids.iter().copied().collect();
    assert_eq!(all_pids.len(), 6);
    assert!(all_pids.insert(run.restarted_validator4_pid));
    assert_eq!(all_pids.len(), 7);
    assert_eq!(
        run.bound_tcp_ports
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .len(),
        6
    );
    assert_eq!(run.coverage_report.total_os_processes_exercised, 6);
    assert_eq!(run.coverage_report.pre_upgrade_finalized_height, 3);
    assert_eq!(run.coverage_report.activation_height, 4);
    assert_eq!(run.coverage_report.post_upgrade_finalized_height, 6);
    assert_eq!(
        run.coverage_report.fault_controls_passed_mask,
        TESTNET_FAULT_MASK_ALL_10
    );
    assert!(run.coverage_report.all_required_gates_passed);
    assert_eq!(
        TESTNET_1_CLUSTER_CONFIG_SCHEMA,
        b"bitblock/testnet-1/cluster-config/v1"
    );
    assert_eq!(
        TESTNET_1_COVERAGE_REPORT_SCHEMA,
        b"bitblock/testnet-1/coverage-report/v1"
    );
    assert_eq!(
        TestnetNodeRoleV1::from_code(TestnetNodeRoleV1::ArchiveRecovery.code()).unwrap(),
        TestnetNodeRoleV1::ArchiveRecovery
    );
    let _cli_fn: fn() -> Result<(), Testnet1Error> =
        bitblock::testnet_1::testnet_proc::run_testnet_node_cli;
}

#[test]
fn wave33_testnet_1_eight_fail_closed_boot_matrix_and_ambient_registry_controls() {
    let fx = build_canonical_devnet_testnet_fixture().expect("fixture");
    let launch_pkg = &fx.successor_build.launch_build.package;
    let rel_pol = &fx.successor_build.launch_build.release_policy;
    let launch_pol = &fx.successor_build.launch_build.launch_policy;
    let succ_car = &fx.successor_build.carrier;

    // 1. Ambient successor registry attempt -> rejected fail-closed.
    assert!(matches!(
        validate_testnet_boot_pin(
            &fx.cluster_config,
            launch_pkg,
            rel_pol,
            launch_pol,
            Some(succ_car),
            Some(&fx.successor_build),
            true
        ),
        Err(Testnet1Error::AmbientOrSubstitutedReference(_))
    ));

    // 2. Substituted chain_launch_manifest_id -> rejected before height 1.
    let mut bad_cfg = fx.cluster_config.clone();
    bad_cfg.chain_launch_manifest_id[0] ^= 0xFF;
    assert!(matches!(
        validate_testnet_boot_pin(
            &bad_cfg,
            launch_pkg,
            rel_pol,
            launch_pol,
            Some(succ_car),
            Some(&fx.successor_build),
            false
        ),
        Err(Testnet1Error::BootPinRejected(_))
    ));

    // 3. Substituted launch_closure_bundle_id -> rejected before height 1.
    let mut bad_cfg = fx.cluster_config.clone();
    bad_cfg.launch_closure_bundle_id[0] ^= 0x01;
    assert!(matches!(
        validate_testnet_boot_pin(
            &bad_cfg,
            launch_pkg,
            rel_pol,
            launch_pol,
            Some(succ_car),
            Some(&fx.successor_build),
            false
        ),
        Err(Testnet1Error::BootPinRejected(_))
    ));

    // 4. Missing CE-44 successor carrier when in_place_upgrade_active = true -> rejected.
    assert!(matches!(
        validate_testnet_boot_pin(
            &fx.cluster_config,
            launch_pkg,
            rel_pol,
            launch_pol,
            None,
            Some(&fx.successor_build),
            false
        ),
        Err(Testnet1Error::BootPinRejected(_))
    ));

    // 5. Substituted successor_activation_bundle_id -> rejected.
    let mut bad_cfg = fx.cluster_config.clone();
    bad_cfg.successor_activation_bundle_id[0] ^= 0x42;
    assert!(matches!(
        validate_testnet_boot_pin(
            &bad_cfg,
            launch_pkg,
            rel_pol,
            launch_pol,
            Some(succ_car),
            Some(&fx.successor_build),
            false
        ),
        Err(Testnet1Error::BootPinRejected(_))
    ));

    // 6. Unresolved / excluded required G5.core gate -> rejected.
    let mut bad_cfg = fx.cluster_config.clone();
    for r in &mut bad_cfg.stage_gate_matrix {
        if r.subclaim_id == "G5.core" {
            r.disposition = TestnetGateDispositionV1::ExcludedWithReason;
            r.exclusion_reason = "attempted-bypass".to_string();
        }
    }
    assert!(matches!(
        bad_cfg.validate(),
        Err(Testnet1Error::GateMatrixViolation(_))
    ));

    // 7. Reassigning production-only G18 away from PROD-OPS-1 -> rejected.
    let mut bad_cfg = fx.cluster_config.clone();
    for r in &mut bad_cfg.stage_gate_matrix {
        if r.subclaim_id == "G18.production_ops" {
            r.owner_capability = "TESTNET-1".to_string();
        }
    }
    assert!(matches!(
        bad_cfg.validate(),
        Err(Testnet1Error::GateMatrixViolation(_))
    ));

    // 8. Incomplete fault mask in TestnetCoverageReportV1 -> rejected.
    let mut bad_rep = fx.expected_coverage_report;
    bad_rep.fault_controls_passed_mask &= !(1 << 2);
    assert!(matches!(
        bad_rep.validate(),
        Err(Testnet1Error::GateMatrixViolation(_))
    ));
}
