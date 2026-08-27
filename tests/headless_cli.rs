use std::path::Path;
use std::process::{Command, Output};

fn run_example(audit: Option<&str>) -> Output {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_bitblock"));
    command
        .current_dir(manifest)
        .arg("--simulate-actions")
        .arg(manifest.join("examples").join("observer_walk.tape"))
        .env_remove("BITBLOCK_DISPLAY_AUDIT");
    if let Some(value) = audit {
        command.env("BITBLOCK_DISPLAY_AUDIT", value);
    }
    command.output().expect("headless child process must start")
}

#[test]
fn headless_cli_output_is_independent_of_live_display_audit_environment() {
    let audit_off = run_example(None);
    let audit_on = run_example(Some("1"));

    assert!(
        audit_off.status.success(),
        "audit-off child failed: {}",
        String::from_utf8_lossy(&audit_off.stderr)
    );
    assert!(
        audit_on.status.success(),
        "audit-on child failed: {}",
        String::from_utf8_lossy(&audit_on.stderr)
    );
    assert_eq!(audit_off.stdout, audit_on.stdout);
    assert!(
        String::from_utf8_lossy(&audit_off.stdout)
            .starts_with("interaction-simulation\ttape=#bb.intent.sha256.")
    );
}
