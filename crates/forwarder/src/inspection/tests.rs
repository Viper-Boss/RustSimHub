// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::{Binding, OutboxCompatibilityAssessment as Assessment, Queue};
use std::os::unix::fs::{symlink, PermissionsExt};
use tempfile::TempDir;

fn binding() -> Binding {
    Binding {
        device_key: [1; 32],
        realm: "00000000-0000-4000-8000-000000000001".into(),
        owner_key: [2; 32],
        owner_generation: 1,
    }
}

fn fixture() -> TempDir {
    let d = TempDir::new().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    drop(Queue::open(d.path(), binding()).unwrap());
    d
}

fn snapshot(d: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<_> = fs::read_dir(d)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (
                e.file_name().to_str().unwrap().to_owned(),
                fs::read(e.path()).unwrap(),
            )
        })
        .collect();
    files.sort();
    files
}

#[test]
fn actual_outbox_inspection_never_recovers_in_flight_or_changes_unknown_payloads() {
    let d = fixture();
    let db = Connection::open(d.path().join("outbox.sqlite3")).unwrap();
    for (id, state) in [
        ("one", "in_flight"),
        ("two", "unknown"),
        ("three", "confirmed"),
    ] {
        db.execute("INSERT INTO jobs(id,message_id,target_id,rule_id,generation,line,state,attempt_id,request) VALUES(?1,?1,'target','rule',1,1,?2,'attempt','synthetic private immutable payload')", (id,state)).unwrap();
    }
    drop(db);
    let before = snapshot(d.path());
    let probe = inspect_outbox(d.path()).unwrap();
    assert_eq!(probe.assessment().unwrap(), Assessment::ReadWrite);
    assert!(probe._connection.execute("DELETE FROM jobs", []).is_err());
    assert_eq!(snapshot(d.path()), before);
    drop(probe);
    assert_eq!(snapshot(d.path()), before);
    let db = Connection::open(d.path().join("outbox.sqlite3")).unwrap();
    assert_eq!(
        db.query_row::<String, _, _>("SELECT state FROM jobs WHERE id='one'", [], |r| r.get(0))
            .unwrap(),
        "in_flight"
    );
    assert_eq!(
        db.query_row::<i64, _, _>(
            "SELECT count(*) FROM jobs WHERE request='synthetic private immutable payload'",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        3
    );
}

#[test]
fn actual_old_formats_require_migration_but_inspection_does_not_migrate() {
    for version in [1, 2] {
        let d = fixture();
        let db = Connection::open(d.path().join("outbox.sqlite3")).unwrap();
        if version == 1 {
            db.execute_batch("ALTER TABLE jobs DROP COLUMN purpose;")
                .unwrap();
        }
        db.execute_batch("DROP TABLE history_batches;").unwrap();
        db.pragma_update(None, "user_version", version).unwrap();
        drop(db);
        let before = snapshot(d.path());
        let probe = inspect_outbox(d.path()).unwrap();
        assert_eq!(probe.version().format, version);
        assert_eq!(probe.assessment().unwrap(), Assessment::MigrationRequired);
        drop(probe);
        assert_eq!(snapshot(d.path()), before);
    }
}

#[test]
fn real_writer_lock_is_mutually_exclusive_in_both_directions() {
    let d = fixture();
    let queue = Queue::open(d.path(), binding()).unwrap();
    assert!(matches!(inspect_outbox(d.path()), Err(Error::Busy)));
    drop(queue);
    let probe = inspect_outbox(d.path()).unwrap();
    assert!(matches!(Queue::open(d.path(), binding()), Err(Error::Busy)));
    drop(probe);
    assert!(Queue::open(d.path(), binding()).is_ok());
}

#[test]
fn missing_lock_database_and_directory_never_initialize_anything() {
    let d = fixture();
    for name in [".writer.lock", "outbox.sqlite3"] {
        let path = d.path().join(name);
        let bytes = fs::read(&path).unwrap();
        fs::remove_file(&path).unwrap();
        let before = snapshot(d.path());
        assert!(inspect_outbox(d.path()).is_err());
        assert_eq!(snapshot(d.path()), before);
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    assert!(inspect_outbox(&d.path().join("absent")).is_err());
    assert!(!d.path().join("absent").exists());
    assert!(inspect_outbox(Path::new("relative")).is_err());
}

#[test]
fn all_sidecars_including_empty_files_and_dangling_links_require_maintenance() {
    let d = fixture();
    for name in SIDE_CARS {
        let path = d.path().join(name);
        for bytes in [b"".as_slice(), b"synthetic journal".as_slice()] {
            fs::write(&path, bytes).unwrap();
            let before = snapshot(d.path());
            assert!(matches!(inspect_outbox(d.path()), Err(Error::Conflict)));
            assert_eq!(snapshot(d.path()), before);
        }
        fs::remove_file(&path).unwrap();
        symlink(d.path().join("missing"), &path).unwrap();
        assert!(matches!(inspect_outbox(d.path()), Err(Error::Conflict)));
        assert!(fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink());
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn foreign_and_future_formats_are_reported_without_relabeling_the_database() {
    for (app, format, expected) in [
        (0, 3, Assessment::ForeignApplication),
        (
            crate::OUTBOX_APPLICATION_ID,
            0,
            Assessment::UnsupportedFormat,
        ),
        (
            crate::OUTBOX_APPLICATION_ID,
            4,
            Assessment::UnsupportedFormat,
        ),
        (
            crate::OUTBOX_APPLICATION_ID,
            -1,
            Assessment::UnsupportedFormat,
        ),
    ] {
        let d = fixture();
        let db = Connection::open(d.path().join("outbox.sqlite3")).unwrap();
        db.pragma_update(None, "application_id", app).unwrap();
        db.pragma_update(None, "user_version", format).unwrap();
        drop(db);
        let before = snapshot(d.path());
        let probe = inspect_outbox(d.path()).unwrap();
        assert_eq!(probe.assessment().unwrap(), expected);
        drop(probe);
        assert_eq!(snapshot(d.path()), before);
    }
}

#[test]
fn symlinks_hardlinks_unsafe_permissions_and_oversized_files_are_refused() {
    let d = fixture();
    let alias = d.path().join("directory-link");
    symlink(d.path(), &alias).unwrap();
    assert!(inspect_outbox(&alias).is_err());
    fs::remove_file(alias).unwrap();
    for name in [".writer.lock", "outbox.sqlite3"] {
        let path = d.path().join(name);
        let saved = d.path().join("saved");
        fs::rename(&path, &saved).unwrap();
        symlink(&saved, &path).unwrap();
        assert!(inspect_outbox(d.path()).is_err());
        fs::remove_file(&path).unwrap();
        fs::hard_link(&saved, &path).unwrap();
        assert!(inspect_outbox(d.path()).is_err());
        fs::remove_file(&path).unwrap();
        fs::rename(saved, &path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        assert!(inspect_outbox(d.path()).is_err());
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let db = OpenOptions::new()
        .write(true)
        .open(d.path().join("outbox.sqlite3"))
        .unwrap();
    db.set_len(32 * 1024 * 1024 + 1).unwrap();
    assert!(matches!(inspect_outbox(d.path()), Err(Error::Forbidden)));
}

#[test]
fn corrupt_and_wal_databases_are_not_repaired_or_switched_to_delete() {
    let d = fixture();
    let path = d.path().join("outbox.sqlite3");
    let original = fs::read(&path).unwrap();
    fs::write(&path, vec![0x55; 4096]).unwrap();
    let before = snapshot(d.path());
    assert!(inspect_outbox(d.path()).is_err());
    assert_eq!(snapshot(d.path()), before);
    fs::write(&path, original).unwrap();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
    drop(db);
    let before = snapshot(d.path());
    assert!(matches!(inspect_outbox(d.path()), Err(Error::Conflict)));
    assert_eq!(snapshot(d.path()), before);
}

#[test]
fn sqlite_vm_work_budget_interrupts_an_actual_unbounded_recursive_query() {
    let db = Connection::open_in_memory().unwrap();
    query_budget(&db, Duration::from_secs(3), 1000).unwrap();
    let error = db
        .query_row::<i64, _, _>(
            "WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n) SELECT sum(x) FROM n",
            [],
            |r| r.get(0),
        )
        .unwrap_err();
    assert_eq!(
        error.sqlite_error_code(),
        Some(rusqlite::ErrorCode::OperationInterrupted)
    );
}
