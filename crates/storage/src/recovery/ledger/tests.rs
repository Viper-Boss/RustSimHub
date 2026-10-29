// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use tempfile::TempDir;
fn dir() -> TempDir {
    let d = TempDir::new().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    }
    d
}
fn fixture() -> (TempDir, Store, Principal, RecoveryGrant) {
    let d = dir();
    let store = Store::open_recovery(d.path()).unwrap();
    let h = store.handle();
    let owner = h.bootstrap_owner([9; 32]).unwrap();
    let g = h
        .grant_recovery(
            owner.clone(),
            RecoveryKey {
                profile: "test-only".into(),
                public_key: vec![42; 33],
            },
            RecoveryLimits {
                session_ms: 120000,
                request_budget: 64,
            },
            0,
        )
        .unwrap();
    (d, store, owner, g)
}
fn id(n: u128) -> String {
    Uuid::from_u128(n).to_string()
}
fn reserve(
    h: &Handle,
    g: &RecoveryGrant,
    n: u128,
    a: RecoveryMutation,
    t: Option<String>,
) -> RecoveryReceipt {
    match h
        .reserve_recovery_request(g.clone(), id(n), a, [7; 32], t)
        .unwrap()
    {
        RecoveryReservation::New(r) => r,
        _ => panic!("expected new request"),
    }
}
#[test]
fn duplicate_requests_return_original_receipts_but_never_another_permit() {
    let (_d, store, _owner, g) = fixture();
    let h = store.handle();
    for (n, a, outcome) in [
        (1, RecoveryMutation::BeginWifi, RecoveryReceiptState::Ready),
        (
            2,
            RecoveryMutation::ConfirmWifi,
            RecoveryReceiptState::Saved,
        ),
        (
            3,
            RecoveryMutation::CancelWifi,
            RecoveryReceiptState::RolledBack,
        ),
    ] {
        let t = if a == RecoveryMutation::BeginWifi {
            None
        } else {
            Some("a".repeat(64))
        };
        let r = reserve(&h, &g, n, a, t.clone());
        let p = h.claim_recovery_request(g.clone(), r.clone()).unwrap();
        assert!(matches!(
            h.claim_recovery_request(g.clone(), r),
            Err(Error::Conflict)
        ));
        let finished = h
            .finish_recovery_request(p, outcome, Some("a".repeat(64)))
            .unwrap();
        match h
            .reserve_recovery_request(g.clone(), id(n), a, [7; 32], t)
            .unwrap()
        {
            RecoveryReservation::Existing(old) => assert_eq!(old, finished),
            _ => panic!("must not create a new request"),
        }
        assert_eq!(h.recovery_request(g.clone(), id(n)).unwrap(), finished);
    }
}
#[test]
fn duplicate_id_cannot_change_digest_action_or_original_trial() {
    let (_d, store, _owner, g) = fixture();
    let h = store.handle();
    reserve(
        &h,
        &g,
        1,
        RecoveryMutation::ConfirmWifi,
        Some("a".repeat(64)),
    );
    for (a, digest, t) in [
        (RecoveryMutation::ConfirmWifi, [8; 32], Some("a".repeat(64))),
        (RecoveryMutation::CancelWifi, [7; 32], Some("a".repeat(64))),
        (RecoveryMutation::ConfirmWifi, [7; 32], Some("b".repeat(64))),
    ] {
        assert!(matches!(
            h.reserve_recovery_request(g.clone(), id(1), a, digest, t),
            Err(Error::Conflict)
        ));
    }
    for (request, a, t) in [
        (id(0), RecoveryMutation::BeginWifi, None),
        ("not-uuid".into(), RecoveryMutation::BeginWifi, None),
        (id(2), RecoveryMutation::BeginWifi, Some("a".repeat(64))),
        (id(3), RecoveryMutation::ConfirmWifi, None),
        (id(4), RecoveryMutation::CancelWifi, Some("Z".repeat(64))),
    ] {
        assert!(matches!(
            h.reserve_recovery_request(g.clone(), request, a, [7; 32], t),
            Err(Error::InvalidInput)
        ));
    }
}
#[test]
fn concurrent_claims_on_the_real_writer_issue_only_one_permit() {
    let (_d, store, _owner, g) = fixture();
    let h = store.handle();
    let r = reserve(&h, &g, 1, RecoveryMutation::BeginWifi, None);
    let mut threads = Vec::new();
    for _ in 0..6 {
        let h = h.clone();
        let g = g.clone();
        let r = r.clone();
        threads.push(thread::spawn(move || h.claim_recovery_request(g, r)));
    }
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        h.recovery_request(g, id(1)).unwrap().state,
        RecoveryReceiptState::Accepted
    );
}
#[test]
fn restart_quarantines_prepared_and_accepted_preserving_unknown_and_terminal() {
    let (d, store, _owner, g) = fixture();
    let h = store.handle();
    reserve(&h, &g, 1, RecoveryMutation::BeginWifi, None);
    let r = reserve(&h, &g, 2, RecoveryMutation::BeginWifi, None);
    drop(h.claim_recovery_request(g.clone(), r).unwrap());
    let r = reserve(&h, &g, 3, RecoveryMutation::BeginWifi, None);
    let p = h.claim_recovery_request(g.clone(), r).unwrap();
    h.finish_recovery_request(p, RecoveryReceiptState::Unknown, None)
        .unwrap();
    let r = reserve(&h, &g, 4, RecoveryMutation::BeginWifi, None);
    let p = h.claim_recovery_request(g.clone(), r).unwrap();
    h.finish_recovery_request(p, RecoveryReceiptState::Ready, Some("a".repeat(64)))
        .unwrap();
    drop(store);
    let reopened = Store::open_recovery(d.path()).unwrap();
    let h = reopened.handle();
    for n in 1..=3 {
        let r = h.recovery_request(g.clone(), id(n)).unwrap();
        assert_eq!(r.state, RecoveryReceiptState::Unknown);
        assert!(matches!(
            h.claim_recovery_request(g.clone(), r),
            Err(Error::Conflict)
        ));
    }
    assert_eq!(
        h.recovery_request(g, id(4)).unwrap().state,
        RecoveryReceiptState::Ready
    );
}
#[test]
fn ledger_process_exit_child() {
    let Some(path) = std::env::var_os("RSH_LEDGER_EXIT_CHILD") else {
        return;
    };
    let store = Store::open_recovery(PathBuf::from(path)).unwrap();
    let h = store.handle();
    let owner = h.principals().unwrap().remove(0);
    let g = h.recovery_grants(owner).unwrap().remove(0);
    let r = reserve(&h, &g, 90, RecoveryMutation::BeginWifi, None);
    let _permit = h.claim_recovery_request(g, r).unwrap();
    std::process::exit(37);
}
#[test]
fn actual_process_exit_after_claim_never_reissues_the_request() {
    let (d, store, _owner, g) = fixture();
    drop(store);
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "recovery::ledger::tests::ledger_process_exit_child",
            "--nocapture",
        ])
        .env("RSH_LEDGER_EXIT_CHILD", d.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(37),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reopened = Store::open_recovery(d.path()).unwrap();
    let h = reopened.handle();
    let r = h.recovery_request(g.clone(), id(90)).unwrap();
    assert_eq!(r.state, RecoveryReceiptState::Unknown);
    assert!(matches!(
        h.claim_recovery_request(g.clone(), r.clone()),
        Err(Error::Conflict)
    ));
    assert!(matches!(
        h.reserve_recovery_request(g, id(90), RecoveryMutation::BeginWifi, [7; 32], None),
        Ok(RecoveryReservation::Existing(_))
    ));
}
#[test]
fn full_capacity_keeps_all_unknown_tombstones_and_allows_original_lookup() {
    let (d, store, _owner, g) = fixture();
    let h = store.handle();
    for n in 1..=MAX_RECEIPTS {
        reserve(&h, &g, n as u128, RecoveryMutation::BeginWifi, None);
    }
    assert!(matches!(
        h.reserve_recovery_request(
            g.clone(),
            id(257),
            RecoveryMutation::BeginWifi,
            [7; 32],
            None
        ),
        Err(Error::Full)
    ));
    drop(store);
    let reopened = Store::open_recovery(d.path()).unwrap();
    let h = reopened.handle();
    assert_eq!(
        h.recovery_request(g.clone(), id(1)).unwrap().state,
        RecoveryReceiptState::Unknown
    );
    assert!(matches!(
        h.reserve_recovery_request(g.clone(), id(1), RecoveryMutation::BeginWifi, [7; 32], None),
        Ok(RecoveryReservation::Existing(_))
    ));
    assert!(matches!(
        h.reserve_recovery_request(g, id(257), RecoveryMutation::BeginWifi, [7; 32], None),
        Err(Error::Full)
    ));
}
#[test]
fn schema_seven_upgrades_explicitly_and_failed_migration_remains_seven() {
    for broken in [false, true] {
        let (d, store, owner, g) = fixture();
        let h = store.handle();
        h.call(move |db| {
            db.connection
                .execute_batch("DROP TABLE recovery_receipts;PRAGMA user_version=7")?;
            if broken {
                db.connection
                    .execute_batch("UPDATE recovery_grants SET key_profile='../bad'")?;
            }
            Ok(())
        })
        .unwrap();
        drop(store);
        if broken {
            assert!(matches!(
                Store::open_recovery(d.path()),
                Err(Error::Corrupt)
            ));
            let c = Connection::open(d.path().join("store.sqlite3")).unwrap();
            assert_eq!(
                c.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                    .unwrap(),
                7
            );
            assert_eq!(
                c.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE name='recovery_receipts'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
        } else {
            let upgraded = Store::open_recovery(d.path()).unwrap();
            let h = upgraded.handle();
            assert_eq!(h.recovery_grants(owner).unwrap(), vec![g.clone()]);
            reserve(&h, &g, 1, RecoveryMutation::BeginWifi, None);
            assert_eq!(
                h.call(|db| Ok(db
                    .connection
                    .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?))
                    .unwrap(),
                8
            );
        }
    }
}
#[test]
fn receipt_corruption_and_missing_tables_fail_closed_on_reopen() {
    for sql in [
        "PRAGMA ignore_check_constraints=ON;UPDATE recovery_receipts SET format=2",
        "UPDATE recovery_receipts SET request_id='00000000-0000-0000-0000-000000000000'",
        "UPDATE recovery_receipts SET trial_id='z'||substr(hex(zeroblob(32)),2)",
        "UPDATE recovery_receipts SET state='saved'",
        "DROP TABLE recovery_receipts",
    ] {
        let (d, store, _owner, g) = fixture();
        let h = store.handle();
        reserve(&h, &g, 1, RecoveryMutation::BeginWifi, None);
        let statement = sql.to_owned();
        h.call(move |db| {
            db.connection.execute_batch(&statement)?;
            Ok(())
        })
        .unwrap();
        drop(store);
        assert!(
            matches!(Store::open_recovery(d.path()), Err(Error::Corrupt)),
            "{sql}"
        );
    }
}
#[test]
fn writer_failure_revocation_and_late_completion_preserve_truthful_receipts() {
    let (_d, store, owner, g) = fixture();
    let h = store.handle();
    h.call(|db|{db.connection.execute_batch("CREATE TEMP TRIGGER deny_receipt BEFORE INSERT ON recovery_receipts BEGIN SELECT RAISE(ABORT,'test');END")?;Ok(())}).unwrap();
    assert!(h
        .reserve_recovery_request(g.clone(), id(1), RecoveryMutation::BeginWifi, [7; 32], None)
        .is_err());
    assert!(matches!(
        h.recovery_request(g.clone(), id(1)),
        Err(Error::NotFound)
    ));
    h.call(|db| {
        db.connection.execute_batch("DROP TRIGGER deny_receipt")?;
        Ok(())
    })
    .unwrap();
    let r = reserve(&h, &g, 1, RecoveryMutation::BeginWifi, None);
    h.call(|db|{db.connection.execute_batch("CREATE TEMP TRIGGER deny_update BEFORE UPDATE ON recovery_receipts BEGIN SELECT RAISE(ABORT,'test');END")?;Ok(())}).unwrap();
    assert!(h.claim_recovery_request(g.clone(), r.clone()).is_err());
    assert_eq!(h.recovery_request(g.clone(), id(1)).unwrap(), r);
    h.call(|db| {
        db.connection.execute_batch("DROP TRIGGER deny_update")?;
        Ok(())
    })
    .unwrap();
    let permit = h.claim_recovery_request(g.clone(), r.clone()).unwrap();
    h.revoke_recovery(owner.clone(), g.id.clone(), 1).unwrap();
    assert!(h.recovery_request(g.clone(), id(1)).is_err());
    let saved = h
        .finish_recovery_request(permit, RecoveryReceiptState::Ready, Some("a".repeat(64)))
        .unwrap();
    assert_eq!(h.owner_recovery_request(owner, r).unwrap(), saved);
    assert!(h.claim_recovery_request(g, saved).is_err());
}
#[test]
fn failed_completion_cannot_make_an_accepted_request_retryable() {
    let (d, store, _owner, g) = fixture();
    let h = store.handle();
    let r = reserve(&h, &g, 1, RecoveryMutation::BeginWifi, None);
    let permit = h.claim_recovery_request(g.clone(), r).unwrap();
    h.call(|db|{db.connection.execute_batch("CREATE TEMP TRIGGER deny_finish BEFORE UPDATE ON recovery_receipts BEGIN SELECT RAISE(ABORT,'test');END")?;Ok(())}).unwrap();
    assert!(h
        .finish_recovery_request(permit, RecoveryReceiptState::Ready, Some("a".repeat(64)))
        .is_err());
    let accepted = h.recovery_request(g.clone(), id(1)).unwrap();
    assert_eq!(accepted.state, RecoveryReceiptState::Accepted);
    assert!(matches!(
        h.claim_recovery_request(g.clone(), accepted),
        Err(Error::Conflict)
    ));
    drop(store);
    let reopened = Store::open_recovery(d.path()).unwrap();
    assert_eq!(
        reopened.handle().recovery_request(g, id(1)).unwrap().state,
        RecoveryReceiptState::Unknown
    );
}
#[test]
fn restored_realm_and_reapproved_generation_cannot_claim_old_receipts() {
    let (d, store, owner, g) = fixture();
    let h = store.handle();
    let r = reserve(&h, &g, 1, RecoveryMutation::BeginWifi, None);
    let backup = dir();
    h.backup(backup.path()).unwrap();
    drop(store);
    let destination = dir();
    let restored = Store::restore_recovery(backup.path(), destination.path()).unwrap();
    let h = restored.handle();
    let quarantined = h.principal(owner.public_key).unwrap().unwrap();
    let owner = h.recover_owner(quarantined).unwrap();
    let old = h.recovery_grants(owner.clone()).unwrap().remove(0);
    let new = h
        .grant_recovery(owner.clone(), g.key.clone(), g.limits, old.generation)
        .unwrap();
    assert!(matches!(
        h.owner_recovery_request(owner, r.clone()),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        h.claim_recovery_request(new.clone(), r),
        Err(Error::Forbidden)
    ));
    reserve(&h, &new, 1, RecoveryMutation::BeginWifi, None);
    assert!(Store::open(d.path()).is_err());
}
