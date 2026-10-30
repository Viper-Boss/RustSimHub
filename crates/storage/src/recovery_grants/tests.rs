// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use rustsimhub_policy::{Kind, ALL_SCOPES};
use tempfile::TempDir;

fn directory() -> TempDir {
    crate::tests::directory()
}
fn request(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn fixture() -> (TempDir, TempDir, Store, Principal) {
    let d = directory();
    let backup = directory();
    let s = Store::open(d.path()).unwrap();
    s.handle().bind_identity([9; 32]).unwrap();
    let owner = s.handle().bootstrap_owner([1; 32]).unwrap();
    drop(s);
    let s = Store::migrate_recovery_grants(d.path(), backup.path()).unwrap();
    (d, backup, s, owner)
}
fn grant(
    h: &Handle,
    owner: &Principal,
    key: u8,
    generation: u64,
    id: u32,
) -> Result<RecoveryGrant> {
    h.change_recovery_grant(
        owner.clone(),
        [key; 32],
        generation,
        request(id),
        RecoveryChange::Grant {
            max_trial_ms: 40000,
        },
    )
}
fn version(path: &Path) -> i64 {
    Connection::open(path.join("store.sqlite3"))
        .unwrap()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap()
}

#[test]
fn authorization_gate_checks_every_snapshot_field_without_writing_or_trusting_receipts() {
    let (_d, _b, s, owner) = fixture();
    let h = s.handle();
    let g = grant(&h, &owner, 2, 0, 1).unwrap();
    let before = h.call(|db| Ok(db.connection.total_changes())).unwrap();
    assert_eq!(h.check_recovery_grant(g.clone()).unwrap(), g);
    let mut changed = Vec::new();
    let mut x = g.clone();
    x.id = request(99);
    changed.push(x);
    let mut x = g.clone();
    x.realm = request(99);
    changed.push(x);
    let mut x = g.clone();
    x.owner_id = request(99);
    changed.push(x);
    let mut x = g.clone();
    x.owner_generation += 1;
    changed.push(x);
    let mut x = g.clone();
    x.generation += 1;
    changed.push(x);
    let mut x = g.clone();
    x.max_trial_ms -= 1;
    changed.push(x);
    let mut x = g.clone();
    x.independent_public_key = [3; 32];
    changed.push(x);
    let mut x = g.clone();
    x.device_public_key = [3; 32];
    changed.push(x);
    let mut x = g.clone();
    x.active = false;
    changed.push(x);
    for x in changed {
        assert!(matches!(h.check_recovery_grant(x), Err(Error::Forbidden)));
    }
    assert_eq!(
        h.call(|db| Ok(db.connection.total_changes())).unwrap(),
        before
    );
    h.change_recovery_grant(
        owner.clone(),
        [2; 32],
        1,
        request(2),
        RecoveryChange::Revoke,
    )
    .unwrap();
    let historical = h
        .recovery_grant_request(owner, request(1))
        .unwrap()
        .unwrap();
    assert!(historical.active);
    assert!(matches!(
        h.check_recovery_grant(historical),
        Err(Error::Forbidden)
    ));
}

#[test]
fn authorization_gate_rechecks_real_owner_generation_active_state_realm_and_device() {
    // Each case mutates only synthetic authority metadata on the actual writer.
    // No fake Principal or alternate authorization implementation is used.
    for sql in [
        "UPDATE principals SET generation=generation+1 WHERE kind='owner'",
        "UPDATE principals SET active=0 WHERE kind='owner'",
        "UPDATE meta SET db_epoch='00000000-0000-4000-8000-000000000099'",
        "UPDATE device_identity SET public_key=zeroblob(32)",
    ] {
        let (_d, _b, s, owner) = fixture();
        let h = s.handle();
        let g = grant(&h, &owner, 2, 0, 1).unwrap();
        h.call(move |db| {
            db.connection.execute(sql, [])?;
            Ok(())
        })
        .unwrap();
        assert!(
            matches!(h.check_recovery_grant(g), Err(Error::Forbidden)),
            "{sql}"
        );
    }
}

#[test]
fn authorization_gate_rejects_schema6_and_real_restore_without_rewriting_grants() {
    let (_d, _b, s, owner) = fixture();
    let h = s.handle();
    let g = grant(&h, &owner, 2, 0, 1).unwrap();
    let six = directory();
    let six = Store::open(six.path()).unwrap();
    assert!(matches!(
        six.handle().check_recovery_grant(g.clone()),
        Err(Error::InvalidTransition)
    ));
    let backup = directory();
    let target = directory();
    h.backup(backup.path()).unwrap();
    let restored = Store::restore_recovery_grants(backup.path(), target.path()).unwrap();
    let rh = restored.handle();
    assert!(matches!(
        rh.check_recovery_grant(g.clone()),
        Err(Error::Forbidden)
    ));
    let prior = rh.principal(owner.public_key).unwrap().unwrap();
    assert!(!prior.active);
    let renewed = rh.recover_owner(prior).unwrap();
    assert!(renewed.generation > owner.generation);
    let stored = rh
        .recovery_grant(renewed, g.independent_public_key)
        .unwrap()
        .unwrap();
    assert_eq!(stored, g); // active historical row survived, and is still not authority.
    assert!(matches!(
        rh.check_recovery_grant(stored),
        Err(Error::Forbidden)
    ));
}

#[test]
fn explicit_migration_has_a_schema6_backup_and_default_open_never_adopts_schema7() {
    let (d, backup, s, owner) = fixture();
    let h = s.handle();
    assert_eq!(h.principals().unwrap(), vec![owner]);
    assert_eq!(h.cursor().unwrap().db_epoch, {
        let c = Connection::open(backup.path().join("backup.sqlite3")).unwrap();
        assert_eq!(
            c.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            6
        );
        c.query_row("SELECT db_epoch FROM meta", [], |r| r.get::<_, String>(0))
            .unwrap()
    });
    drop(h);
    drop(s);
    assert_eq!(version(d.path()), 7);
    assert!(matches!(Store::open(d.path()), Err(Error::NewerSchema)));
    Store::open_recovery_grants(d.path()).unwrap();
    let target = directory();
    assert!(Store::restore(backup.path(), target.path()).is_ok());
    let six = directory();
    let s = Store::open(six.path()).unwrap();
    drop(s);
    assert!(matches!(
        Store::open_recovery_grants(six.path()),
        Err(Error::InvalidTransition)
    ));
    assert_eq!(version(six.path()), 6);
}

#[test]
fn lost_reply_replays_exact_snapshot_and_revoke_regrant_never_reuses_a_generation() {
    let (d, _b, s, owner) = fixture();
    let h = s.handle();
    let g = grant(&h, &owner, 2, 0, 1).unwrap();
    assert_eq!(grant(&h, &owner, 2, 0, 1).unwrap(), g);
    assert_eq!(
        h.recovery_grant_request(owner.clone(), request(1)).unwrap(),
        Some(g.clone())
    );
    assert!(matches!(grant(&h, &owner, 3, 0, 1), Err(Error::Conflict)));
    assert!(matches!(grant(&h, &owner, 2, 0, 2), Err(Error::Conflict)));
    let off = h
        .change_recovery_grant(
            owner.clone(),
            [2; 32],
            g.generation,
            request(2),
            RecoveryChange::Revoke,
        )
        .unwrap();
    assert!(!off.active);
    assert_eq!(off.generation, g.generation + 1);
    assert_eq!(grant(&h, &owner, 2, 0, 1).unwrap(), g); // A historical receipt is not a live capability.
    let next = grant(&h, &owner, 2, off.generation, 3).unwrap();
    assert_eq!(next.id, g.id);
    assert_eq!(next.generation, off.generation + 1);
    drop(h);
    drop(s);
    let s = Store::open_recovery_grants(d.path()).unwrap();
    assert_eq!(
        s.handle().recovery_grant(owner, [2; 32]).unwrap(),
        Some(next)
    );
}

#[test]
fn transaction_failure_writes_neither_grant_nor_success_receipt() {
    let (_d, _b, s, owner) = fixture();
    let h = s.handle();
    h.call(|db|{db.connection.execute_batch("CREATE TRIGGER fail_recovery_receipt BEFORE INSERT ON recovery_grant_receipts BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")?;Ok(())}).unwrap();
    assert!(grant(&h, &owner, 2, 0, 1).is_err());
    assert!(h.recovery_grant(owner.clone(), [2; 32]).unwrap().is_none());
    assert!(h
        .recovery_grant_request(owner.clone(), request(1))
        .unwrap()
        .is_none());
    h.call(|db| {
        db.connection
            .execute_batch("DROP TRIGGER fail_recovery_receipt")?;
        Ok(())
    })
    .unwrap();
    grant(&h, &owner, 2, 0, 1).unwrap();
}

#[test]
fn bounded_receipts_reserve_revocation_capacity_and_key_tombstones_are_not_evicted() {
    let (_d, _b, s, owner) = fixture();
    let h = s.handle();
    let mut generation = 0;
    // 127 full cycles use 254 receipts; the last grant reserves receipt 256 for revoke.
    for i in 0..127 {
        let g = grant(&h, &owner, 2, generation, 2 * i + 1).unwrap();
        generation = h
            .change_recovery_grant(
                owner.clone(),
                [2; 32],
                g.generation,
                request(2 * i + 2),
                RecoveryChange::Revoke,
            )
            .unwrap()
            .generation;
    }
    let last = grant(&h, &owner, 2, generation, 255).unwrap();
    assert!(matches!(grant(&h, &owner, 3, 0, 300), Err(Error::Full)));
    let off = h
        .change_recovery_grant(
            owner.clone(),
            [2; 32],
            last.generation,
            request(256),
            RecoveryChange::Revoke,
        )
        .unwrap();
    assert!(!off.active);
    assert!(matches!(
        grant(&h, &owner, 2, off.generation, 257),
        Err(Error::Full)
    ));
    assert_eq!(h.recovery_grant(owner, [2; 32]).unwrap(), Some(off));
    h.call(|db| {
        validate(&db.connection)?;
        Ok(())
    })
    .unwrap();
    let (_d, _b, s, owner) = fixture();
    let h = s.handle();
    for key in 20..36 {
        grant(&h, &owner, key, 0, u32::from(key)).unwrap();
    }
    assert!(matches!(grant(&h, &owner, 36, 0, 36), Err(Error::Full)));
    assert_eq!(h.principals().unwrap().len(), 1);
}

#[test]
fn generation_exhaustion_still_allows_final_revocation_and_forbids_regrant() {
    let (_d, _b, s, owner) = fixture();
    let h = s.handle();
    grant(&h, &owner, 2, 0, 1).unwrap();
    h.call(|db| {
        db.connection.execute(
            "UPDATE recovery_grants SET generation=?",
            [MAX_GENERATION as i64 - 1],
        )?;
        Ok(())
    })
    .unwrap();
    let off = h
        .change_recovery_grant(
            owner.clone(),
            [2; 32],
            MAX_GENERATION - 1,
            request(2),
            RecoveryChange::Revoke,
        )
        .unwrap();
    assert_eq!(off.generation, MAX_GENERATION);
    assert!(!off.active);
    assert!(matches!(
        grant(&h, &owner, 2, off.generation, 3),
        Err(Error::Full)
    ));
}

#[test]
fn failed_backup_and_each_ddl_failure_preserve_schema6_and_original_tables() {
    for collision in ["recovery_grants", "recovery_grant_receipts"] {
        let d = directory();
        let b = directory();
        let s = Store::open(d.path()).unwrap();
        let name = collision.to_owned();
        s.handle()
            .call(move |db| {
                db.connection.execute_batch(&format!(
                    "CREATE TABLE {name}(evidence TEXT); INSERT INTO {name} VALUES('preserve');"
                ))?;
                Ok(())
            })
            .unwrap();
        drop(s);
        assert!(Store::migrate_recovery_grants(d.path(), b.path()).is_err());
        assert_eq!(version(d.path()), 6);
        let c = Connection::open(d.path().join("store.sqlite3")).unwrap();
        assert_eq!(
            c.query_row(&format!("SELECT evidence FROM {collision}"), [], |r| r
                .get::<_, String>(
                0
            ))
            .unwrap(),
            "preserve"
        );
        if collision == "recovery_grant_receipts" {
            assert_eq!(
                c.query_row(
                    "SELECT COUNT(*) FROM sqlite_schema WHERE name='recovery_grants'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
        }
    }
    let d = directory();
    let b = directory();
    fs::write(b.path().join("backup.sqlite3"), b"keep").unwrap();
    assert!(Store::migrate_recovery_grants(d.path(), b.path()).is_err());
    assert_eq!(version(d.path()), 6);
    assert_eq!(fs::read(b.path().join("backup.sqlite3")).unwrap(), b"keep");
}

#[test]
fn missing_relaxed_corrupt_or_future_schema_is_rejected_without_repair() {
    for sql in [
        "DROP TABLE recovery_grant_receipts",
        "ALTER TABLE recovery_grants RENAME TO evidence",
        "PRAGMA user_version=8",
        "PRAGMA ignore_check_constraints=ON; UPDATE recovery_grants SET format=2",
        "UPDATE recovery_grant_receipts SET result='{}'",
    ] {
        let (d, _b, s, owner) = fixture();
        grant(&s.handle(), &owner, 2, 0, 1).unwrap();
        drop(s);
        let c = Connection::open(d.path().join("store.sqlite3")).unwrap();
        c.execute_batch(sql).unwrap();
        drop(c);
        assert!(
            Store::open_recovery_grants(d.path()).is_err(),
            "accepted {sql}"
        );
    }
}

#[test]
fn restore_rotates_realm_preserves_evidence_and_requires_new_owner_approval() {
    let (_d, _b, s, owner) = fixture();
    let h = s.handle();
    let g = grant(&h, &owner, 2, 0, 1).unwrap();
    let operation = h
        .prepare_sms(SmsRequest {
            client_id: owner.id.clone(),
            idempotency_key: "synthetic-restored-prepare".into(),
            line: LineEpoch(1),
            recipient: "+12025550123".into(),
            text: "synthetic offline restore".into(),
        })
        .unwrap();
    let b = directory();
    h.backup(b.path()).unwrap();
    let before = fs::read(b.path().join("backup.sqlite3")).unwrap();
    assert!(matches!(
        Store::restore(b.path(), directory().path()),
        Err(Error::NewerSchema)
    ));
    assert_eq!(fs::read(b.path().join("backup.sqlite3")).unwrap(), before);
    let target = directory();
    let restored = Store::restore_recovery_grants(b.path(), target.path()).unwrap();
    let rh = restored.handle();
    let op_id = operation.id.clone();
    rh.call(move |db| {
        assert_eq!(
            db.connection.query_row(
                "SELECT state FROM sms_operations WHERE id=?",
                [op_id],
                |r| r.get::<_, String>(0)
            )?,
            "unknown"
        );
        Ok(())
    })
    .unwrap();
    assert_ne!(rh.cursor().unwrap().db_epoch, g.realm);
    assert!(rh.recovery_grant(owner.clone(), [2; 32]).is_err());
    let inactive = rh.principal(owner.public_key).unwrap().unwrap();
    let recovered = rh.recover_owner(inactive).unwrap();
    assert_eq!(
        rh.recovery_grant(recovered.clone(), [2; 32]).unwrap(),
        Some(g.clone())
    );
    assert!(rh
        .recovery_grant_request(recovered.clone(), request(1))
        .unwrap()
        .is_none());
    let new = grant(&rh, &recovered, 2, g.generation, 2).unwrap();
    assert_eq!(new.id, g.id);
    assert_ne!(new.realm, g.realm);
    assert!(new.generation > g.generation);
}

#[test]
fn independent_keys_cannot_become_clients_and_similar_payloads_cannot_cross_cas() {
    let (_d, _b, s, owner) = fixture();
    let h = s.handle();
    let g = grant(&h, &owner, 2, 0, 1).unwrap();
    assert!(h
        .approve_client(owner.clone(), [2; 32], Kind::Client, ALL_SCOPES)
        .is_err());
    h.approve_client(owner.clone(), [3; 32], Kind::Client, 1)
        .unwrap();
    assert!(matches!(grant(&h, &owner, 3, 0, 2), Err(Error::Forbidden)));
    assert!(matches!(grant(&h, &owner, 9, 0, 3), Err(Error::Forbidden)));
    let h1 = h.clone();
    let h2 = h.clone();
    let o1 = owner.clone();
    let o2 = owner;
    let a = thread::spawn(move || {
        h1.change_recovery_grant(
            o1,
            [2; 32],
            g.generation,
            request(10),
            RecoveryChange::Revoke,
        )
    });
    let b = thread::spawn(move || {
        h2.change_recovery_grant(
            o2,
            [2; 32],
            g.generation,
            request(11),
            RecoveryChange::Revoke,
        )
    });
    let outcomes = [a.join().unwrap(), b.join().unwrap()];
    assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|r| matches!(r, Err(Error::Conflict)))
            .count(),
        1
    );
}

#[test]
fn single_writer_rechecks_owner_generation_after_queueing() {
    use std::sync::{Arc, Barrier};
    let (_d, _b, s, owner) = fixture();
    let h = s.handle();
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let e = entered.clone();
    let r = release.clone();
    let blocker = h.clone();
    let job = thread::spawn(move || {
        blocker.call(move |db| {
            e.wait();
            r.wait();
            db.connection
                .execute("UPDATE principals SET generation=generation+1", [])?;
            Ok(())
        })
    });
    entered.wait();
    let writer = h.clone();
    let old = owner.clone();
    let mutation = thread::spawn(move || grant(&writer, &old, 2, 0, 1));
    release.wait();
    job.join().unwrap().unwrap();
    assert!(matches!(mutation.join().unwrap(), Err(Error::Forbidden)));
    let fresh = h.principal(owner.public_key).unwrap().unwrap();
    assert!(h
        .recovery_grant_request(fresh, request(1))
        .unwrap()
        .is_none());
}

#[test]
fn versioned_digest_vector_binds_every_cas_and_owner_context_field() {
    let owner = Principal {
        id: "00000000-0000-4000-8000-000000000001".into(),
        public_key: [1; 32],
        kind: Kind::Owner,
        scopes: ALL_SCOPES,
        generation: 1,
        active: true,
    };
    let realm = "00000000-0000-4000-8000-000000000002";
    let d = digest(
        &owner,
        realm,
        &[9; 32],
        &[2; 32],
        0,
        RecoveryChange::Grant {
            max_trial_ms: 40000,
        },
    );
    let hex = d.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        hex,
        "924e5da71e62469afc316bb45d0466ea6f0173c029830b39f6f32d0a0e045c72"
    );
    assert_ne!(
        d,
        digest(
            &owner,
            realm,
            &[9; 32],
            &[2; 32],
            1,
            RecoveryChange::Grant {
                max_trial_ms: 40000
            }
        )
    );
    assert_ne!(
        d,
        digest(&owner, realm, &[9; 32], &[2; 32], 0, RecoveryChange::Revoke)
    );
}

#[test]
fn all_legacy_schema_versions_upgrade_through_existing_migrations_without_losing_history() {
    let migrations = [
        include_str!("../schema.sql"),
        include_str!("../auth.sql"),
        include_str!("../inbox.sql"),
        include_str!("../outgoing.sql"),
        include_str!("../calls.sql"),
        include_str!("../inbox-management.sql"),
    ];
    for version in 1..=6 {
        let d = directory();
        let b = directory();
        let path = d.path().join("store.sqlite3");
        let c = Connection::open(&path).unwrap();
        for sql in migrations.iter().take(version) {
            c.execute_batch(sql).unwrap();
        }
        c.execute(
            "INSERT INTO meta VALUES(1,'00000000-0000-4000-8000-000000000001',1)",
            [],
        )
        .unwrap();
        c.execute("INSERT INTO sms_operations VALUES('synthetic-op','synthetic-client','synthetic-request',1,zeroblob(32),'+12025550123','synthetic history','prepared')",[]).unwrap();
        drop(c);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let s = Store::migrate_recovery_grants(d.path(), b.path()).unwrap();
        s.handle()
            .call(|db| {
                assert_eq!(
                    db.connection
                        .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))?,
                    7
                );
                let value: (String, String) = db.connection.query_row(
                    "SELECT body,state FROM sms_operations WHERE id='synthetic-op'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                assert_eq!(value, ("synthetic history".into(), "prepared".into()));
                Ok(())
            })
            .unwrap();
        let backup = Connection::open(b.path().join("backup.sqlite3")).unwrap();
        assert_eq!(
            backup
                .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            6
        );
    }
}
