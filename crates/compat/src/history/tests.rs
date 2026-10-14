// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::os::unix::fs::{symlink, PermissionsExt};
const SOURCE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
fn fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("snapshot.sqlite");
    let db = Connection::open(&input).unwrap();
    db.execute_batch(include_str!("test-schema.sql")).unwrap();
    drop(db);
    fs::set_permissions(&input, fs::Permissions::from_mode(0o600)).unwrap();
    let destination = root.path().join("archive");
    fs::create_dir(&destination).unwrap();
    fs::set_permissions(&destination, fs::Permissions::from_mode(0o700)).unwrap();
    (root, input, destination)
}
#[test]
fn repeated_import_preserves_ids_tombstones_user_states_and_never_imports_credentials_or_outbox() {
    let (_root, input, destination) = fixture();
    let before = fs::read(&input).unwrap();
    fs::set_permissions(&input, fs::Permissions::from_mode(0o400)).unwrap();
    assert_eq!(validate(&input, SOURCE).unwrap().rows_by_entity["sms"], 3);
    assert_eq!(import(&input, SOURCE, &destination).unwrap().inserted, 15);
    let second = import(&input, SOURCE, &destination).unwrap();
    assert_eq!(second.inserted, 0);
    assert_eq!(second.unchanged, 15);
    assert_eq!(fs::read(&input).unwrap(), before);
    let db = Connection::open(destination.join("history.sqlite")).unwrap();
    let payload: String = db
        .query_row(
            "SELECT payload FROM history_records WHERE entity='sms' AND legacy_key='[43]'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let deleted: Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(deleted["deleted_at"], "2026-10-13T01:00:00Z");
    assert_eq!(deleted["id"], 43);
    let owner: String = db
        .query_row(
            "SELECT payload FROM history_records WHERE entity='contacts'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&owner).unwrap()["owner_user_id"],
        "legacy_user_a"
    );
    assert_eq!(db.query_row("SELECT COUNT(*) FROM history_records WHERE entity='modemdeck_user_message_thread_state'",[],|r|r.get::<_,i64>(0)).unwrap(),2);
    assert_eq!(db.query_row("SELECT COUNT(*) FROM sqlite_schema WHERE name IN ('principals','sms_operations','call_operations')",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    assert!(
        !String::from_utf8_lossy(&fs::read(destination.join("history.sqlite")).unwrap())
            .contains("MUST_NEVER_IMPORT_THIS_AUTH_SECRET")
    );
    assert_eq!(inspect(&destination).unwrap().rows_by_entity["sms"], 3);
    assert_eq!(
        fs::metadata(destination.join("history.sqlite"))
            .unwrap()
            .mode()
            & 0o777,
        0o600
    );
}
#[test]
fn conflicting_snapshot_cannot_revive_deleted_record_and_batch_rolls_back() {
    let (_root, input, destination) = fixture();
    import(&input, SOURCE, &destination).unwrap();
    let db = Connection::open(&input).unwrap();
    db.execute_batch("INSERT INTO sms(id,line_id,peer,content,type) VALUES(1,'line_synthetic','+12025550123','new lower-ID record',1); UPDATE sms SET deleted_at=NULL WHERE id=43;").unwrap();
    drop(db);
    assert!(matches!(
        import(&input, SOURCE, &destination),
        Err(Error::Conflict)
    ));
    let db = Connection::open(destination.join("history.sqlite")).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM history_records", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        15
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM history_batches", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    let value: String = db
        .query_row(
            "SELECT payload FROM history_records WHERE entity='sms' AND legacy_key='[43]'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!serde_json::from_str::<Value>(&value).unwrap()["deleted_at"].is_null());
}
#[test]
fn two_nas_namespaces_never_merge_identical_legacy_ids() {
    let (_root, input, destination) = fixture();
    import(&input, SOURCE, &destination).unwrap();
    assert_eq!(
        import(&input, &"b".repeat(64), &destination)
            .unwrap()
            .inserted,
        15
    );
    assert_eq!(inspect(&destination).unwrap().rows_by_entity["sms"], 6);
}
#[test]
fn active_snapshot_links_private_modes_foreign_schema_and_bad_rows_fail_without_archive() {
    let (root, input, destination) = fixture();
    let wal = root.path().join("snapshot.sqlite-wal");
    fs::write(&wal, b"").unwrap();
    assert!(matches!(
        import(&input, SOURCE, &destination),
        Err(Error::SnapshotRequired)
    ));
    fs::remove_file(wal).unwrap();
    let link = root.path().join("linked.sqlite");
    symlink(&input, &link).unwrap();
    assert!(import(&link, SOURCE, &destination).is_err());
    fs::set_permissions(&input, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(
        import(&input, SOURCE, &destination),
        Err(Error::Forbidden)
    ));
    fs::set_permissions(&input, fs::Permissions::from_mode(0o600)).unwrap();
    let hard = root.path().join("hard.sqlite");
    fs::hard_link(&input, &hard).unwrap();
    assert!(matches!(
        import(&input, SOURCE, &destination),
        Err(Error::Forbidden)
    ));
    fs::remove_file(hard).unwrap();
    let db = Connection::open(&input).unwrap();
    db.execute("UPDATE sms SET content=?1 WHERE id=42", [vec![0xffu8]])
        .unwrap();
    drop(db);
    assert!(matches!(
        import(&input, SOURCE, &destination),
        Err(Error::Schema)
    ));
    assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
    let db = Connection::open(&input).unwrap();
    db.execute_batch("DROP TABLE modemdeck_user_call_state")
        .unwrap();
    drop(db);
    assert!(matches!(validate(&input, SOURCE), Err(Error::Schema)));
}
#[test]
fn foreign_destination_and_concurrent_import_do_not_change_data() {
    let (_root, input, destination) = fixture();
    let path = destination.join("history.sqlite");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE sentinel(value TEXT);INSERT INTO sentinel VALUES('preserve');")
        .unwrap();
    drop(db);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(matches!(
        import(&input, SOURCE, &destination),
        Err(Error::Schema)
    ));
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let lock = private_file(&destination.join(".history.lock"), true, false).unwrap();
    FileExt::try_lock_exclusive(&lock).unwrap();
    assert!(matches!(
        import(&input, SOURCE, &destination),
        Err(Error::Busy)
    ));
}
