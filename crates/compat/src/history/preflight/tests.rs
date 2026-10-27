// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::os::unix::fs::{symlink, PermissionsExt};

const SOURCE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let input = root.path().join("snapshot.sqlite");
    let db = Connection::open(&input).unwrap();
    db.execute_batch(include_str!("../test-schema.sql"))
        .unwrap();
    drop(db);
    fs::set_permissions(&input, fs::Permissions::from_mode(0o600)).unwrap();
    let destination = root.path().join("archive");
    fs::create_dir(&destination).unwrap();
    fs::set_permissions(&destination, fs::Permissions::from_mode(0o700)).unwrap();
    // Adjacent private state must never be opened, overwritten or copied.
    for name in [
        "identity.seed",
        "owner.json",
        "current.sqlite",
        "outbox.sqlite",
    ] {
        fs::write(root.path().join(name), b"SYNTHETIC_CURRENT_STATE_SENTINEL").unwrap();
    }
    (root, input, destination)
}

fn manifest(path: &Path) -> BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut entries = BTreeMap::new();
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            entries.extend(manifest(&path));
        } else {
            entries.insert(path.clone(), fs::read(path).unwrap());
        }
    }
    entries
}

fn total(report: &PreflightReport, field: fn(&Counts) -> usize) -> usize {
    report.tables.values().map(|t| field(&t.counts)).sum()
}

#[test]
fn source_only_reports_format_counts_unknown_values_and_no_payloads() {
    let (root, input, archive) = fixture();
    let db = Connection::open(&input).unwrap();
    db.execute_batch("PRAGMA application_id=1234; PRAGMA user_version=47;")
        .unwrap();
    drop(db);
    let before = manifest(root.path());
    let report = preflight(&input, SOURCE, None);
    assert!(report.accepted && report.contract_scan_complete);
    assert_eq!(report.eligible_for_history_import, None);
    assert_eq!(report.archive.comparison, Comparison::NotRequested);
    assert_eq!(report.source.format, "legacy_nas_sqlite_column_contract_v1");
    assert_eq!(report.source.sqlite_application_id, Some(1234));
    assert_eq!(report.source.sqlite_user_version, Some(47));
    assert_eq!(report.source.product_version, None);
    assert_eq!(report.source.namespace.as_deref(), Some(SOURCE));
    assert_eq!(report.tables["sms"].rows, Some(3));
    assert_eq!(report.tables["call_history"].rows, Some(1));
    assert_eq!(report.tables["contacts"].rows, Some(1));
    assert_eq!(total(&report, |c| c.uncompared), 15);
    assert_eq!(total(&report, |c| c.new), 0);
    assert_eq!(report.tables["sms"].missing_values["timestamp"], 3);
    assert_eq!(report.tables["sms"].missing_values["sender"], 1);
    assert_eq!(report.excluded_credential_tables, 1);
    assert_eq!(report.excluded_credential_rows, 1);
    let json = serde_json::to_string(&report).unwrap();
    for secret in [
        "+12025550123",
        "合成历史消息",
        "合成联系人",
        "contact_synthetic",
        "call_synthetic",
        "legacy_user_a",
        "legacy-device-synthetic",
        "MUST_NEVER_IMPORT_THIS_AUTH_SECRET",
        "SYNTHETIC_CURRENT_STATE_SENTINEL",
    ] {
        assert!(
            !json.contains(secret),
            "report leaked a synthetic private value"
        );
    }
    assert!(report.read_only && report.history_only && !report.credentials_imported);
    assert_eq!(report.hardware_operations, 0);
    assert_eq!(manifest(root.path()), before);
    assert!(!archive.join(".history.lock").exists());
}

#[test]
fn forecasts_match_real_importer_without_preflight_writes() {
    let (root, input, archive) = fixture();
    let before = manifest(root.path());
    let first = preflight(&input, SOURCE, Some(&archive));
    assert_eq!(first.eligible_for_history_import, Some(true));
    assert_eq!(first.archive.comparison, Comparison::NewArchive);
    assert_eq!(manifest(root.path()), before);
    let imported = import(&input, SOURCE, &archive).unwrap();
    assert_eq!(total(&first, |c| c.new), imported.inserted);
    let before = manifest(root.path());
    let repeated = preflight(&input, SOURCE, Some(&archive));
    assert!(repeated.accepted);
    assert_eq!(repeated.archive.comparison, Comparison::Compared);
    assert_eq!(total(&repeated, |c| c.unchanged), imported.inserted);
    assert_eq!(total(&repeated, |c| c.new), 0);
    assert_eq!(manifest(root.path()), before);
}

#[test]
fn conflicts_new_rows_and_distinct_sources_are_reported_without_overwrite() {
    let (root, input, archive) = fixture();
    import(&input, SOURCE, &archive).unwrap();
    let db = Connection::open(&input).unwrap();
    db.execute_batch("UPDATE sms SET content='SYNTHETIC_CHANGED_CONTENT' WHERE id=42; INSERT INTO sms(id,content) VALUES(45,'SYNTHETIC_NEW_CONTENT');").unwrap();
    drop(db);
    let before = manifest(root.path());
    let report = preflight(&input, SOURCE, Some(&archive));
    assert!(!report.accepted);
    assert_eq!(report.tables["sms"].counts.conflicts, 1);
    assert_eq!(report.tables["sms"].counts.new, 1);
    assert_eq!(report.tables["sms"].counts.unchanged, 2);
    assert_eq!(report.reasons[&Reason::ConflictingRecords], 1);
    let another = preflight(&input, &"b".repeat(64), Some(&archive));
    assert!(another.accepted);
    assert_eq!(total(&another, |c| c.conflicts), 0);
    assert_eq!(total(&another, |c| c.new), 16);
    assert_eq!(manifest(root.path()), before);
}

#[test]
fn missing_schema_and_null_keys_are_explicit_not_zero_record_success() {
    let (root, input, _) = fixture();
    let db = Connection::open(&input).unwrap();
    db.execute_batch("ALTER TABLE sms DROP COLUMN timestamp; DROP TABLE contacts;")
        .unwrap();
    drop(db);
    let before = manifest(root.path());
    let report = preflight(&input, SOURCE, None);
    assert!(!report.accepted && !report.contract_scan_complete);
    assert_eq!(report.tables["sms"].missing_columns, ["timestamp"]);
    assert_eq!(report.tables["sms"].counts.rejected, 3);
    assert_eq!(report.tables["contacts"].rows, None);
    assert!(report.reasons.contains_key(&Reason::MissingTable));
    assert!(report.reasons.contains_key(&Reason::MissingColumns));
    assert_eq!(manifest(root.path()), before);
}

#[test]
fn unsupported_records_and_malicious_names_are_counted_without_echoing_names() {
    let (root, input, _) = fixture();
    let db = Connection::open(&input).unwrap();
    let name = "PRIVATE_UNSUPPORTED_NAME\"; DROP TABLE sms;--";
    db.execute_batch(&format!("CREATE TABLE {}(body TEXT); INSERT INTO {} VALUES('PRIVATE_UNSUPPORTED_PAYLOAD'),('SECOND_SYNTHETIC_ROW'); CREATE VIEW future_view AS SELECT 1;", quoted(name), quoted(name))).unwrap();
    drop(db);
    let before = manifest(root.path());
    let report = preflight(&input, SOURCE, None);
    assert!(!report.accepted);
    assert_eq!(report.unsupported_tables, 2);
    assert_eq!(report.unsupported_known_rows, 2);
    assert!(!report.unsupported_counts_complete);
    assert!(report.reasons.contains_key(&Reason::UnsupportedRecords));
    assert_eq!(report.tables["sms"].rows, Some(3));
    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains("PRIVATE_UNSUPPORTED") && !json.contains("future_view"));
    assert_eq!(manifest(root.path()), before);
}

#[test]
fn invalid_rows_and_missing_keys_are_rejected_with_finite_reasons() {
    let (root, input, _) = fixture();
    let db = Connection::open(&input).unwrap();
    db.execute_batch("UPDATE sms SET content=x'00' WHERE id=42; UPDATE sms SET content=CAST(x'80' AS TEXT) WHERE id=43; INSERT INTO contact_phones(id) VALUES('');").unwrap();
    drop(db);
    let before = manifest(root.path());
    let report = preflight(&input, SOURCE, None);
    assert!(!report.accepted);
    assert_eq!(report.tables["sms"].counts.rejected, 2);
    assert_eq!(report.tables["sms"].counts.valid, 1);
    assert_eq!(report.tables["contact_phones"].counts.rejected, 1);
    assert_eq!(report.tables["contact_phones"].missing_values["id"], 1);
    assert!(report.reasons.contains_key(&Reason::InvalidRecord));
    assert!(report.reasons.contains_key(&Reason::InvalidText));
    assert_eq!(manifest(root.path()), before);
}

#[test]
fn duplicate_input_rows_are_separate_from_conflicting_duplicate_keys() {
    let (root, input, archive) = fixture();
    let db = Connection::open(&input).unwrap();
    db.execute_batch("INSERT INTO sms_contacts SELECT * FROM sms_contacts;")
        .unwrap();
    drop(db);
    let same = preflight(&input, SOURCE, Some(&archive));
    assert!(same.accepted);
    assert_eq!(same.tables["sms_contacts"].counts.duplicates, 1);
    assert_eq!(same.tables["sms_contacts"].counts.new, 1);
    let db = Connection::open(&input).unwrap();
    db.execute_batch("UPDATE sms_contacts SET unread_count=99 WHERE rowid=2;")
        .unwrap();
    drop(db);
    let before = manifest(root.path());
    let different = preflight(&input, SOURCE, Some(&archive));
    assert!(!different.accepted);
    assert_eq!(different.tables["sms_contacts"].counts.conflicts, 1);
    assert_eq!(manifest(root.path()), before);
}

#[test]
fn wrong_archive_version_busy_and_offline_guards_preserve_all_data() {
    let (root, input, archive) = fixture();
    initialize(&archive).unwrap();
    let db = Connection::open(archive.join("history.sqlite")).unwrap();
    db.execute_batch("PRAGMA user_version=99;").unwrap();
    drop(db);
    let before = manifest(root.path());
    let report = preflight(&input, SOURCE, Some(&archive));
    assert!(!report.accepted);
    assert_eq!(report.archive.sqlite_user_version, Some(99));
    assert!(report
        .reasons
        .contains_key(&Reason::UnsupportedArchiveFormat));
    assert_eq!(manifest(root.path()), before);
    let lock = archive_lock(&archive, true).unwrap();
    assert!(preflight(&input, SOURCE, Some(&archive))
        .reasons
        .contains_key(&Reason::Busy));
    drop(lock);
    fs::write(archive.join("history.sqlite-wal"), b"SYNTHETIC_WAL_MARKER").unwrap();
    let before = manifest(root.path());
    assert!(preflight(&input, SOURCE, Some(&archive))
        .reasons
        .contains_key(&Reason::OfflineArchiveRequired));
    assert_eq!(manifest(root.path()), before);
}

#[test]
fn source_permissions_links_sidecars_and_invalid_namespace_are_denied() {
    let (root, input, _) = fixture();
    fs::set_permissions(&input, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(preflight(&input, SOURCE, None)
        .reasons
        .contains_key(&Reason::PrivateSourceRequired));
    fs::set_permissions(&input, fs::Permissions::from_mode(0o600)).unwrap();
    let link = root.path().join("linked.sqlite");
    symlink(&input, &link).unwrap();
    assert!(preflight(&link, SOURCE, None)
        .reasons
        .contains_key(&Reason::PrivateSourceRequired));
    fs::write(
        root.path().join("snapshot.sqlite-wal"),
        b"SYNTHETIC_WAL_MARKER",
    )
    .unwrap();
    let before = manifest(root.path());
    assert!(preflight(&input, SOURCE, None)
        .reasons
        .contains_key(&Reason::OfflineSourceRequired));
    let bad = preflight(&input, "PRIVATE_INVALID_NAMESPACE", None);
    assert!(bad.reasons.contains_key(&Reason::InvalidNamespace));
    assert!(!serde_json::to_string(&bad)
        .unwrap()
        .contains("PRIVATE_INVALID_NAMESPACE"));
    assert_eq!(manifest(root.path()), before);
}

#[test]
fn row_and_row_count_limits_are_not_silently_truncated() {
    let (_root, input, _) = fixture();
    let db = Connection::open(&input).unwrap();
    db.execute(
        "UPDATE sms SET content=?1 WHERE id=42",
        ["x".repeat(MAX_ROW + 1)],
    )
    .unwrap();
    drop(db);
    let report = preflight(&input, SOURCE, None);
    assert!(!report.accepted && report.reasons.contains_key(&Reason::Bound));
    let (_root, input, _) = fixture();
    let db = Connection::open(&input).unwrap();
    db.execute_batch("DELETE FROM sms_contacts; WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100001) INSERT INTO sms_contacts(line_id,peer) SELECT 'synthetic-line',CAST(x AS TEXT) FROM n;").unwrap();
    drop(db);
    let report = preflight(&input, SOURCE, None);
    assert!(!report.accepted && report.reasons.contains_key(&Reason::Bound));
    assert_eq!(report.tables["sms_contacts"].rows, Some(MAX_ROWS + 1));
    assert!(!report.tables["sms_contacts"].row_count_exact);
}

#[test]
fn overlapping_source_and_archive_are_refused_without_creating_files() {
    let (root, input, _) = fixture();
    let renamed = root.path().join("history.sqlite");
    fs::rename(&input, &renamed).unwrap();
    let before = manifest(root.path());
    let report = preflight(&renamed, SOURCE, Some(root.path()));
    assert!(!report.accepted);
    assert!(report
        .reasons
        .contains_key(&Reason::SourceDestinationOverlap));
    assert_eq!(manifest(root.path()), before);
}
