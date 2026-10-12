// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn recovery_commit_failure_cannot_activate_owner_or_clients() {
    let d = directory();
    let s = Store::open(d.path()).unwrap();
    let h = s.handle();
    let o = h.bootstrap_owner([21; 32]).unwrap();
    h.approve_client(o, [22; 32], rustsimhub_policy::Kind::Nas, 1)
        .unwrap();
    let backup = directory();
    h.backup(backup.path()).unwrap();
    let target = directory();
    let r = Store::restore(backup.path(), target.path()).unwrap();
    let h = r.handle();
    let owner = h.principal([21; 32]).unwrap().unwrap();
    h.call(|db| {db.connection.execute_batch("CREATE TEMP TRIGGER fail_recovery AFTER UPDATE ON main.principals BEGIN SELECT RAISE(ABORT,'synthetic'); END;")?;Ok(())}).unwrap();
    assert!(h.recover_owner(owner.clone()).is_err());
    assert!(!h.principal([21; 32]).unwrap().unwrap().active);
    assert_eq!(h.principal([21; 32]).unwrap().unwrap(), owner);
    h.call(|db| {
        db.connection.execute_batch("DROP TRIGGER fail_recovery;")?;
        Ok(())
    })
    .unwrap();
    let mut stale = owner.clone();
    stale.generation += 1;
    assert!(h.recover_owner(stale).is_err());
    assert!(h.recover_owner(owner).unwrap().active);
    assert!(!h.principal([22; 32]).unwrap().unwrap().active);
}

#[test]
fn schema_one_migration_preserves_operations_and_cursors() {
    let d = directory();
    let path = d.path().join("store.sqlite3");
    private_file(&path, true).unwrap();
    let c = Connection::open(&path).unwrap();
    c.execute_batch(include_str!("schema.sql")).unwrap();
    c.execute("INSERT INTO meta VALUES(1,'old-epoch',1)", [])
        .unwrap();
    c.execute("INSERT INTO sms_operations VALUES('old','client','key',1,zeroblob(32),'+12025550123','synthetic','prepared')",[]).unwrap();
    c.execute(
        "INSERT INTO events(operation_id,line_epoch,kind) VALUES('old',1,'prepared')",
        [],
    )
    .unwrap();
    drop(c);
    let s = Store::open(d.path()).unwrap();
    assert_eq!(
        s.handle().operation("old", "client").unwrap().state,
        SmsState::Prepared
    );
    assert_eq!(
        s.handle().cursor().unwrap(),
        Cursor {
            db_epoch: "old-epoch".into(),
            seq: "1".into()
        }
    );
    assert!(s.handle().principal([1; 32]).unwrap().is_none());
}

#[test]
fn failed_owner_commit_never_acknowledges_or_leaves_authority() {
    let d = directory();
    let s = Store::open(d.path()).unwrap();
    let h = s.handle();
    h.call(|db| {db.connection.execute_batch("CREATE TEMP TRIGGER fail_authority AFTER INSERT ON main.principals BEGIN SELECT RAISE(ABORT,'synthetic'); END;")?;Ok(())}).unwrap();
    assert!(h.bootstrap_owner([1; 32]).is_err());
    assert!(h.principal([1; 32]).unwrap().is_none());
    h.call(|db| {
        db.connection
            .execute_batch("DROP TRIGGER fail_authority;")?;
        Ok(())
    })
    .unwrap();
    let owner = h.bootstrap_owner([1; 32]).unwrap();
    assert!(h.bootstrap_owner([2; 32]).is_err());
    assert!(h
        .approve_client(owner.clone(), [2; 32], rustsimhub_policy::Kind::Owner, 1)
        .is_err());
    h.approve_client(owner.clone(), [2; 32], rustsimhub_policy::Kind::Nas, 1)
        .unwrap();
    let invalid = rustsimhub_policy::Principal {
        generation: owner.generation + 1,
        ..owner.clone()
    };
    assert!(h.revoke_client(invalid, [2; 32]).is_err());
    assert!(h.principal([2; 32]).unwrap().unwrap().active);
    assert!(h.revoke_client(owner, [1; 32]).is_err());
}

fn directory() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    }
    dir
}
fn request(key: &str) -> SmsRequest {
    SmsRequest {
        client_id: "test-client".into(),
        idempotency_key: key.into(),
        line: LineEpoch(1),
        recipient: "+12025550123".into(),
        text: "synthetic message".into(),
    }
}
fn start_cursor(handle: &Handle) -> Cursor {
    let mut cursor = handle.cursor().unwrap();
    cursor.seq = "0".into();
    cursor
}

#[test]
fn concurrent_retries_commit_one_operation_and_one_event() {
    let dir = directory();
    let store = Store::open(dir.path()).unwrap();
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let handle = store.handle();
            thread::spawn(move || handle.prepare_sms(request("same")).unwrap())
        })
        .collect();
    let operations: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert!(operations.iter().all(|op| op == &operations[0]));
    let handle = store.handle();
    assert_eq!(
        handle
            .events(start_cursor(&handle), 256)
            .unwrap()
            .events
            .len(),
        1
    );
    let mut changed = request("same");
    changed.text = "different synthetic message".into();
    assert_eq!(handle.prepare_sms(changed), Err(Error::Conflict));
    let mut other = request("same");
    other.client_id = "other-client".into();
    assert_ne!(handle.prepare_sms(other).unwrap().id, operations[0].id);
}

#[test]
fn client_scope_and_duplicate_submission_are_enforced() {
    let dir = directory();
    let store = Store::open(dir.path()).unwrap();
    let handle = store.handle();
    let op = handle.prepare_sms(request("one")).unwrap();
    assert!(matches!(
        handle.begin_submit(&op.id, "wrong-client"),
        Err(Error::Forbidden)
    ));
    handle.begin_submit(&op.id, "test-client").unwrap();
    assert!(matches!(
        handle.begin_submit(&op.id, "test-client"),
        Err(Error::InvalidTransition)
    ));
    handle.mark_submitted(&op.id, "test-client").unwrap();
    assert_eq!(
        handle.mark_submitted(&op.id, "test-client"),
        Err(Error::InvalidTransition)
    );
}

#[test]
fn event_failure_rolls_back_state_and_never_returns_a_send_permit() {
    let dir = directory();
    let store = Store::open(dir.path()).unwrap();
    let handle = store.handle();
    let op = handle.prepare_sms(request("one")).unwrap();
    let before = handle.cursor().unwrap();
    handle.call(|db| { db.connection.execute_batch("CREATE TEMP TRIGGER fail_event BEFORE INSERT ON events BEGIN SELECT RAISE(ABORT,'synthetic fault'); END;")?; Ok(()) }).unwrap();
    assert!(handle.begin_submit(&op.id, "test-client").is_err());
    assert_eq!(
        handle.operation(&op.id, "test-client").unwrap().state,
        SmsState::Prepared
    );
    assert_eq!(handle.cursor().unwrap(), before);
}

#[test]
fn sim_switch_fences_pending_and_inflight_operations_persistently() {
    let dir = directory();
    let store = Store::open(dir.path()).unwrap();
    let handle = store.handle();
    let queued = handle.prepare_sms(request("queued")).unwrap();
    let inflight = handle.prepare_sms(request("inflight")).unwrap();
    handle.begin_submit(&inflight.id, "test-client").unwrap();
    handle.change_line(LineEpoch(2)).unwrap();
    assert_eq!(
        handle.operation(&queued.id, "test-client").unwrap().state,
        SmsState::Cancelled
    );
    assert_eq!(
        handle.operation(&inflight.id, "test-client").unwrap().state,
        SmsState::Unknown
    );
    assert!(handle.mark_submitted(&inflight.id, "test-client").is_err());
    assert_eq!(handle.change_line(LineEpoch(1)), Err(Error::StaleLine));
    // Old retries return the fenced old operation; a changed line with same key conflicts.
    assert_eq!(
        handle.prepare_sms(request("queued")).unwrap().state,
        SmsState::Cancelled
    );
    let mut next = request("queued");
    next.line = LineEpoch(2);
    assert_eq!(handle.prepare_sms(next), Err(Error::Conflict));
    drop(store);
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(
        reopened.handle().prepare_sms(request("new-old-line")),
        Err(Error::StaleLine)
    );
}

#[test]
fn backup_restore_preserves_data_and_rotates_event_epoch() {
    let source = directory();
    let backup = directory();
    let target = directory();
    let store = Store::open(source.path()).unwrap();
    let handle = store.handle();
    let op = handle.prepare_sms(request("backup")).unwrap();
    let old = handle.cursor().unwrap();
    handle.backup(backup.path()).unwrap();
    assert_eq!(handle.backup(backup.path()), Err(Error::BackupExists));
    let restored = Store::restore(backup.path(), target.path()).unwrap();
    let reader = restored.handle();
    let restored_op = reader.operation(&op.id, "test-client").unwrap();
    assert_eq!(restored_op.id, op.id);
    assert_eq!(restored_op.state, SmsState::Unknown);
    assert!(matches!(
        reader.begin_submit(&op.id, "test-client"),
        Err(Error::InvalidTransition)
    ));
    assert_ne!(reader.cursor().unwrap().db_epoch, old.db_epoch);
    assert!(matches!(reader.events(old, 10), Err(Error::ResyncRequired)));
    assert!(matches!(
        Store::restore(backup.path(), target.path()),
        Err(Error::AlreadyOpen)
    ));
}

#[test]
fn second_writer_cannot_trigger_recovery_of_active_submission() {
    let dir = directory();
    let store = Store::open(dir.path()).unwrap();
    let handle = store.handle();
    let op = handle.prepare_sms(request("active")).unwrap();
    handle.begin_submit(&op.id, "test-client").unwrap();
    assert!(matches!(Store::open(dir.path()), Err(Error::AlreadyOpen)));
    assert_eq!(
        handle.operation(&op.id, "test-client").unwrap().state,
        SmsState::Submitting
    );
    drop(store);
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(
        reopened
            .handle()
            .operation(&op.id, "test-client")
            .unwrap()
            .state,
        SmsState::Unknown
    );
    assert_eq!(handle.cursor().unwrap_err(), Error::Stopped);
}

#[test]
fn cursor_cannot_skip_history_or_overflow() {
    let dir = directory();
    let store = Store::open(dir.path()).unwrap();
    let handle = store.handle();
    let mut cursor = handle.cursor().unwrap();
    cursor.seq = "1".into();
    assert!(matches!(
        handle.events(cursor, 10),
        Err(Error::ResyncRequired)
    ));
    let mut cursor = handle.cursor().unwrap();
    cursor.seq = u64::MAX.to_string();
    assert!(matches!(
        handle.events(cursor, 10),
        Err(Error::InvalidInput)
    ));
    assert!(matches!(
        handle.events(start_cursor(&handle), 257),
        Err(Error::InvalidInput)
    ));
}

#[test]
fn newer_or_foreign_database_is_preserved_and_rejected() {
    let dir = directory();
    {
        let connection = Connection::open(dir.path().join("store.sqlite3")).unwrap();
        connection.execute_batch("PRAGMA user_version=99;").unwrap();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            dir.path().join("store.sqlite3"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
    }
    let before = fs::read(dir.path().join("store.sqlite3")).unwrap();
    assert!(matches!(Store::open(dir.path()), Err(Error::NewerSchema)));
    assert_eq!(fs::read(dir.path().join("store.sqlite3")).unwrap(), before);
    let foreign = directory();
    {
        let connection = Connection::open(foreign.path().join("store.sqlite3")).unwrap();
        connection
            .execute_batch("CREATE TABLE foreign_data(secret TEXT);")
            .unwrap();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            foreign.path().join("store.sqlite3"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
    }
    assert!(matches!(
        Store::open(foreign.path()),
        Err(Error::ForeignDatabase)
    ));
}

#[test]
fn corrupt_database_is_not_recreated() {
    let dir = directory();
    let path = dir.path().join("store.sqlite3");
    let mut file = private_file(&path, true).unwrap();
    use std::io::Write;
    file.write_all(b"synthetic broken database").unwrap();
    drop(file);
    assert!(matches!(Store::open(dir.path()), Err(Error::Corrupt)));
    assert_eq!(fs::read(path).unwrap(), b"synthetic broken database");
}

#[test]
fn full_database_never_acknowledges_partial_operation_or_event() {
    let dir = directory();
    let store = Store::open(dir.path()).unwrap();
    let handle = store.handle();
    handle
        .call(|db| {
            db.connection.execute_batch("PRAGMA max_page_count=32;")?;
            Ok(())
        })
        .unwrap();
    let mut encountered = false;
    for index in 0..100 {
        let before = handle.cursor().unwrap();
        let mut sms = request(&format!("full-{index}"));
        sms.text = "x".repeat(4096);
        match handle.prepare_sms(sms) {
            Ok(_) => {}
            Err(Error::Full) => {
                assert_eq!(handle.cursor().unwrap(), before);
                encountered = true;
                break;
            }
            Err(error) => panic!("unexpected storage category: {error}"),
        }
    }
    assert!(encountered);
}

#[test]
fn crash_worker() {
    let Ok(path) = std::env::var("RUSTSIMHUB_TEST_CRASH_DIRECTORY") else {
        return;
    };
    let mode = std::env::var("RUSTSIMHUB_TEST_CRASH_MODE").unwrap();
    let store = Store::open(path).unwrap();
    let handle = store.handle();
    let op = handle.prepare_sms(request("crash")).unwrap();
    if mode == "committed" {
        handle.begin_submit(&op.id, "test-client").unwrap();
        std::process::exit(73); // No Rust drops or SQLite close/checkpoint.
    }
    handle
        .call(move |db| -> Result<()> {
            db.connection.execute_batch("BEGIN IMMEDIATE;")?;
            db.connection.execute(
                "UPDATE sms_operations SET state='submitting' WHERE id=?",
                [op.id],
            )?;
            std::process::exit(74); // Kill-equivalent with uncommitted state.
        })
        .unwrap();
}

#[test]
fn process_exit_recovers_committed_uncertainty_and_rolls_back_uncommitted_state() {
    for (mode, code, expected) in [
        ("committed", 73, SmsState::Unknown),
        ("uncommitted", 74, SmsState::Prepared),
    ] {
        let dir = directory();
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::crash_worker", "--nocapture"])
            .env("RUSTSIMHUB_TEST_CRASH_DIRECTORY", dir.path())
            .env("RUSTSIMHUB_TEST_CRASH_MODE", mode)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(code));
        let store = Store::open(dir.path()).unwrap();
        let handle = store.handle();
        let op = handle.prepare_sms(request("crash")).unwrap();
        assert_eq!(op.state, expected);
        if expected == SmsState::Unknown {
            assert!(matches!(
                handle.begin_submit(&op.id, "test-client"),
                Err(Error::InvalidTransition)
            ));
        }
        let events = handle.events(start_cursor(&handle), 256).unwrap().events;
        assert_eq!(events.len(), if mode == "committed" { 3 } else { 1 });
    }
}

#[cfg(target_os = "linux")]
#[test]
fn permissive_directory_and_symlink_database_are_rejected() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = directory();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(Store::open(dir.path()), Err(Error::Forbidden)));
    let private = directory();
    let target = directory();
    let file = target.path().join("protected");
    fs::write(&file, b"evidence").unwrap();
    symlink(&file, private.path().join("store.sqlite3")).unwrap();
    assert!(matches!(Store::open(private.path()), Err(Error::Forbidden)));
    assert_eq!(fs::read(file).unwrap(), b"evidence");
}

#[test]
fn malformed_recipient_never_enters_queue() {
    let dir = directory();
    let store = Store::open(dir.path()).unwrap();
    let handle = store.handle();
    let mut sms = request("bad");
    sms.recipient = "+++123".into();
    assert_eq!(handle.prepare_sms(sms), Err(Error::InvalidInput));
    assert_eq!(handle.cursor().unwrap().seq, "0");
}

#[test]
fn interrupted_restore_cannot_revive_a_previously_sent_operation() {
    let source = directory();
    let backup = directory();
    let target = directory();
    let store = Store::open(source.path()).unwrap();
    let handle = store.handle();
    let op = handle.prepare_sms(request("restore-race")).unwrap();
    let old_epoch = handle.cursor().unwrap().db_epoch;
    handle.backup(backup.path()).unwrap();
    handle.begin_submit(&op.id, "test-client").unwrap();
    handle.mark_submitted(&op.id, "test-client").unwrap();
    private_file(&target.path().join(".restore-pending"), true).unwrap();
    let output = target.path().join("store.sqlite3");
    private_file(&output, true).unwrap();
    fs::copy(backup.path().join("backup.sqlite3"), &output).unwrap();
    let restored = Store::open(target.path()).unwrap();
    assert_eq!(
        restored
            .handle()
            .operation(&op.id, "test-client")
            .unwrap()
            .state,
        SmsState::Unknown
    );
    assert_ne!(restored.handle().cursor().unwrap().db_epoch, old_epoch);
    assert!(!target.path().join(".restore-pending").exists());
}

#[test]
fn empty_partial_restore_is_rejected_without_overwriting_evidence() {
    let target = directory();
    private_file(&target.path().join(".restore-pending"), true).unwrap();
    private_file(&target.path().join("store.sqlite3"), true).unwrap();
    assert!(matches!(Store::open(target.path()), Err(Error::Corrupt)));
    assert!(target.path().join(".restore-pending").exists());
    assert_eq!(
        fs::metadata(target.path().join("store.sqlite3"))
            .unwrap()
            .len(),
        0
    );
}
