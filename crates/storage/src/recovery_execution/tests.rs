// SPDX-License-Identifier: AGPL-3.0-only
//! Actual private SQLite/actor/Backup API. Raw rows are synthetic test setup;
//! this module does not provide a production request writer or fake authority.
use super::*;
use rustsimhub_policy::Principal;
use tempfile::TempDir;
fn directory() -> TempDir {
    crate::tests::directory()
}
fn request(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn seven() -> (TempDir, Store, Principal, RecoveryGrant) {
    let d = directory();
    let b = directory();
    let s = Store::open(d.path()).unwrap();
    s.handle().bind_identity([9; 32]).unwrap();
    let owner = s.handle().bootstrap_owner([1; 32]).unwrap();
    drop(s);
    let s = Store::migrate_recovery_grants(d.path(), b.path()).unwrap();
    let g = s
        .handle()
        .change_recovery_grant(
            owner.clone(),
            [2; 32],
            0,
            request(1),
            RecoveryChange::Grant {
                max_trial_ms: 40000,
            },
        )
        .unwrap();
    (d, s, owner, g)
}
fn nine() -> (TempDir, Store, Principal, RecoveryGrant) {
    let (d, s, owner, g) = seven();
    drop(s);
    let b = directory();
    Store::migrate_recovery_execution(d.path(), b.path()).unwrap();
    let s = Store::open_recovery_execution(d.path()).unwrap();
    (d, s, owner, g)
}
fn version(path: &Path) -> i64 {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap()
}
fn seed(h: &Handle, g: &RecoveryGrant, id: u32, action: &str, state: &str, trial: Option<String>) {
    let (g, action, state) = (g.clone(), action.to_owned(), state.to_owned());
    h.call(move |db| {
        let (saved, rollback) = match state.as_str() {
            "saved" => (Some(1), None),
            "rolled_back" => (Some(0), Some(1)),
            _ => (None, None),
        };
        db.connection.execute(
            "INSERT INTO recovery_execution_receipts VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            params![
                2,
                g.realm,
                g.device_public_key.as_slice(),
                g.owner_id,
                g.owner_generation as i64,
                g.id,
                g.generation as i64,
                request(id),
                action,
                [5u8; 32].as_slice(),
                state,
                trial,
                saved,
                rollback
            ],
        )?;
        Ok(())
    })
    .unwrap();
}
fn rows(h: &Handle) -> Vec<(String, String, Option<String>)> {
    h.call(|db|{
    Ok(db.connection.prepare("SELECT request_id,state,trial_id FROM recovery_execution_receipts ORDER BY request_id")?
        .query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?.collect::<std::result::Result<Vec<_>,_>>()?)
}).unwrap()
}
#[test]
fn explicit_exact_seven_to_nine_has_completed_seven_backup_and_default_gates() {
    let (d, s, owner, g) = seven();
    drop(s);
    let backup = directory();
    Store::migrate_recovery_execution(d.path(), backup.path()).unwrap();
    assert_eq!(version(&d.path().join("store.sqlite3")), 9);
    assert_eq!(version(&backup.path().join("backup.sqlite3")), 7);
    assert!(matches!(Store::open(d.path()), Err(Error::NewerSchema)));
    assert!(matches!(
        Store::open_recovery_grants(d.path()),
        Err(Error::NewerSchema)
    ));
    let s = Store::open_recovery_execution(d.path()).unwrap();
    assert_eq!(s.handle().check_recovery_grant(g.clone()).unwrap(), g);
    s.handle()
        .change_recovery_grant(owner, [2; 32], 1, request(2), RecoveryChange::Revoke)
        .unwrap();
    assert!(matches!(
        s.handle().check_recovery_grant(g),
        Err(Error::Forbidden)
    ));
}
#[test]
fn migration_backup_precedes_normal_open_recovery_writes() {
    let (d, s, _owner, _g) = seven();
    s.handle()
        .call(|db| {
            db.connection
                .execute("UPDATE line_observation SET status='ready'", [])?;
            Ok(())
        })
        .unwrap();
    drop(s);
    let backup = directory();
    Store::migrate_recovery_execution(d.path(), backup.path()).unwrap();
    for path in [
        d.path().join("store.sqlite3"),
        backup.path().join("backup.sqlite3"),
    ] {
        let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        assert_eq!(
            c.query_row("SELECT status FROM line_observation", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "ready"
        );
    }
    let s = Store::open_recovery_execution(d.path()).unwrap();
    assert_eq!(
        s.handle()
            .call(|db| Ok(db.connection.query_row(
                "SELECT status FROM line_observation",
                [],
                |r| r.get::<_, String>(0)
            )?))
            .unwrap(),
        "unavailable"
    );
}
#[test]
fn existing_backup_failure_keeps_source_format_rows_and_bytes() {
    let (d, s, _o, _g) = seven();
    s.handle()
        .call(|db| {
            db.connection
                .execute("UPDATE line_observation SET status='ready'", [])?;
            Ok(())
        })
        .unwrap();
    let b = directory();
    s.handle().backup(b.path()).unwrap();
    drop(s);
    let path = d.path().join("store.sqlite3");
    let before = fs::read(&path).unwrap();
    assert!(matches!(
        Store::migrate_recovery_execution(d.path(), b.path()),
        Err(Error::BackupExists)
    ));
    assert_eq!(fs::read(path).unwrap(), before);
    assert_eq!(version(&b.path().join("backup.sqlite3")), 7);
}
#[test]
fn actual_sqlite_full_rolls_back_ddl_after_completed_backup_under_same_actor() {
    let (d, s, _o, _g) = seven();
    drop(s);
    let b = directory();
    let backup = b.path().to_owned();
    let s = Store::start(d.path().to_owned(), None, OpenMode::ExecutionSource).unwrap();
    let result = s.handle().call(move |db| {
        db.connection.execute_batch("VACUUM")?;
        let pages: i64 = db
            .connection
            .pragma_query_value(None, "page_count", |r| r.get(0))?;
        db.connection.pragma_update(None, "max_page_count", pages)?;
        migrate_locked(db, &backup)
    });
    assert!(matches!(result, Err(Error::Full)));
    s.handle()
        .call(|db| {
            assert_eq!(
                db.connection
                    .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))?,
                7
            );
            assert_eq!(
                db.connection.query_row::<i64, _, _>(
                    "SELECT COUNT(*) FROM sqlite_schema WHERE name GLOB 'recovery_execution_*'",
                    [],
                    |r| r.get(0)
                )?,
                0
            );
            Ok(())
        })
        .unwrap();
    drop(s);
    assert_eq!(version(&b.path().join("backup.sqlite3")), 7);
    let restored = directory();
    assert!(Store::restore_recovery_grants(b.path(), restored.path()).is_ok());
}
#[test]
fn no_six_eight_nine_or_future_source_can_be_silently_migrated() {
    for version in [6, 8, 9, 10] {
        let (d, s, _o, _g) = seven();
        drop(s);
        let c = Connection::open(d.path().join("store.sqlite3")).unwrap();
        c.pragma_update(None, "user_version", version).unwrap();
        drop(c);
        let before = fs::read(d.path().join("store.sqlite3")).unwrap();
        let b = directory();
        assert!(Store::migrate_recovery_execution(d.path(), b.path()).is_err());
        assert_eq!(fs::read(d.path().join("store.sqlite3")).unwrap(), before);
        assert!(!b.path().join("backup.sqlite3").exists());
    }
}
#[test]
fn explicit_nine_open_never_initializes_or_upgrades_seven() {
    let (d, s, _o, _g) = seven();
    drop(s);
    assert!(matches!(
        Store::open_recovery_execution(d.path()),
        Err(Error::InvalidTransition)
    ));
    assert_eq!(version(&d.path().join("store.sqlite3")), 7);
}
#[test]
fn source_mixed_legacy_ledger_or_execution_objects_is_rejected_before_backup() {
    for sql in [
        "CREATE TABLE recovery_receipts(x)",
        "CREATE TABLE Recovery_receipts(x)",
        "CREATE TABLE recovery_execution_meta(x)",
        "CREATE VIEW recovery_unapproved AS SELECT 1",
    ] {
        let (d, s, _o, _g) = seven();
        s.handle()
            .call(move |db| {
                db.connection.execute_batch(sql)?;
                Ok(())
            })
            .unwrap();
        drop(s);
        let b = directory();
        assert!(matches!(
            Store::migrate_recovery_execution(d.path(), b.path()),
            Err(Error::Corrupt)
        ));
        assert!(!b.path().join("backup.sqlite3").exists());
        assert_eq!(version(&d.path().join("store.sqlite3")), 7);
    }
}
#[test]
fn active_writer_blocks_migration_without_releasing_or_reopening_source() {
    let (d, s, _o, g) = seven();
    let b = directory();
    assert!(matches!(
        Store::migrate_recovery_execution(d.path(), b.path()),
        Err(Error::AlreadyOpen)
    ));
    assert_eq!(s.handle().check_recovery_grant(g.clone()).unwrap(), g);
    assert!(!b.path().join("backup.sqlite3").exists());
}
#[test]
fn schema_tag_alone_and_added_indices_triggers_views_are_not_admitted() {
    for sql in [
        "DROP INDEX recovery_execution_begin_trial",
        "CREATE INDEX recovery_extra ON recovery_execution_receipts(state)",
        "CREATE TRIGGER recovery_extra AFTER INSERT ON recovery_execution_receipts BEGIN SELECT 1; END",
        "CREATE VIEW recovery_extra AS SELECT * FROM recovery_execution_receipts",
        "CREATE VIEW Recovery_extra AS SELECT * FROM recovery_execution_receipts",
        "DROP TABLE recovery_execution_meta",
    ] {
        let (d,s,_o,_g)=nine();s.handle().call(move|db|{db.connection.execute_batch(sql)?;Ok(())}).unwrap();drop(s);
        assert!(matches!(Store::open_recovery_execution(d.path()),Err(Error::Corrupt)));
    }
}
#[test]
fn invalid_meta_model_or_fingerprint_scheme_is_rejected() {
    for column in ["model", "fingerprint_scheme"] {
        let (d, s, _o, _g) = nine();
        s.handle()
            .call(move |db| {
                db.connection
                    .execute_batch("PRAGMA ignore_check_constraints=ON")?;
                db.connection.execute(
                    &format!("UPDATE recovery_execution_meta SET {column}='unapproved'"),
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        drop(s);
        assert!(matches!(
            Store::open_recovery_execution(d.path()),
            Err(Error::Corrupt)
        ));
    }
}
#[test]
fn canonical_ids_pin_and_generations_are_checked_beyond_sql_shape() {
    for sql in [
        "UPDATE recovery_execution_receipts SET request_id='00000000-0000-4000-8000-000000000ABC'",
        "UPDATE recovery_execution_receipts SET owner_generation=owner_generation+1",
        "UPDATE recovery_execution_receipts SET grant_generation=grant_generation+1",
        "UPDATE recovery_execution_receipts SET device_public_key=zeroblob(32)",
    ] {
        let (d, s, _o, g) = nine();
        seed(&s.handle(), &g, 10, "begin_wifi", "reserved", None);
        s.handle()
            .call(move |db| {
                db.connection.execute(sql, [])?;
                Ok(())
            })
            .unwrap();
        drop(s);
        assert!(
            matches!(
                Store::open_recovery_execution(d.path()),
                Err(Error::Corrupt)
            ),
            "{sql}"
        );
    }
}
#[test]
fn confirm_and_cancel_require_actual_begin_binding_not_just_trial_shape() {
    for action in ["confirm_wifi", "cancel_wifi"] {
        let (d, s, _o, g) = nine();
        seed(
            &s.handle(),
            &g,
            10,
            action,
            "reserved",
            Some("a".repeat(64)),
        );
        drop(s);
        assert!(matches!(
            Store::open_recovery_execution(d.path()),
            Err(Error::Corrupt)
        ));
    }
}
#[test]
fn cross_grant_trial_reference_is_rejected_with_real_current_grants() {
    let (d, s, owner, g) = nine();
    let other = s
        .handle()
        .change_recovery_grant(
            owner,
            [3; 32],
            0,
            request(2),
            RecoveryChange::Grant {
                max_trial_ms: 40000,
            },
        )
        .unwrap();
    seed(
        &s.handle(),
        &g,
        10,
        "begin_wifi",
        "ready",
        Some("a".repeat(64)),
    );
    seed(
        &s.handle(),
        &other,
        11,
        "confirm_wifi",
        "reserved",
        Some("a".repeat(64)),
    );
    drop(s);
    assert!(matches!(
        Store::open_recovery_execution(d.path()),
        Err(Error::Corrupt)
    ));
}
#[test]
fn begin_cannot_claim_saved_without_a_confirm_and_conflicting_terminals_reject() {
    let (d, s, _o, g) = nine();
    seed(
        &s.handle(),
        &g,
        10,
        "begin_wifi",
        "saved",
        Some("a".repeat(64)),
    );
    drop(s);
    assert!(matches!(
        Store::open_recovery_execution(d.path()),
        Err(Error::Corrupt)
    ));
    let (d, s, _o, g) = nine();
    seed(
        &s.handle(),
        &g,
        10,
        "begin_wifi",
        "saved",
        Some("a".repeat(64)),
    );
    seed(
        &s.handle(),
        &g,
        11,
        "confirm_wifi",
        "saved",
        Some("a".repeat(64)),
    );
    seed(
        &s.handle(),
        &g,
        12,
        "cancel_wifi",
        "rolled_back",
        Some("a".repeat(64)),
    );
    drop(s);
    assert!(matches!(
        Store::open_recovery_execution(d.path()),
        Err(Error::Corrupt)
    ));
}
#[test]
fn reopen_keeps_ids_trials_terminal_evidence_and_marks_every_nonterminal_unknown() {
    let (d, s, _o, g) = nine();
    let states = [
        "reserved",
        "claimed",
        "starting",
        "connecting",
        "ready",
        "saving",
    ];
    for (n, state) in states.iter().enumerate() {
        seed(
            &s.handle(),
            &g,
            10 + n as u32,
            "begin_wifi",
            state,
            if *state == "reserved" {
                None
            } else {
                Some(format!("{n:064x}"))
            },
        );
    }
    seed(
        &s.handle(),
        &g,
        30,
        "begin_wifi",
        "saved",
        Some("a".repeat(64)),
    );
    seed(
        &s.handle(),
        &g,
        31,
        "confirm_wifi",
        "saved",
        Some("a".repeat(64)),
    );
    seed(
        &s.handle(),
        &g,
        40,
        "begin_wifi",
        "rolled_back",
        Some("b".repeat(64)),
    );
    seed(&s.handle(), &g, 50, "begin_wifi", "rejected", None);
    seed(&s.handle(), &g, 51, "begin_wifi", "unknown", None);
    let before = rows(&s.handle());
    drop(s);
    let s = Store::open_recovery_execution(d.path()).unwrap();
    let after = rows(&s.handle());
    for (old, new) in before.iter().zip(&after) {
        assert_eq!(old.0, new.0);
        assert_eq!(old.2, new.2);
        assert_eq!(
            new.1,
            if states.contains(&old.1.as_str()) {
                "unknown"
            } else {
                old.1.as_str()
            }
        );
    }
    drop(s);
    let s = Store::open_recovery_execution(d.path()).unwrap();
    assert_eq!(rows(&s.handle()), after);
}
#[test]
fn noncancel_capacity_reserves_one_slot_and_total_capacity_never_exceeds_256() {
    let (d, s, _o, g) = nine();
    for n in 0..256 {
        seed(&s.handle(), &g, 1000 + n, "begin_wifi", "reserved", None);
    }
    drop(s);
    assert!(matches!(
        Store::open_recovery_execution(d.path()),
        Err(Error::Corrupt)
    ));
    let (d, s, _o, g) = nine();
    seed(
        &s.handle(),
        &g,
        10,
        "begin_wifi",
        "ready",
        Some("a".repeat(64)),
    );
    for n in 0..256 {
        seed(
            &s.handle(),
            &g,
            1000 + n,
            "cancel_wifi",
            "reserved",
            Some("a".repeat(64)),
        );
    }
    drop(s);
    assert!(matches!(
        Store::open_recovery_execution(d.path()),
        Err(Error::Corrupt)
    ));
}
#[test]
fn real_backup_restore_requires_explicit_nine_and_quarantines_owner_grants() {
    let (_d, s, _o, g) = nine();
    seed(
        &s.handle(),
        &g,
        10,
        "begin_wifi",
        "ready",
        Some("a".repeat(64)),
    );
    let b = directory();
    s.handle().backup(b.path()).unwrap();
    let original = fs::read(b.path().join("backup.sqlite3")).unwrap();
    for grant in [false, true] {
        let target = directory();
        let error = if grant {
            Store::restore_recovery_grants(b.path(), target.path())
        } else {
            Store::restore(b.path(), target.path())
        };
        assert!(matches!(error, Err(Error::NewerSchema)));
        assert!(target.path().join(".restore-pending").exists());
    }
    let target = directory();
    let restored = Store::restore_recovery_execution(b.path(), target.path()).unwrap();
    assert_eq!(rows(&restored.handle())[0].1, "unknown");
    assert!(!target.path().join(".restore-pending").exists());
    assert!(matches!(
        restored.handle().check_recovery_grant(g),
        Err(Error::Forbidden)
    ));
    assert_eq!(fs::read(b.path().join("backup.sqlite3")).unwrap(), original);
}
#[test]
fn corrupt_nine_restore_keeps_source_and_destination_quarantined() {
    let (_d, s, _o, g) = nine();
    seed(
        &s.handle(),
        &g,
        10,
        "confirm_wifi",
        "reserved",
        Some("a".repeat(64)),
    );
    let b = directory();
    s.handle().backup(b.path()).unwrap();
    let original = fs::read(b.path().join("backup.sqlite3")).unwrap();
    let target = directory();
    assert!(matches!(
        Store::restore_recovery_execution(b.path(), target.path()),
        Err(Error::Corrupt)
    ));
    assert!(target.path().join(".restore-pending").exists());
    assert!(Store::open_recovery_execution(target.path()).is_err());
    assert_eq!(fs::read(b.path().join("backup.sqlite3")).unwrap(), original);
}

#[test]
fn valid_capacity_255_writes_and_one_linked_cancel_is_admitted_without_pruning() {
    let (d, s, _o, g) = nine();
    seed(
        &s.handle(),
        &g,
        10,
        "begin_wifi",
        "ready",
        Some("a".repeat(64)),
    );
    for n in 0..254 {
        seed(&s.handle(), &g, 1000 + n, "begin_wifi", "reserved", None);
    }
    seed(
        &s.handle(),
        &g,
        2000,
        "cancel_wifi",
        "reserved",
        Some("a".repeat(64)),
    );
    drop(s);
    let s = Store::open_recovery_execution(d.path()).unwrap();
    let data = rows(&s.handle());
    assert_eq!(data.len(), 256);
    assert!(data.iter().all(|r| r.1 == "unknown"));
    assert!(data
        .iter()
        .any(|r| r.0 == request(2000) && r.2 == Some("a".repeat(64))));
}
