// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use rustsimhub_policy::Kind;
fn dir() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    d
}
fn bound() -> (tempfile::TempDir, Store) {
    let d = dir();
    let s = Store::open(d.path()).unwrap();
    s.handle().bind_identity([1; 32]).unwrap();
    s.handle().bootstrap_owner([2; 32]).unwrap();
    (d, s)
}
#[test]
fn source_is_real_schema_and_realm_and_hold_never_opens_a_second_writer() {
    let (d, s) = bound();
    let h = s.handle();
    let realm = h.cursor().unwrap().db_epoch;
    let lease = h
        .update_health_lease(Instant::now() + Duration::from_secs(2), false)
        .unwrap();
    assert_eq!(lease.snapshot().schema, 6);
    assert_eq!(lease.snapshot().realm, realm);
    assert_eq!(lease.snapshot().owner.kind, Kind::Owner);
    assert!(!lease.snapshot().forwarding.enabled);
    assert!(Store::open(d.path()).is_err());
    lease.revalidate().unwrap();
    lease.release().unwrap();
    assert_eq!(h.cursor().unwrap().db_epoch, realm);
}
#[test]
fn actual_schema_nine_is_never_reported_as_six() {
    let (_d, s) = bound();
    let h = s.handle();
    h.call(|db| {
        db.connection.pragma_update(None, "user_version", 9)?;
        Ok(())
    })
    .unwrap();
    let lease = h
        .update_health_lease(Instant::now() + Duration::from_secs(1), false)
        .unwrap();
    assert_eq!(lease.snapshot().schema, 9);
    lease.release().unwrap();
}
#[test]
fn revoked_owner_missing_identity_and_unknown_forwarding_are_not_healthy() {
    let (d, s) = bound();
    let h = s.handle();
    h.call(|db| {
        db.connection
            .execute("UPDATE principals SET active=0 WHERE kind='owner'", [])?;
        Ok(())
    })
    .unwrap();
    assert!(h
        .update_health_lease(Instant::now() + Duration::from_secs(1), false)
        .is_err());
    h.call(|db| {
        db.connection
            .execute("UPDATE principals SET active=1 WHERE kind='owner'", [])?;
        Ok(())
    })
    .unwrap();
    fs::create_dir(d.path().join("forwarding")).unwrap();
    assert!(h
        .update_health_lease(Instant::now() + Duration::from_secs(1), false)
        .is_err());
}
#[test]
fn external_sqlite_generation_change_invalidates_original_live_lease() {
    let (d, s) = bound();
    let h = s.handle();
    let lease = h
        .update_health_lease(Instant::now() + Duration::from_secs(2), false)
        .unwrap();
    let other = Connection::open(d.path().join("store.sqlite3")).unwrap();
    other
        .execute(
            "UPDATE meta SET db_epoch=? WHERE singleton=1",
            [Uuid::new_v4().to_string()],
        )
        .unwrap();
    assert!(lease.revalidate().is_err());
    drop(lease);
    assert!(h.cursor().is_ok());
}
#[test]
fn writer_lock_replacement_and_pending_restore_invalidate_the_same_actor_lease() {
    for pending in [false, true] {
        let (d, s) = bound();
        let h = s.handle();
        let lease = h
            .update_health_lease(Instant::now() + Duration::from_secs(2), false)
            .unwrap();
        if pending {
            fs::write(d.path().join(".restore-pending"), b"restore").unwrap();
        } else {
            fs::rename(
                d.path().join(".writer.lock"),
                d.path().join(".old-writer.lock"),
            )
            .unwrap();
            fs::write(d.path().join(".writer.lock"), b"").unwrap();
        }
        assert!(lease.revalidate().is_err());
        drop(lease);
    }
}
