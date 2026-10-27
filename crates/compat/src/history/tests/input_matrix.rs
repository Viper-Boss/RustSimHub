// SPDX-License-Identifier: AGPL-3.0-only
//! C-07 uses the existing real SQLite fixtures and production APIs.
use super::*;
use rustsimhub_domain::{HistoryCursor, HistoryEntity as E};
use rustsimhub_policy::{Kind, Scope};
use std::{io::Read, path::PathBuf};

fn watch(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    for item in fs::read_dir(root).unwrap() {
        let path = item.unwrap().path();
        if path.is_dir() {
            files.extend(watch(&path));
        } else {
            files.insert(path.clone(), fs::read(path).unwrap());
        }
    }
    files
}
fn sentinels(root: &Path) {
    for name in [
        "identity.seed",
        "owner.json",
        "current.sqlite",
        "outbox.sqlite",
    ] {
        fs::write(root.join(name), b"SYNTHETIC_CURRENT_UNCHANGED").unwrap();
    }
}

#[test]
fn nas_identity_scope_and_revocation_denials_preserve_archive_access_and_current_files() {
    let (root, _, archive, access) = reader_fixture();
    sentinels(root.path());
    let owner = principal(1, Kind::Owner);
    let nas = principal(4, Kind::Nas);
    let mut service = HistoryService::open(&archive, &access, [77; 32], REALM).unwrap();
    service
        .grant(&owner, &nas, SOURCE, None, &[E::Sms])
        .unwrap();
    let first = service.page(&nas, SOURCE, E::Sms, None, 1).unwrap();
    let cursor: HistoryCursor = serde_json::from_value(first["cursor"].clone()).unwrap();
    let id = first["records"][0]["id"].as_str().unwrap();
    let mut variants = vec![principal(5, Kind::Nas)];
    let mut wrong_kind = nas.clone();
    wrong_kind.kind = Kind::Client;
    variants.push(wrong_kind);
    let mut revoked = nas.clone();
    revoked.active = false;
    variants.push(revoked);
    let mut generation = nas.clone();
    generation.generation += 1;
    variants.push(generation);
    let mut narrowed = nas.clone();
    narrowed.scopes = 1 << (Scope::StatusRead as u16);
    variants.push(narrowed);
    let before = watch(root.path());
    for actor in variants {
        assert!(service.sources(&actor).unwrap().is_empty());
        assert!(matches!(
            service.page(&actor, SOURCE, E::Sms, Some(&cursor), 1),
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            service.detail(&actor, SOURCE, E::Sms, id),
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            service.grant(&actor, &nas, SOURCE, None, &[E::Sms]),
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            service.revoke(&actor, nas.public_key, SOURCE),
            Err(Error::Forbidden)
        ));
    }
    assert!(matches!(
        service.page(&nas, SOURCE, E::Calls, None, 1),
        Err(Error::Forbidden)
    ));
    assert_eq!(watch(root.path()), before);
    service.revoke(&owner, nas.public_key, SOURCE).unwrap();
    let after_revoke = watch(root.path());
    assert!(matches!(
        service.page(&nas, SOURCE, E::Sms, Some(&cursor), 1),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        service.detail(&nas, SOURCE, E::Sms, id),
        Err(Error::Forbidden)
    ));
    assert_eq!(watch(root.path()), after_revoke);
}

#[test]
fn malformed_cursor_ids_namespaces_and_page_sizes_never_become_sql_or_authority() {
    let (root, _, archive, access) = reader_fixture();
    let owner = principal(1, Kind::Owner);
    let service = HistoryService::open(&archive, &access, [77; 32], REALM).unwrap();
    let page = service.page(&owner, SOURCE, E::Sms, None, 1).unwrap();
    let cursor: HistoryCursor = serde_json::from_value(page["cursor"].clone()).unwrap();
    let before = watch(root.path());
    for value in [
        "",
        "-1",
        "0 OR 1=1",
        "1; DROP TABLE history_records;--",
        "../1",
        "\0",
        "１２",
        "99999999999999999999",
    ] {
        assert!(matches!(
            service.detail(&owner, SOURCE, E::Sms, value),
            Err(Error::Forbidden)
        ));
        let mut bad = cursor.clone();
        bad.after = value.into();
        assert!(matches!(
            service.page(&owner, SOURCE, E::Sms, Some(&bad), 1),
            Err(Error::Forbidden)
        ));
    }
    assert!(matches!(
        service.detail(&owner, SOURCE, E::Sms, "0"),
        Err(Error::Forbidden)
    ));
    for size in [0, 17, usize::MAX] {
        assert!(matches!(
            service.page(&owner, SOURCE, E::Sms, None, size),
            Err(Error::Bound)
        ));
    }
    for field in 0..4 {
        let mut bad = cursor.clone();
        match field {
            0 => bad.archive_epoch = "f".repeat(64),
            1 => bad.audience = "f".repeat(64),
            2 => bad.source = "b".repeat(64),
            _ => bad.entity = E::Calls,
        }
        assert!(matches!(
            service.page(&owner, SOURCE, E::Sms, Some(&bad), 1),
            Err(Error::Forbidden)
        ));
    }
    for source in ["", "../archive", "a' OR 1=1--", "\0"] {
        assert!(matches!(
            service.page(&owner, source, E::Sms, None, 1),
            Err(Error::Forbidden)
        ));
    }
    assert_eq!(watch(root.path()), before);
}

#[test]
fn invalid_source_rows_and_missing_schema_preserve_an_existing_archive_and_current_files() {
    let cases = [
        ("UPDATE sms SET content=x'0001' WHERE id=42", Error::Schema),
        (
            "UPDATE sms SET content=CAST(x'80' AS TEXT) WHERE id=42",
            Error::Corrupt,
        ),
        ("UPDATE sms SET id=0 WHERE id=42", Error::Schema),
        ("UPDATE sms_contacts SET peer=''", Error::Schema),
        ("UPDATE sms_contacts SET line_id=NULL", Error::Schema),
        ("ALTER TABLE sms DROP COLUMN timestamp", Error::Schema),
        ("DROP TABLE contacts", Error::Schema),
    ];
    for (sql, expected) in cases {
        let (root, input, archive) = fixture();
        sentinels(root.path());
        import(&input, SOURCE, &archive).unwrap();
        let db = Connection::open(&input).unwrap();
        db.execute_batch(sql).unwrap();
        drop(db);
        let before = watch(root.path());
        assert!(matches!(validate(&input, SOURCE), Err(e) if e == expected));
        assert!(matches!(import(&input, SOURCE, &archive), Err(e) if e == expected));
        assert!(!preflight::preflight(&input, SOURCE, Some(&archive)).accepted);
        assert_eq!(watch(root.path()), before);
    }
    // Field length itself fits, but canonical JSON overhead crosses MAX_ROW.
    let (root, input, archive) = fixture();
    import(&input, SOURCE, &archive).unwrap();
    let db = Connection::open(&input).unwrap();
    db.execute(
        "UPDATE sms SET content=?1 WHERE id=42",
        ["x".repeat(MAX_ROW)],
    )
    .unwrap();
    drop(db);
    let before = watch(root.path());
    assert!(matches!(
        import(&input, SOURCE, &archive),
        Err(Error::Bound)
    ));
    assert_eq!(watch(root.path()), before);
}

#[test]
fn composite_duplicate_keys_are_idempotent_and_conflicting_duplicates_roll_back() {
    let (root, input, archive) = fixture();
    let db = Connection::open(&input).unwrap();
    db.execute_batch("INSERT INTO sms_contacts SELECT * FROM sms_contacts")
        .unwrap();
    drop(db);
    let r = import(&input, SOURCE, &archive).unwrap();
    assert_eq!((r.inserted, r.unchanged), (15, 1));
    let again = import(&input, SOURCE, &archive).unwrap();
    assert_eq!((again.inserted, again.unchanged), (0, 16));
    let db = Connection::open(&input).unwrap();
    db.execute_batch("INSERT INTO sms(id,line_id,content,type) VALUES(1,'line_synthetic','new before duplicate conflict',1); UPDATE sms_contacts SET unread_count=99 WHERE rowid=2;").unwrap();
    drop(db);
    let before = watch(root.path());
    assert!(matches!(
        import(&input, SOURCE, &archive),
        Err(Error::Conflict)
    ));
    assert_eq!(watch(root.path()), before);
    assert_eq!(inspect(&archive).unwrap().rows_by_entity["sms"], 3);
}

#[test]
fn valid_utf8_nul_and_unknown_time_are_preserved_without_synthesizing_values() {
    let (_root, input, archive) = fixture();
    let text = "合成\0é";
    let db = Connection::open(&input).unwrap();
    db.execute(
        "UPDATE sms SET content=?1,timestamp=NULL,sender=NULL WHERE id=42",
        [text],
    )
    .unwrap();
    drop(db);
    let source = fs::read(&input).unwrap();
    assert_eq!(validate(&input, SOURCE).unwrap().rows_by_entity["sms"], 3);
    let r = import(&input, SOURCE, &archive).unwrap();
    assert!(r.history_only && !r.credentials_imported && r.hardware_operations == 0);
    let db = Connection::open(archive.join("history.sqlite")).unwrap();
    let payload: String = db
        .query_row(
            "SELECT payload FROM history_records WHERE entity='sms' AND legacy_key='[42]'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let row: Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(row["content"], text);
    assert!(row["timestamp"].is_null() && row["sender"].is_null());
    assert_eq!(fs::read(&input).unwrap(), source);
}

#[test]
fn noncanonical_paths_links_and_invalid_source_ids_leave_all_files_unchanged() {
    let (root, input, archive) = fixture();
    import(&input, SOURCE, &archive).unwrap();
    let link = root.path().join("source-link.sqlite");
    symlink(&input, &link).unwrap();
    let directory_link = root.path().join("directory-link");
    symlink(root.path(), &directory_link).unwrap();
    // Do not recursively walk the directory symlink; compare real directories only.
    let before_source = fs::read(&input).unwrap();
    let before_archive = watch(&archive);
    for path in [
        PathBuf::from("snapshot.sqlite"),
        root.path().join("./snapshot.sqlite"),
        root.path().join("archive/../snapshot.sqlite"),
        link,
        directory_link.join("snapshot.sqlite"),
        root.path().join("bad\0.sqlite"),
    ] {
        assert!(import(&path, SOURCE, &archive).is_err());
    }
    assert!(import(&input, SOURCE, &root.path().join("archive/../archive")).is_err());
    let hard = root.path().join("hard.sqlite");
    fs::hard_link(&input, &hard).unwrap();
    assert!(matches!(
        import(&hard, SOURCE, &archive),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        import(&input, SOURCE, &archive),
        Err(Error::Forbidden)
    ));
    for source in [
        String::new(),
        "a".repeat(63),
        "a".repeat(65),
        "A".repeat(64),
        format!("{}\n", "a".repeat(63)),
        "../PRIVATE_NAMESPACE".into(),
        "\0".into(),
    ] {
        assert!(matches!(
            import(&input, &source, &archive),
            Err(Error::Forbidden)
        ));
        let report = preflight::preflight(&input, &source, Some(&archive));
        assert!(!report.accepted && report.source.namespace.is_none());
        assert!(!serde_json::to_string(&report)
            .unwrap()
            .contains("PRIVATE_NAMESPACE"));
    }
    assert_eq!(fs::read(&input).unwrap(), before_source);
    assert_eq!(watch(&archive), before_archive);
}

#[test]
fn wrong_archive_versions_are_refused_by_import_and_reader_without_replacement() {
    for pragma in ["PRAGMA user_version=2", "PRAGMA application_id=0"] {
        let (root, input, archive) = fixture();
        import(&input, SOURCE, &archive).unwrap();
        let access = root.path().join("access");
        fs::create_dir(&access).unwrap();
        fs::set_permissions(&access, fs::Permissions::from_mode(0o700)).unwrap();
        let db = Connection::open(archive.join("history.sqlite")).unwrap();
        db.execute_batch(pragma).unwrap();
        drop(db);
        let before = watch(root.path());
        assert!(matches!(
            import(&input, SOURCE, &archive),
            Err(Error::Schema)
        ));
        assert!(matches!(
            HistoryService::open(&archive, &access, [77; 32], REALM),
            Err(Error::Schema)
        ));
        assert_eq!(watch(root.path()), before);
    }
}

fn prefix(path: &Path, length: u64) -> Vec<u8> {
    let mut out = Vec::new();
    File::open(path)
        .unwrap()
        .take(length)
        .read_to_end(&mut out)
        .unwrap();
    out
}
#[test]
fn oversized_sparse_source_and_archive_are_rejected_without_truncation_or_reset() {
    let (root, input, archive) = fixture();
    import(&input, SOURCE, &archive).unwrap();
    let original_len = fs::metadata(&input).unwrap().len();
    let original_prefix = prefix(&input, original_len);
    let archived = fs::read(archive.join("history.sqlite")).unwrap();
    OpenOptions::new()
        .write(true)
        .open(&input)
        .unwrap()
        .set_len(MAX_SNAPSHOT + 1)
        .unwrap();
    assert!(matches!(
        import(&input, SOURCE, &archive),
        Err(Error::Bound)
    ));
    assert_eq!(fs::metadata(&input).unwrap().len(), MAX_SNAPSHOT + 1);
    assert_eq!(prefix(&input, original_len), original_prefix);
    assert_eq!(fs::read(archive.join("history.sqlite")).unwrap(), archived);
    drop(root);
    let (root, input, archive) = fixture();
    import(&input, SOURCE, &archive).unwrap();
    let path = archive.join("history.sqlite");
    let length = fs::metadata(&path).unwrap().len();
    let before = prefix(&path, length);
    let source = fs::read(&input).unwrap();
    OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(MAX_ARCHIVE_BYTES + 1)
        .unwrap();
    assert!(matches!(
        import(&input, SOURCE, &archive),
        Err(Error::Bound)
    ));
    let access = root.path().join("access");
    fs::create_dir(&access).unwrap();
    fs::set_permissions(&access, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(
        HistoryService::open(&archive, &access, [77; 32], REALM),
        Err(Error::Bound)
    ));
    assert_eq!(fs::metadata(&path).unwrap().len(), MAX_ARCHIVE_BYTES + 1);
    assert_eq!(prefix(&path, length), before);
    assert_eq!(fs::read(&input).unwrap(), source);
    assert!(fs::read_dir(access).unwrap().next().is_none());
}
