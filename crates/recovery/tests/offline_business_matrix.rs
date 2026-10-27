// SPDX-License-Identifier: AGPL-3.0-only
//! Minimal test-only wiring; workspace/Cargo.lock and production code are unchanged.
#[cfg(target_os = "linux")]
#[test]
fn real_business_backup_restore_matrix_runs_every_required_case() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let controller = std::process::Command::new("python3")
        .current_dir(root)
        .args([
            "-m",
            "unittest",
            "discover",
            "-s",
            "tools/fixtures/backup-recovery",
            "-p",
            "test_runner.py",
        ])
        .output()
        .expect("Python 3 is required for artifact-selection tests");
    assert!(
        controller.status.success(),
        "matrix controller tests failed:\n{}",
        String::from_utf8_lossy(&controller.stderr)
    );
    let output = std::process::Command::new("python3")
        .current_dir(root)
        .arg(root.join("tools/test-business-recovery-matrix.py"))
        .arg("--run")
        .output()
        .expect("C-09..12 matrix requires Python 3 and the pinned Rust/Clippy");
    assert!(
        output.status.success(),
        "real recovery matrix failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["passed"], true);
    assert_eq!(result["case_count"], 15);
    assert_eq!(result["backend"], "locked-production-rust-rlibs");
}
