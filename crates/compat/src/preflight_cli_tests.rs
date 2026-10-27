// SPDX-License-Identifier: AGPL-3.0-only
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn history_preflight_cli_uses_real_sqlite_and_outputs_redacted_rejections() {
    let _guard = crate::OFFLINE_CLI_TEST_LOCK.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let source = root.path().join("snapshot.sqlite");
    let db = rusqlite::Connection::open(&source).unwrap();
    db.execute_batch(include_str!("history/test-schema.sql"))
        .unwrap();
    drop(db);
    fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
    let archive = root.path().join("archive");
    fs::create_dir(&archive).unwrap();
    fs::set_permissions(&archive, fs::Permissions::from_mode(0o700)).unwrap();
    let source_id = "a".repeat(64);
    let args = vec![
        "preflight".into(),
        source.to_str().unwrap().into(),
        source_id,
        archive.to_str().unwrap().into(),
    ];
    let before = fs::read(&source).unwrap();
    let mut output = Vec::new();
    crate::run_cli(args.clone(), &mut output).unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["accepted"], true);
    assert_eq!(report["eligible_for_history_import"], true);
    assert_eq!(report["tables"]["sms"]["counts"]["new"], 3);
    assert!(fs::read_dir(&archive).unwrap().next().is_none());
    assert_eq!(fs::read(&source).unwrap(), before);
    rustsimhub_compat::history::import(&source, &"a".repeat(64), &archive).unwrap();
    let db = rusqlite::Connection::open(&source).unwrap();
    db.execute(
        "UPDATE sms SET content='SYNTHETIC_SECRET_CONFLICT' WHERE id=42",
        [],
    )
    .unwrap();
    drop(db);
    let before = fs::read(archive.join("history.sqlite")).unwrap();
    output.clear();
    assert!(crate::run_cli(args, &mut output).is_err());
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["accepted"], false);
    assert_eq!(report["reasons"]["conflicting_records"], 1);
    let text = String::from_utf8(output).unwrap();
    assert!(!text.contains("SYNTHETIC_SECRET_CONFLICT") && !text.contains("+12025550123"));
    assert_eq!(fs::read(archive.join("history.sqlite")).unwrap(), before);
}
