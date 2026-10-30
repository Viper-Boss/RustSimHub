// SPDX-License-Identifier: AGPL-3.0-only
//! Current boundaries, not an implemented schema9 or execution ledger.
use super::*;
use std::os::unix::fs::PermissionsExt;
const LEGACY: &str = include_str!("../../recovery-grants-legacy-ledger-schema8.sql");

#[test]
fn grant_management_receipt_is_not_a_wifi_execution_or_request_lookup() {
    let (_d, _b, s, owner) = fixture();
    let h = s.handle();
    let g = grant(&h, &owner, 2, 0, 1).unwrap();
    let snapshot = h
        .recovery_grant_request(owner, request(1))
        .unwrap()
        .unwrap();
    assert_eq!(snapshot, g);
    let value = serde_json::to_value(snapshot).unwrap();
    assert!(value.get("trial_id").is_none());
    assert!(value.get("settings_saved").is_none());
    assert!(value.get("state").is_none());
    let counts=h.call(|db| {
        let executions:i64=db.connection.query_row("SELECT COUNT(*) FROM sqlite_schema WHERE name IN ('recovery_receipts','recovery_execution_receipts','recovery_execution_meta')",[],|r|r.get(0))?;
        let management:i64=db.connection.query_row("SELECT COUNT(*) FROM recovery_grant_receipts",[],|r|r.get(0))?;
        Ok((executions,management))
    }).unwrap();
    assert_eq!(counts, (0, 1));
}

fn hybrid_schema8() -> (TempDir, Vec<u8>) {
    let (d, _b, s, owner) = fixture();
    let h = s.handle();
    grant(&h, &owner, 2, 0, 1).unwrap();
    drop(h);
    drop(s);
    let db = Connection::open(d.path().join("store.sqlite3")).unwrap();
    db.execute_batch(LEGACY).unwrap();
    drop(db);
    let bytes = fs::read(d.path().join("store.sqlite3")).unwrap();
    (d, bytes)
}
#[test]
fn blindly_appending_old_ledger_schema8_is_rejected_without_reinterpreting_current_grants() {
    let (d, before) = hybrid_schema8();
    assert!(matches!(Store::open(d.path()), Err(Error::NewerSchema)));
    assert!(matches!(
        Store::open_recovery_grants(d.path()),
        Err(Error::NewerSchema)
    ));
    assert_eq!(fs::read(d.path().join("store.sqlite3")).unwrap(), before);
    assert_eq!(version(d.path()), 8);
    let db = Connection::open(d.path().join("store.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM recovery_grants", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM recovery_receipts", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn unsupported_ledger_restore_keeps_source_and_quarantined_target_without_activation() {
    let (d, _) = hybrid_schema8();
    let backup = directory();
    let target = directory();
    let from = Connection::open(d.path().join("store.sqlite3")).unwrap();
    let path = backup.path().join("backup.sqlite3");
    let mut to = Connection::open(&path).unwrap();
    rusqlite::backup::Backup::new(&from, &mut to)
        .unwrap()
        .run_to_completion(128, Duration::from_millis(1), None)
        .unwrap();
    drop(to);
    drop(from);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let before = fs::read(&path).unwrap();
    assert!(matches!(
        Store::restore_recovery_grants(backup.path(), target.path()),
        Err(Error::NewerSchema)
    ));
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(version(target.path()), 8);
    assert!(target.path().join(".restore-pending").exists());
    assert!(matches!(
        Store::open(target.path()),
        Err(Error::NewerSchema)
    ));
    assert!(matches!(
        Store::open_recovery_grants(target.path()),
        Err(Error::NewerSchema)
    ));
    assert_eq!(version(target.path()), 8);
}

#[test]
fn schema9_review_candidate_runs_only_on_a_private_sqlite_copy_and_enforces_evidence_fields() {
    let (d, _b, s, owner) = fixture();
    let h = s.handle();
    let g = grant(&h, &owner, 2, 0, 1).unwrap();
    let b = directory();
    h.backup(b.path()).unwrap();
    let source = Connection::open(b.path().join("backup.sqlite3")).unwrap();
    let mut candidate = Connection::open_in_memory().unwrap();
    rusqlite::backup::Backup::new(&source, &mut candidate)
        .unwrap()
        .run_to_completion(128, Duration::from_millis(1), None)
        .unwrap();
    let original_ddl: String = candidate
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE name='recovery_grants'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    candidate.pragma_update(None, "foreign_keys", true).unwrap();
    candidate
        .execute_batch(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/ble/recovery-execution-schema9.sql"
        )))
        .unwrap();
    assert_eq!(
        candidate
            .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        9
    );
    assert_eq!(
        candidate
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE name='recovery_grants'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        original_ddl
    );
    candidate.execute("INSERT INTO recovery_execution_receipts VALUES(2,?,?,?,?,?,?,?,'begin_wifi',?,'reserved',NULL,NULL,NULL)",
        params![g.realm,g.device_public_key.as_slice(),g.owner_id,g.owner_generation as i64,g.id,g.generation as i64,request(2),[4u8;32].as_slice()]).unwrap();
    for sql in [
        "UPDATE recovery_execution_receipts SET state='saved'",
        "UPDATE recovery_execution_receipts SET state='ready'",
        "UPDATE recovery_execution_receipts SET action='cancel_wifi'",
        "UPDATE recovery_execution_receipts SET trial_id='not-a-trial'",
        "UPDATE recovery_execution_receipts SET state='rolled_back',settings_saved=0,rollback_verified=NULL",
    ] { assert!(candidate.execute(sql,[]).is_err(),"{sql}"); }
    assert_eq!(version(d.path()), 7);
    assert_eq!(
        h.recovery_grant(owner, g.independent_public_key).unwrap(),
        Some(g)
    );
}
