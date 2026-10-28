// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use tempfile::TempDir;

fn directory() -> TempDir {
    let dir = TempDir::new().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    }
    dir
}
fn key(n: u8) -> RecoveryKey {
    RecoveryKey {
        profile: "test-key-v1".into(),
        public_key: vec![n; 33],
    }
}
fn limits() -> RecoveryLimits {
    RecoveryLimits {
        session_ms: 120_000,
        request_budget: 64,
    }
}
fn fixture() -> (TempDir, Store, Principal) {
    let dir = directory();
    let store = Store::open_recovery(dir.path()).unwrap();
    let owner = store.handle().bootstrap_owner([9; 32]).unwrap();
    (dir, store, owner)
}

#[test]
fn migration_is_explicit_and_the_normal_schema_six_reader_refuses_seven() {
    let dir = directory();
    let store = Store::open(dir.path()).unwrap();
    let h = store.handle();
    let owner = h.bootstrap_owner([9; 32]).unwrap();
    let before = h.cursor().unwrap();
    assert!(!h.recovery_enabled().unwrap());
    assert!(matches!(
        h.grant_recovery(owner.clone(), key(1), limits(), 0),
        Err(Error::InvalidTransition)
    ));
    drop(store);
    let store = Store::open_recovery(dir.path()).unwrap();
    let h = store.handle();
    assert!(h.recovery_enabled().unwrap());
    assert_eq!(h.cursor().unwrap().db_epoch, before.db_epoch);
    assert_eq!(h.principal(owner.public_key).unwrap().unwrap(), owner);
    let grant = h.grant_recovery(owner, key(1), limits(), 0).unwrap();
    drop(store);
    let bytes = fs::read(dir.path().join("store.sqlite3")).unwrap();
    assert!(matches!(Store::open(dir.path()), Err(Error::NewerSchema)));
    assert_eq!(fs::read(dir.path().join("store.sqlite3")).unwrap(), bytes);
    let store = Store::open_recovery(dir.path()).unwrap();
    assert_eq!(
        store.handle().check_recovery_grant(grant.clone()).unwrap(),
        grant
    );
}

#[test]
fn durable_grants_revoke_and_reapprove_by_exact_generation_without_normal_roles() {
    let (_dir, store, owner) = fixture();
    let h = store.handle();
    let g = h
        .grant_recovery(owner.clone(), key(1), limits(), 0)
        .unwrap();
    assert_eq!(g.generation, 1);
    assert!(g.active);
    assert!(matches!(
        h.grant_recovery(owner.clone(), key(1), limits(), 0),
        Err(Error::Conflict)
    ));
    assert!(matches!(
        h.revoke_recovery(owner.clone(), g.id.clone(), 2),
        Err(Error::Conflict)
    ));
    let revoked = h.revoke_recovery(owner.clone(), g.id.clone(), 1).unwrap();
    assert_eq!(revoked.generation, 2);
    assert!(!revoked.active);
    assert!(matches!(h.check_recovery_grant(g), Err(Error::Forbidden)));
    assert!(matches!(
        h.revoke_recovery(owner.clone(), revoked.id.clone(), 1),
        Err(Error::Conflict)
    ));
    assert_eq!(
        h.revoke_recovery(owner.clone(), revoked.id.clone(), 2)
            .unwrap(),
        revoked
    );
    let new = h
        .grant_recovery(owner.clone(), key(1), limits(), 2)
        .unwrap();
    assert_eq!(new.id, revoked.id);
    assert_eq!(new.generation, 3);
    assert_eq!(h.check_recovery_grant(new.clone()).unwrap(), new);
    assert_eq!(h.principals().unwrap(), vec![owner.clone()]);
    assert_eq!(
        h.recovery_audit(owner)
            .unwrap()
            .iter()
            .map(|a| a.kind.as_str())
            .collect::<Vec<_>>(),
        vec!["grant", "revoke", "reapprove"]
    );
}

#[test]
fn forged_snapshots_and_stale_owner_bindings_do_not_authorize() {
    let (_dir, store, owner) = fixture();
    let h = store.handle();
    let grant = h
        .grant_recovery(owner.clone(), key(1), limits(), 0)
        .unwrap();
    for field in 0..9 {
        let mut g = grant.clone();
        match field {
            0 => g.id = Uuid::new_v4().to_string(),
            1 => g.realm = Uuid::new_v4().to_string(),
            2 => g.generation += 1,
            3 => g.owner_generation += 1,
            4 => g.owner_key = [8; 32],
            5 => g.key.profile = "other-suite".into(),
            6 => g.key.public_key[0] ^= 1,
            7 => g.limits.request_budget -= 1,
            _ => g.active = false,
        }
        assert!(matches!(h.check_recovery_grant(g), Err(Error::Forbidden)));
    }
    h.call(|db| {
        db.connection.execute(
            "UPDATE principals SET active=0,generation=generation+1 WHERE kind='owner'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    assert!(matches!(
        h.check_recovery_grant(grant),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        h.grant_recovery(owner.clone(), key(2), limits(), 0),
        Err(Error::Forbidden)
    ));
    assert!(matches!(h.recovery_grants(owner), Err(Error::Forbidden)));
}

#[test]
fn ordinary_keys_clients_unknown_shapes_and_unbounded_limits_are_rejected() {
    let (_dir, store, owner) = fixture();
    let h = store.handle();
    h.bind_identity([7; 32]).unwrap();
    let client = h
        .approve_client(
            owner.clone(),
            [8; 32],
            Kind::Client,
            rustsimhub_policy::ALL_SCOPES,
        )
        .unwrap();
    for ordinary in [vec![9; 32], vec![8; 32], vec![7; 32]] {
        assert!(matches!(
            h.grant_recovery(
                owner.clone(),
                RecoveryKey {
                    profile: "test-key-v1".into(),
                    public_key: ordinary
                },
                limits(),
                0
            ),
            Err(Error::Forbidden)
        ));
    }
    assert!(matches!(
        h.grant_recovery(client, key(1), limits(), 0),
        Err(Error::Forbidden)
    ));
    for bad in [
        RecoveryKey {
            profile: "../key".into(),
            public_key: vec![1; 32],
        },
        RecoveryKey {
            profile: "x".repeat(65),
            public_key: vec![1; 32],
        },
        RecoveryKey {
            profile: "test-key-v1".into(),
            public_key: vec![],
        },
        RecoveryKey {
            profile: "test-key-v1".into(),
            public_key: vec![1; 1025],
        },
    ] {
        assert!(matches!(
            h.grant_recovery(owner.clone(), bad, limits(), 0),
            Err(Error::InvalidInput)
        ));
    }
    for bad in [
        RecoveryLimits {
            session_ms: 14_999,
            request_budget: 1,
        },
        RecoveryLimits {
            session_ms: 300_001,
            request_budget: 1,
        },
        RecoveryLimits {
            session_ms: 15_000,
            request_budget: 0,
        },
        RecoveryLimits {
            session_ms: 15_000,
            request_budget: 129,
        },
    ] {
        assert!(matches!(
            h.grant_recovery(owner.clone(), key(1), bad, 0),
            Err(Error::InvalidInput)
        ));
    }
    assert!(h.recovery_grants(owner).unwrap().is_empty());
}

#[test]
fn grant_revoke_and_reapproval_audit_failures_rollback_the_whole_writer_transaction() {
    let (_dir, store, owner) = fixture();
    let h = store.handle();
    let g = h
        .grant_recovery(owner.clone(), key(1), limits(), 0)
        .unwrap();
    let block = |db: &mut Database| {
        db.connection.execute_batch("CREATE TRIGGER refuse_audit BEFORE INSERT ON recovery_audit BEGIN SELECT RAISE(ABORT,'injected audit failure'); END;")?;
        Ok(())
    };
    h.call(block).unwrap();
    assert!(h
        .grant_recovery(owner.clone(), key(2), limits(), 0)
        .is_err());
    assert!(h.revoke_recovery(owner.clone(), g.id.clone(), 1).is_err());
    assert_eq!(h.recovery_grants(owner.clone()).unwrap(), vec![g.clone()]);
    assert_eq!(h.recovery_audit(owner.clone()).unwrap().len(), 1);
    h.call(|db| {
        db.connection.execute_batch("DROP TRIGGER refuse_audit")?;
        Ok(())
    })
    .unwrap();
    let g = h.revoke_recovery(owner.clone(), g.id, 1).unwrap();
    h.call(block).unwrap();
    assert!(h
        .grant_recovery(owner.clone(), key(1), limits(), 2)
        .is_err());
    assert_eq!(h.recovery_grants(owner.clone()).unwrap(), vec![g]);
    assert_eq!(h.recovery_audit(owner).unwrap().len(), 2);
}

#[test]
fn migration_failure_does_not_leave_partial_tables_or_change_schema_six() {
    let dir = directory();
    let store = Store::open(dir.path()).unwrap();
    let h = store.handle();
    let owner = h.bootstrap_owner([9; 32]).unwrap();
    h.call(|db| {
        db.connection
            .execute_batch("CREATE TABLE recovery_audit(collision TEXT)")?;
        Ok(())
    })
    .unwrap();
    drop(store);
    assert!(Store::open_recovery(dir.path()).is_err());
    let conn = Connection::open(dir.path().join("store.sqlite3")).unwrap();
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        6
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='recovery_grants'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    drop(conn);
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        store.handle().principal(owner.public_key).unwrap().unwrap(),
        owner
    );
}

#[test]
fn corrupted_and_future_recovery_records_fail_closed_on_reopen() {
    for sql in [
        "PRAGMA ignore_check_constraints=ON;UPDATE recovery_grants SET format=2",
        "UPDATE recovery_grants SET key_profile='../key'",
        "PRAGMA ignore_check_constraints=ON;UPDATE recovery_grants SET active=2",
        "DROP TABLE recovery_audit",
    ] {
        let (dir, store, owner) = fixture();
        let h = store.handle();
        h.grant_recovery(owner, key(1), limits(), 0).unwrap();
        let statement = sql.to_owned();
        h.call(move |db| {
            db.connection.execute_batch(&statement)?;
            Ok(())
        })
        .unwrap();
        drop(store);
        assert!(
            matches!(Store::open_recovery(dir.path()), Err(Error::Corrupt)),
            "{sql}"
        );
    }
    let (dir, store, _owner) = fixture();
    store
        .handle()
        .call(|db| {
            db.connection.execute_batch("PRAGMA user_version=8")?;
            Ok(())
        })
        .unwrap();
    drop(store);
    assert!(matches!(
        Store::open_recovery(dir.path()),
        Err(Error::NewerSchema)
    ));
}

#[test]
fn tombstones_are_not_evicted_at_capacity_and_only_audit_history_is_bounded() {
    let (_dir, store, owner) = fixture();
    let h = store.handle();
    let rows = (1..=32)
        .map(|n| {
            h.grant_recovery(owner.clone(), key(n), limits(), 0)
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        h.grant_recovery(owner.clone(), key(33), limits(), 0),
        Err(Error::Full)
    ));
    let mut g = h
        .revoke_recovery(owner.clone(), rows[0].id.clone(), 1)
        .unwrap();
    assert!(matches!(
        h.grant_recovery(owner.clone(), key(33), limits(), 0),
        Err(Error::Full)
    ));
    for _ in 0..140 {
        g = h
            .grant_recovery(owner.clone(), key(1), limits(), g.generation)
            .unwrap();
        g = h
            .revoke_recovery(owner.clone(), g.id, g.generation)
            .unwrap();
    }
    assert_eq!(h.recovery_grants(owner.clone()).unwrap().len(), 32);
    let audit = h.recovery_audit(owner).unwrap();
    assert_eq!(audit.len(), 256);
    assert!(audit[0].seq > 1);
}

#[test]
fn concurrent_compare_and_swap_cannot_revoke_or_reapprove_twice() {
    let (_dir, store, owner) = fixture();
    let h = store.handle();
    let g = h
        .grant_recovery(owner.clone(), key(1), limits(), 0)
        .unwrap();
    let a = h.clone();
    let b = h.clone();
    let (o1, o2) = (owner.clone(), owner.clone());
    let (id1, id2) = (g.id.clone(), g.id.clone());
    let one = thread::spawn(move || a.revoke_recovery(o1, id1, 1));
    let two = thread::spawn(move || b.revoke_recovery(o2, id2, 1));
    let results = [one.join().unwrap(), two.join().unwrap()];
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    let a = h.clone();
    let b = h.clone();
    let (o1, o2) = (owner.clone(), owner.clone());
    let one = thread::spawn(move || a.grant_recovery(o1, key(1), limits(), 2));
    let two = thread::spawn(move || b.grant_recovery(o2, key(1), limits(), 2));
    let results = [one.join().unwrap(), two.join().unwrap()];
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(h.recovery_audit(owner.clone()).unwrap().len(), 3);
    assert_eq!(h.recovery_grants(owner).unwrap()[0].generation, 3);
}

#[test]
fn old_backup_cannot_resurrect_recovery_keys_or_bootstrap_a_second_owner() {
    let (dir, store, owner) = fixture();
    let h = store.handle();
    let g = h
        .grant_recovery(owner.clone(), key(1), limits(), 0)
        .unwrap();
    let backup = directory();
    h.backup(backup.path()).unwrap();
    h.revoke_recovery(owner.clone(), g.id.clone(), 1).unwrap();
    drop(store);
    let destination = directory();
    let restored = Store::restore_recovery(backup.path(), destination.path()).unwrap();
    let r = restored.handle();
    assert_ne!(r.cursor().unwrap().db_epoch, g.realm);
    assert!(matches!(
        r.check_recovery_grant(g.clone()),
        Err(Error::Forbidden)
    ));
    assert!(matches!(r.bootstrap_owner([8; 32]), Err(Error::Forbidden)));
    let quarantined = r.principal(owner.public_key).unwrap().unwrap();
    assert!(!quarantined.active);
    let current = r.recover_owner(quarantined).unwrap();
    let prior = r.recovery_grants(current.clone()).unwrap().remove(0);
    assert!(!prior.active);
    assert_eq!(prior.generation, 2);
    assert!(matches!(
        r.grant_recovery(current.clone(), key(1), limits(), 0),
        Err(Error::Conflict)
    ));
    let new = r.grant_recovery(current, key(1), limits(), 2).unwrap();
    assert_eq!(new.id, g.id);
    assert_eq!(new.generation, 3);
    assert_ne!(new.realm, g.realm);
    assert!(r.check_recovery_grant(new.clone()).is_ok());
    drop(restored);
    let normal = Store::open_recovery(destination.path()).unwrap();
    assert_eq!(
        normal.handle().check_recovery_grant(new.clone()).unwrap(),
        new
    );
    let untouched = Store::open_recovery(dir.path()).unwrap();
    assert!(!untouched.handle().recovery_grants(owner).unwrap()[0].active);
}
