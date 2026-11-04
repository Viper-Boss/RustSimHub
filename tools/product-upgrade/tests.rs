// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
fn fixture() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let s = rustsimhub_storage::Store::open(d.path()).unwrap();
    s.handle().bind_identity([9; 32]).unwrap();
    s.handle().bootstrap_owner([1; 32]).unwrap();
    drop(s);
    let c = rusqlite::Connection::open(d.path().join("store.sqlite3")).unwrap();
    c.execute("INSERT INTO sms_operations VALUES('00000000-0000-4000-8000-000000000001','00000000-0000-4000-8000-000000000002','synthetic-idempotency',1,?,'+10000000000','SYNTHETIC BODY','unknown')",[&[7u8;32][..]]).unwrap();
    drop(c);
    d
}
fn protected(d: &Path) -> String {
    let c = rusqlite::Connection::open_with_flags(
        format!("file:{}?immutable=1", d.join("store.sqlite3").display()),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap();
    let meta: (String, i64) = c
        .query_row("SELECT db_epoch,line_epoch FROM meta", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    let op: (String, String, String) = c
        .query_row("SELECT id,body,state FROM sms_operations", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    let owner: (Vec<u8>, i64) = c
        .query_row(
            "SELECT public_key,generation FROM principals WHERE kind='owner'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    digest(&serde_json::to_vec(&(meta, op, owner)).unwrap())
}
fn permit(d: &Path) -> Permit {
    let (schema, hash, binding) = observed(d).unwrap();
    Permit {
        format: 1,
        source_schema: schema,
        target_schema: 9,
        source_sha256: hash,
        binding_sha256: binding,
        request_id: "b".repeat(64),
        deadline_ms: boot_ms().unwrap() + 30_000,
    }
}
#[test]
fn real_original_six_to_seven_to_nine_preserves_protected_rows_and_rejects_reexecution() {
    let d = fixture();
    let before = protected(d.path());
    let p = permit(d.path());
    execute(d.path(), &p, || Ok(()), |_| Ok(())).unwrap();
    assert_eq!(protected(d.path()), before);
    assert_eq!(inspect_at(d.path()).unwrap()["state"], "committed");
    assert_eq!(observed(d.path()).unwrap().0, 9);
    assert!(rustsimhub_storage::Store::open(d.path()).is_err());
    let bytes = fs::read(d.path().join("store.sqlite3")).unwrap();
    assert!(execute(d.path(), &p, || Ok(()), |_| Ok(())).is_err());
    assert_eq!(fs::read(d.path().join("store.sqlite3")).unwrap(), bytes);
    assert!(d
        .path()
        .join("product-upgrade/before-grants/backup.sqlite3")
        .is_file());
    assert!(d
        .path()
        .join("product-upgrade/before-execution/backup.sqlite3")
        .is_file());
}
#[test]
fn interrupted_phases_are_reported_without_restoring_or_resubmitting_data() {
    for stop in [
        "prepared",
        "grants-commit-before-journal",
        "grants-committed",
        "execution-commit-before-journal",
    ] {
        let d = fixture();
        let before = protected(d.path());
        let p = permit(d.path());
        assert!(execute(
            d.path(),
            &p,
            || Ok(()),
            |phase| if phase == stop {
                Err(Error::Unknown)
            } else {
                Ok(())
            }
        )
        .is_err());
        assert_eq!(protected(d.path()), before);
        let state = inspect_at(d.path()).unwrap();
        assert_eq!(
            state["state"],
            match stop {
                "prepared" => "not_started",
                "grants-committed" => "grants_committed",
                _ => "unknown",
            }
        );
        let data = fs::read(d.path().join("store.sqlite3")).unwrap();
        assert!(execute(d.path(), &p, || Ok(()), |_| Ok(())).is_err());
        assert_eq!(fs::read(d.path().join("store.sqlite3")).unwrap(), data);
    }
}
#[test]
fn wrong_key_realm_source_busy_and_cancelled_commit_leave_six_bytes_unchanged() {
    for case in [
        "source", "binding", "deadline", "target", "busy", "cancel", "link",
    ] {
        let d = fixture();
        let mut p = permit(d.path());
        let before = fs::read(d.path().join("store.sqlite3")).unwrap();
        match case {
            "source" => p.source_sha256 = "c".repeat(64),
            "binding" => p.binding_sha256 = "c".repeat(64),
            "deadline" => p.deadline_ms = 0,
            "target" => p.target_schema = 6,
            _ => (),
        }
        let lock = File::open(d.path().join(".writer.lock")).unwrap();
        if case == "busy" {
            lock.try_lock_exclusive().unwrap();
        }
        if case == "link" {
            fs::rename(
                d.path().join("store.sqlite3"),
                d.path().join("saved.sqlite3"),
            )
            .unwrap();
            std::os::unix::fs::symlink("saved.sqlite3", d.path().join("store.sqlite3")).unwrap();
        }
        assert!(execute(
            d.path(),
            &p,
            || if case == "cancel" {
                Err(Error::Denied)
            } else {
                Ok(())
            },
            |_| Ok(())
        )
        .is_err());
        assert_eq!(fs::read(d.path().join("store.sqlite3")).unwrap(), before);
        assert!(!d.path().join("product-upgrade").exists());
    }
}
#[test]
fn revoked_recovery_grant_stays_revoked_through_seven_to_nine() {
    let d = fixture();
    let backup = tempfile::tempdir().unwrap();
    fs::set_permissions(backup.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let s = rustsimhub_storage::Store::migrate_recovery_grants(d.path(), backup.path()).unwrap();
    let owner = s.handle().principals().unwrap().into_iter().next().unwrap();
    let grant = s
        .handle()
        .change_recovery_grant(
            owner.clone(),
            [2; 32],
            0,
            "00000000-0000-4000-8000-000000000003".into(),
            rustsimhub_storage::RecoveryChange::Grant {
                max_trial_ms: 40000,
            },
        )
        .unwrap();
    s.handle()
        .change_recovery_grant(
            owner,
            [2; 32],
            1,
            "00000000-0000-4000-8000-000000000004".into(),
            rustsimhub_storage::RecoveryChange::Revoke,
        )
        .unwrap();
    assert!(s.handle().check_recovery_grant(grant.clone()).is_err());
    drop(s);
    // The schema7 migration keeps existing Owner/data; execution migration must
    // preserve its grant catalog rather than recreating credentials or realms.
    let c = rusqlite::Connection::open_with_flags(
        format!(
            "file:{}?immutable=1",
            d.path().join("store.sqlite3").display()
        ),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap();
    let schema: String = c
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='recovery_grants'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    drop(c);
    let before = protected(d.path());
    let p = permit(d.path());
    assert_eq!(p.source_schema, 7);
    execute(d.path(), &p, || Ok(()), |_| Ok(())).unwrap();
    assert_eq!(protected(d.path()), before);
    let migrated = rustsimhub_storage::Store::open_recovery_execution(d.path()).unwrap();
    assert!(migrated.handle().check_recovery_grant(grant).is_err());
    drop(migrated);
    let c = rusqlite::Connection::open_with_flags(
        format!(
            "file:{}?immutable=1",
            d.path().join("store.sqlite3").display()
        ),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap();
    assert_eq!(
        c.query_row(
            "SELECT sql FROM sqlite_master WHERE name='recovery_grants'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        schema
    );
}
