// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::os::unix::fs::{symlink, PermissionsExt};
const SOURCE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const REALM: &str = "ce24de2e-57ee-456e-999a-a62899f96a23";
fn fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
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

fn principal(key: u8, kind: rustsimhub_policy::Kind) -> rustsimhub_policy::Principal {
    rustsimhub_policy::Principal {
        id: format!("synthetic-{key}"),
        public_key: [key; 32],
        kind,
        scopes: rustsimhub_policy::ALL_SCOPES,
        generation: 1,
        active: true,
    }
}
fn reader_fixture() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let (root, input, archive) = fixture();
    let db = Connection::open(&input).unwrap();
    db.execute_batch("INSERT INTO modemdeck_lines(line_id,line_label) VALUES('private_b','Private B');INSERT INTO modemdeck_user_lines VALUES('legacy_user_b','private_b',NULL);INSERT INTO sms(id,line_id,peer,content,type) VALUES(55,'private_b','+12025550999','only user b',1);INSERT INTO contacts(id,owner_user_id,display_name) VALUES('contact_b','legacy_user_b','private contact b');").unwrap();
    drop(db);
    import(&input, SOURCE, &archive).unwrap();
    let access = root.path().join("access");
    fs::create_dir(&access).unwrap();
    fs::set_permissions(&access, fs::Permissions::from_mode(0o700)).unwrap();
    (root, input, archive, access)
}
fn record_id(archive: &Path, entity: &str, key: &str) -> String {
    Connection::open(archive.join("history.sqlite"))
        .unwrap()
        .query_row(
            "SELECT seq FROM history_records WHERE source=?1 AND entity=?2 AND legacy_key=?3",
            params![SOURCE, entity, key],
            |r| Ok(r.get::<_, i64>(0)?.to_string()),
        )
        .unwrap()
}

#[test]
fn archive_authority_is_explicit_line_and_contact_ownership_are_separate_and_user_flags_never_mix()
{
    use rustsimhub_domain::HistoryEntity as E;
    use rustsimhub_policy::Kind;
    let (_root, _input, archive, access) = reader_fixture();
    let mut service = HistoryService::open(&archive, &access, [77; 32], REALM).unwrap();
    let owner = principal(1, Kind::Owner);
    let a = principal(2, Kind::Client);
    let b = principal(3, Kind::Client);
    assert!(service.sources(&a).unwrap().is_empty());
    assert!(matches!(
        service.page(&a, SOURCE, E::Sms, None, 16),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        service.grant(&owner, &a, SOURCE, None, &[E::Sms]),
        Err(Error::Forbidden)
    ));
    service
        .grant(
            &owner,
            &a,
            SOURCE,
            Some("legacy_user_a"),
            &[E::Sms, E::Calls, E::Contacts],
        )
        .unwrap();
    service
        .grant(
            &owner,
            &b,
            SOURCE,
            Some("legacy_user_b"),
            &[E::Sms, E::Calls, E::Contacts],
        )
        .unwrap();
    assert_eq!(
        service.page(&a, SOURCE, E::Sms, None, 16).unwrap()["records"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        service.page(&b, SOURCE, E::Sms, None, 16).unwrap()["records"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    let private_sms = record_id(&archive, "sms", "[55]");
    assert!(matches!(
        service.detail(&a, SOURCE, E::Sms, &private_sms),
        Err(Error::Forbidden)
    ));
    assert_eq!(
        service.detail(&b, SOURCE, E::Sms, &private_sms).unwrap()["content"],
        "only user b"
    );
    let sms = record_id(&archive, "sms", "[42]");
    let tombstone = record_id(&archive, "sms", "[43]");
    let deleted = service.detail(&a, SOURCE, E::Sms, &tombstone).unwrap();
    assert_eq!(deleted["deleted"], true);
    for field in ["content", "sender", "recipient", "peer"] {
        assert!(deleted.get(field).is_none());
    }
    let ad = service.detail(&a, SOURCE, E::Sms, &sms).unwrap();
    let bd = service.detail(&b, SOURCE, E::Sms, &sms).unwrap();
    assert_eq!(ad["user_state"]["read"], true);
    assert_eq!(bd["user_state"]["read"], false);
    assert_eq!(ad["user_state"]["favorite"], true);
    assert_eq!(bd["user_state"]["favorite"], false);
    assert_eq!(bd["user_state"]["marked_unread"], true);
    for hidden in [
        "imsi",
        "iccid",
        "line_id",
        "endpoint_line_id",
        "request_id",
        "legacy_user",
    ] {
        assert!(ad.get(hidden).is_none());
    }
    let call = record_id(&archive, "call_history", "[\"call_synthetic\"]");
    assert_eq!(
        service.detail(&a, SOURCE, E::Calls, &call).unwrap()["user_state"]["read"],
        true
    );
    assert_eq!(
        service.detail(&b, SOURCE, E::Calls, &call).unwrap()["user_state"]["read"],
        false
    );
    assert!(service
        .detail(&b, SOURCE, E::Calls, &call)
        .unwrap()
        .get("read_at")
        .is_none());
    let contact = record_id(&archive, "contacts", "[\"contact_synthetic\"]");
    assert_eq!(
        service.detail(&a, SOURCE, E::Contacts, &contact).unwrap()["phones"][0]["canonical_e164"],
        "+12025550123"
    );
    assert!(matches!(
        service.detail(&b, SOURCE, E::Contacts, &contact),
        Err(Error::Forbidden)
    ));
}

#[test]
fn archive_pagination_rechecks_grants_generations_scopes_and_cursor_audience() {
    use rustsimhub_domain::{HistoryCursor, HistoryEntity as E};
    use rustsimhub_policy::{Kind, Scope};
    let (_root, _input, archive, access) = reader_fixture();
    let owner = principal(1, Kind::Owner);
    let mut service = HistoryService::open(&archive, &access, [77; 32], REALM).unwrap();
    let a = principal(2, Kind::Client);
    let mut b = principal(3, Kind::Client);
    for p in [&a, &b] {
        service
            .grant(&owner, p, SOURCE, Some("legacy_user_a"), &[E::Sms])
            .unwrap();
    }
    let first = service.page(&a, SOURCE, E::Sms, None, 1).unwrap();
    let cursor: HistoryCursor = serde_json::from_value(first["cursor"].clone()).unwrap();
    let second = service.page(&a, SOURCE, E::Sms, Some(&cursor), 1).unwrap();
    assert_ne!(first["records"][0]["id"], second["records"][0]["id"]);
    assert!(matches!(
        service.page(&b, SOURCE, E::Sms, Some(&cursor), 1),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        service.page(&a, &"b".repeat(64), E::Sms, Some(&cursor), 1),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        service.page(&a, SOURCE, E::Calls, Some(&cursor), 1),
        Err(Error::Forbidden)
    ));
    b.generation += 1;
    assert!(matches!(
        service.page(&b, SOURCE, E::Sms, None, 1),
        Err(Error::Forbidden)
    ));
    let mut narrowed = a.clone();
    narrowed.scopes = 1 << (Scope::StatusRead as u16);
    assert!(matches!(
        service.page(&narrowed, SOURCE, E::Sms, Some(&cursor), 1),
        Err(Error::Forbidden)
    ));
    service.revoke(&owner, a.public_key, SOURCE).unwrap();
    assert!(service.sources(&a).unwrap().is_empty());
    assert!(matches!(
        service.page(&a, SOURCE, E::Sms, Some(&cursor), 1),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        service.grant(&a, &b, SOURCE, Some("legacy_user_a"), &[E::Sms]),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        service.page(&owner, SOURCE, E::Sms, None, 17),
        Err(Error::Bound)
    ));
    let nas = principal(4, Kind::Nas);
    assert!(matches!(
        service.page(&nas, SOURCE, E::Sms, None, 1),
        Err(Error::Forbidden)
    ));
    service
        .grant(&owner, &nas, SOURCE, None, &[E::Sms])
        .unwrap();
    assert_eq!(
        service.page(&nas, SOURCE, E::Sms, None, 16).unwrap()["records"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn grants_survive_normal_restart_but_never_changed_identity_realm_or_archive() {
    use rustsimhub_domain::HistoryEntity as E;
    use rustsimhub_policy::Kind;
    let (_root, input, archive, access) = reader_fixture();
    let owner = principal(1, Kind::Owner);
    let a = principal(2, Kind::Client);
    let mut s = HistoryService::open(&archive, &access, [77; 32], REALM).unwrap();
    s.grant(&owner, &a, SOURCE, Some("legacy_user_a"), &[E::Sms])
        .unwrap();
    assert!(matches!(import(&input, SOURCE, &archive), Err(Error::Busy)));
    drop(s);
    let s = HistoryService::open(&archive, &access, [77; 32], REALM).unwrap();
    assert_eq!(s.sources(&a).unwrap().len(), 1);
    drop(s);
    assert!(matches!(
        HistoryService::open(&archive, &access, [78; 32], REALM),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        HistoryService::open(
            &archive,
            &access,
            [77; 32],
            "3419ebeb-60c4-4088-b96d-366c64fc868c"
        ),
        Err(Error::Forbidden)
    ));
    import(&input, &"b".repeat(64), &archive).unwrap();
    assert!(matches!(
        HistoryService::open(&archive, &access, [77; 32], REALM),
        Err(Error::Forbidden)
    ));
}

#[test]
fn corrupted_authorization_rows_and_changes_behind_the_archive_lock_fail_closed() {
    use rustsimhub_domain::HistoryEntity as E;
    use rustsimhub_policy::Kind;
    let (_root, _input, archive, access) = reader_fixture();
    let db = Connection::open(archive.join("history.sqlite")).unwrap();
    db.execute(
        "UPDATE history_records SET payload='{}' WHERE entity='modemdeck_user_lines'",
        [],
    )
    .unwrap();
    drop(db);
    assert!(matches!(
        HistoryService::open(&archive, &access, [77; 32], REALM),
        Err(Error::Corrupt)
    ));
    let (_root, _input, archive, access) = reader_fixture();
    let s = HistoryService::open(&archive, &access, [77; 32], REALM).unwrap();
    let db = Connection::open(archive.join("history.sqlite")).unwrap();
    db.execute(
        "UPDATE history_records SET payload='{}' WHERE entity='sms'",
        [],
    )
    .unwrap();
    drop(db);
    assert!(matches!(
        s.page(&principal(1, Kind::Owner), SOURCE, E::Sms, None, 16),
        Err(Error::Corrupt)
    ));
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
