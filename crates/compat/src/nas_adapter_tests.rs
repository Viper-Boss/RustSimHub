// SPDX-License-Identifier: AGPL-3.0-only
//! Run the real Go bridge contract and private-file CLI fixtures with the
//! compatibility tests on Linux. No gateway, NAS or device is contacted.
#[test]
fn go_nas_readonly_contract_and_synthetic_cli() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let output = std::process::Command::new("go")
        .current_dir(root)
        .args([
            "test",
            "-count=1",
            "-timeout=60s",
            "-v",
            "./tools/nas-adapter",
        ])
        .env("GO111MODULE", "off")
        .env("GOTOOLCHAIN", "local")
        .env("GOPROXY", "off")
        .env("GOSUMDB", "off")
        .env("CGO_ENABLED", "0")
        .env(
            "RSH_GOLDEN_FILE",
            root.join("crates/compat/src/legacy-golden.json"),
        )
        .output()
        .expect(
            "Linux NAS compatibility fixtures require Go on PATH; see tools/nas-adapter/README.md",
        );
    assert!(
        output.status.success(),
        "Go NAS contract/CLI fixtures failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("--- PASS: TestCLISyntheticConfigAndRead"),
        "Linux CLI fixture was not executed"
    );
}
