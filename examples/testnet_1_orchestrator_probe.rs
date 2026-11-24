use std::path::PathBuf;

use bitblock::testnet_1::{
    build_canonical_devnet_testnet_fixture, testnet_proc::run_live_six_process_testnet,
};

fn locate_node_bin() -> PathBuf {
    let exe = std::env::current_exe().expect("current_exe");
    let dir = exe
        .parent()
        .and_then(|p| p.parent())
        .expect("target profile dir");
    let name = if cfg!(windows) {
        "bitblock-testnet-node.exe"
    } else {
        "bitblock-testnet-node"
    };
    dir.join(name)
}

fn main() {
    let fx = build_canonical_devnet_testnet_fixture().expect("fixture");
    let node_bin = locate_node_bin();
    assert!(
        node_bin.exists(),
        "build bitblock-testnet-node first: missing {}",
        node_bin.display()
    );
    let work_dir = std::env::temp_dir().join(format!("bb_tn1_probe_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work_dir);
    let run = run_live_six_process_testnet(&node_bin, &work_dir).expect("6-process testnet run");
    let _ = std::fs::remove_dir_all(&work_dir);

    println!("TN1_BOOT_STAGE_MS={:.2}", run.boot_stage_ms);
    println!(
        "TN1_FAULT_RECOVERY_STAGE_MS={:.2}",
        run.fault_and_recovery_stage_ms
    );
    println!("TN1_CE44_SWITCHOVER_MS={:.2}", run.ce44_switchover_stage_ms);
    println!("TN1_POST_UPGRADE_MS={:.2}", run.post_upgrade_stage_ms);
    println!("TN1_TOTAL_RUN_MS={:.2}", run.total_run_ms);
    println!("TN1_OS_PIDS={:?}", run.initial_os_pids);
    println!("TN1_RESTARTED_V4_PID={}", run.restarted_validator4_pid);
    println!(
        "TN1_CONFIG_WIRE_BYTES={}",
        fx.cluster_config.encode_canonical().expect("cfg").len()
    );
    println!(
        "TN1_REPORT_WIRE_BYTES={}",
        run.coverage_report.encode_canonical().expect("rep").len()
    );
    println!(
        "TN1_CONFIG_TAG={}",
        fx.cluster_config.tag().expect("cfg tag")
    );
    println!(
        "TN1_REPORT_TAG={}",
        run.coverage_report.tag().expect("rep tag")
    );
}
