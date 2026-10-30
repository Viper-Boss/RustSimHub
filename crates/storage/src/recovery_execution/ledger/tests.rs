// SPDX-License-Identifier: AGPL-3.0-only
//! Real writer/SQLite. Test-only duplicated tickets challenge the atomic DB
//! guard; production tickets have no constructor/Clone and cannot be copied.
use super::super::tests::{directory, nine, request, rows, seed};
use super::*;
use std::sync::{mpsc, Barrier};
fn fence() -> RecoveryExecutionFence {
    RecoveryExecutionFence::from_trusted_auth(
        Arc::new(AtomicBool::new(true)),
        Instant::now() + Duration::from_secs(60),
    )
}
fn input(
    g: &RecoveryGrant,
    n: u32,
    a: RecoveryExecutionAction,
    fp: u8,
    t: Option<String>,
) -> TrustedRecoveryExecution {
    TrustedRecoveryExecution::from_trusted_auth(g.clone(), request(n), a, [fp; 32], t, fence())
        .unwrap()
}
fn reserve(h: &Handle, i: TrustedRecoveryExecution) -> StorageRecoveryExecutionReservation {
    match h.reserve_recovery_execution_from_trusted_auth(i).unwrap() {
        RecoveryExecutionReservation::Reserved(r) => r,
        _ => panic!("expected newly inserted reservation"),
    }
}
fn duplicate(r: &StorageRecoveryExecutionReservation) -> StorageRecoveryExecutionReservation {
    let i = &r.input;
    StorageRecoveryExecutionReservation {
        input: Box::new(TrustedRecoveryExecution {
            grant: i.grant.clone(),
            request: i.request.clone(),
            action: i.action,
            fingerprint: i.fingerprint,
            trial: i.trial.clone(),
            fence: RecoveryExecutionFence::from_trusted_auth(
                i.fence.live.clone(),
                i.fence.deadline,
            ),
        }),
        receipt: r.receipt.clone(),
    }
}
fn start(h: &Handle, g: &RecoveryGrant, n: u32, t: &str) -> StorageRecoveryExecutionClaim {
    let r = reserve(h, input(g, n, RecoveryExecutionAction::BeginWifi, 7, None));
    h.claim_recovery_execution_from_trusted_auth(r, Some(t.into()))
        .unwrap()
}
fn lookup(h: &Handle, g: &RecoveryGrant, n: u32) -> Option<RecoveryExecutionReceipt> {
    h.lookup_recovery_execution_from_trusted_auth(g.clone(), request(n), fence())
        .unwrap()
}
#[test]
fn reserve_is_idempotent_conflicts_are_closed_and_claim_is_never_requeued() {
    let (_d, s, _o, g) = nine();
    let h = s.handle();
    let r = reserve(
        &h,
        input(&g, 2, RecoveryExecutionAction::BeginWifi, 7, None),
    );
    assert!(matches!(
        h.reserve_recovery_execution_from_trusted_auth(input(
            &g,
            2,
            RecoveryExecutionAction::BeginWifi,
            7,
            None
        ))
        .unwrap(),
        RecoveryExecutionReservation::Existing(_)
    ));
    for i in [
        input(&g, 2, RecoveryExecutionAction::BeginWifi, 8, None),
        input(&g, 3, RecoveryExecutionAction::BeginWifi, 7, None),
        input(
            &g,
            2,
            RecoveryExecutionAction::CancelWifi,
            7,
            Some("a".repeat(64)),
        ),
    ] {
        assert!(matches!(
            h.reserve_recovery_execution_from_trusted_auth(i),
            Err(Error::Conflict)
        ));
    }
    let duplicate = duplicate(&r);
    let c = h
        .claim_recovery_execution_from_trusted_auth(r, Some("a".repeat(64)))
        .unwrap();
    h.check_recovery_execution_claim_from_trusted_auth(
        input(&g, 2, RecoveryExecutionAction::BeginWifi, 7, None),
        &c,
    )
    .unwrap();
    drop(c);
    assert!(matches!(
        h.claim_recovery_execution_from_trusted_auth(duplicate, Some("b".repeat(64))),
        Err(Error::Conflict)
    ));
    assert_eq!(
        h.reserve_recovery_execution_from_trusted_auth(input(
            &g,
            2,
            RecoveryExecutionAction::BeginWifi,
            7,
            None
        ))
        .unwrap()
        .receipt()
        .state(),
        RecoveryExecutionState::Claimed
    );
    assert_eq!(rows(&h).len(), 1);
}
#[test]
fn competing_handle_claims_commit_exactly_one_trial() {
    let (_d, s, _o, g) = nine();
    let h = s.handle();
    let r = reserve(
        &h,
        input(&g, 2, RecoveryExecutionAction::BeginWifi, 7, None),
    );
    let copy = duplicate(&r);
    let barrier = Arc::new(Barrier::new(2));
    let results = std::thread::scope(|scope| {
        let jobs: Vec<_> = [(r, 'a'), (copy, 'b')]
            .into_iter()
            .map(|(r, letter)| {
                let h = h.clone();
                let b = barrier.clone();
                scope.spawn(move || {
                    b.wait();
                    h.claim_recovery_execution_from_trusted_auth(
                        r,
                        Some(letter.to_string().repeat(64)),
                    )
                })
            })
            .collect();
        jobs.into_iter()
            .map(|j| j.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(Error::Conflict)))
            .count(),
        1
    );
    assert_eq!(rows(&h).len(), 1);
}
#[test]
fn reopen_retains_unknown_and_blocks_new_writes_but_queries_remain_readonly() {
    let (d, s, _o, g) = nine();
    let h = s.handle();
    let r = reserve(
        &h,
        input(&g, 2, RecoveryExecutionAction::BeginWifi, 7, None),
    );
    drop(s);
    let s = Store::open_recovery_execution(d.path()).unwrap();
    let h = s.handle();
    assert_eq!(
        lookup(&h, &g, 2).unwrap().state(),
        RecoveryExecutionState::Unknown
    );
    assert!(lookup(&h, &g, 99).is_none());
    assert!(matches!(
        h.reserve_recovery_execution_from_trusted_auth(input(
            &g,
            2,
            RecoveryExecutionAction::BeginWifi,
            7,
            None
        ))
        .unwrap(),
        RecoveryExecutionReservation::Existing(_)
    ));
    assert!(matches!(
        h.claim_recovery_execution_from_trusted_auth(r, Some("a".repeat(64))),
        Err(Error::Conflict)
    ));
    for (n, a, t) in [
        (3, RecoveryExecutionAction::BeginWifi, None),
        (4, RecoveryExecutionAction::CancelWifi, Some("a".repeat(64))),
    ] {
        assert!(matches!(
            h.reserve_recovery_execution_from_trusted_auth(input(&g, n, a, 7, t)),
            Err(Error::Conflict)
        ));
    }
    assert_eq!(rows(&h).len(), 1);
}
#[test]
fn revoked_or_changed_database_context_cannot_claim_or_read_existing_receipt() {
    for mutation in [
        "UPDATE recovery_grants SET active=0,generation=generation+1",
        "UPDATE principals SET active=0 WHERE kind='owner'",
        "UPDATE meta SET db_epoch='00000000-0000-4000-8000-000000000099'",
    ] {
        let (_d, s, _o, g) = nine();
        let h = s.handle();
        let r = reserve(
            &h,
            input(&g, 2, RecoveryExecutionAction::BeginWifi, 7, None),
        );
        h.call(move |db| {
            db.connection.execute(mutation, [])?;
            Ok(())
        })
        .unwrap();
        assert!(matches!(
            h.claim_recovery_execution_from_trusted_auth(r, Some("a".repeat(64))),
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            h.lookup_recovery_execution_from_trusted_auth(g.clone(), request(2), fence()),
            Err(Error::Forbidden)
        ));
        assert_eq!(rows(&h)[0].1, "reserved");
    }
}
#[test]
fn dead_and_expired_fences_never_insert_or_claim() {
    let (_d, s, _o, g) = nine();
    let h = s.handle();
    for (live, deadline) in [
        (false, Instant::now() + Duration::from_secs(60)),
        (true, Instant::now()),
    ] {
        let mut i = input(&g, 2, RecoveryExecutionAction::BeginWifi, 7, None);
        i.fence =
            RecoveryExecutionFence::from_trusted_auth(Arc::new(AtomicBool::new(live)), deadline);
        assert!(matches!(
            h.reserve_recovery_execution_from_trusted_auth(i),
            Err(Error::Forbidden)
        ));
    }
    assert!(rows(&h).is_empty());
    let r = reserve(
        &h,
        input(&g, 2, RecoveryExecutionAction::BeginWifi, 7, None),
    );
    r.input.fence.live.store(false, Ordering::Release);
    assert!(matches!(
        h.claim_recovery_execution_from_trusted_auth(r, Some("a".repeat(64))),
        Err(Error::Forbidden)
    ));
    assert_eq!(rows(&h)[0].1, "reserved");
}
#[test]
fn queued_claim_checks_actual_cancellation_flag_when_writer_runs() {
    let (_d, s, _o, g) = nine();
    let h = s.handle();
    let r = reserve(
        &h,
        input(&g, 2, RecoveryExecutionAction::BeginWifi, 7, None),
    );
    let live = r.input.fence.live.clone();
    let (et, er) = mpsc::sync_channel(1);
    let (rt, rr) = mpsc::sync_channel(1);
    h.sender
        .try_send(crate::Message::Run(Box::new(move |_| {
            et.send(()).unwrap();
            rr.recv().unwrap();
        })))
        .unwrap();
    er.recv().unwrap();
    let (tx, rx) = mpsc::sync_channel(1);
    h.sender
        .try_send(crate::Message::Run(Box::new(move |db| {
            tx.send(claim_locked(db, r, Some("a".repeat(64))).map(|_| ()))
                .unwrap();
        })))
        .unwrap();
    live.store(false, Ordering::Release);
    rt.send(()).unwrap();
    assert_eq!(rx.recv().unwrap(), Err(Error::Forbidden));
    assert_eq!(rows(&h)[0].1, "reserved");
}
#[test]
fn queued_claim_reads_revocation_committed_ahead_of_it() {
    let (_d, s, _o, g) = nine();
    let h = s.handle();
    let r = reserve(
        &h,
        input(&g, 2, RecoveryExecutionAction::BeginWifi, 7, None),
    );
    let (et, er) = mpsc::sync_channel(1);
    let (rt, rr) = mpsc::sync_channel(1);
    h.sender
        .try_send(crate::Message::Run(Box::new(move |db| {
            et.send(()).unwrap();
            rr.recv().unwrap();
            db.connection
                .execute(
                    "UPDATE recovery_grants SET active=0,generation=generation+1",
                    [],
                )
                .unwrap();
        })))
        .unwrap();
    er.recv().unwrap();
    let (tx, rx) = mpsc::sync_channel(1);
    h.sender
        .try_send(crate::Message::Run(Box::new(move |db| {
            tx.send(claim_locked(db, r, Some("a".repeat(64))).map(|_| ()))
                .unwrap();
        })))
        .unwrap();
    rt.send(()).unwrap();
    assert_eq!(rx.recv().unwrap(), Err(Error::Forbidden));
    assert_eq!(rows(&h)[0].1, "reserved");
}
#[test]
fn confirm_and_cancel_require_same_live_parent_and_do_not_allocate_new_trial() {
    let (_d, s, o, g) = nine();
    let h = s.handle();
    let trial = "a".repeat(64);
    drop(start(&h, &g, 2, &trial));
    let other = h
        .change_recovery_grant(
            o,
            [3; 32],
            0,
            request(20),
            RecoveryChange::Grant {
                max_trial_ms: 40000,
            },
        )
        .unwrap();
    for key in [&g, &other] {
        let target = if key == &g {
            "b".repeat(64)
        } else {
            trial.clone()
        };
        assert!(matches!(
            h.reserve_recovery_execution_from_trusted_auth(input(
                key,
                3,
                RecoveryExecutionAction::ConfirmWifi,
                7,
                Some(target)
            )),
            Err(Error::Conflict)
        ));
    }
    for (n, a) in [
        (3, RecoveryExecutionAction::ConfirmWifi),
        (4, RecoveryExecutionAction::CancelWifi),
    ] {
        let r = reserve(&h, input(&g, n, a, 8, Some(trial.clone())));
        assert!(matches!(
            h.claim_recovery_execution_from_trusted_auth(duplicate(&r), Some("b".repeat(64))),
            Err(Error::InvalidInput)
        ));
        let c = h
            .claim_recovery_execution_from_trusted_auth(r, None)
            .unwrap();
        assert_eq!(c.receipt().trial_id(), Some(trial.as_str()));
        assert!(matches!(
            h.reserve_recovery_execution_from_trusted_auth(input(
                &g,
                n,
                a,
                8,
                Some("b".repeat(64))
            )),
            Err(Error::Conflict)
        ));
    }
}
#[test]
fn full_noncancel_budget_retains_exactly_one_cancel_slot_and_existing_queries() {
    let (_d, s, _o, g) = nine();
    let h = s.handle();
    let trial = "a".repeat(64);
    drop(start(&h, &g, 2, &trial));
    for n in 100..354 {
        seed(&h, &g, n, "begin_wifi", "rejected", None);
    }
    assert_eq!(rows(&h).len(), 255);
    assert!(matches!(
        h.reserve_recovery_execution_from_trusted_auth(input(
            &g,
            3,
            RecoveryExecutionAction::ConfirmWifi,
            7,
            Some(trial.clone())
        )),
        Err(Error::Full)
    ));
    let r = reserve(
        &h,
        input(
            &g,
            4,
            RecoveryExecutionAction::CancelWifi,
            7,
            Some(trial.clone()),
        ),
    );
    drop(
        h.claim_recovery_execution_from_trusted_auth(r, None)
            .unwrap(),
    );
    assert!(matches!(
        h.reserve_recovery_execution_from_trusted_auth(input(
            &g,
            5,
            RecoveryExecutionAction::CancelWifi,
            7,
            Some(trial)
        )),
        Err(Error::Full)
    ));
    assert_eq!(
        lookup(&h, &g, 4).unwrap().state(),
        RecoveryExecutionState::Claimed
    );
    assert_eq!(rows(&h).len(), 256);
}
#[test]
fn identical_uuid_in_another_grant_cannot_bypass_begin_exclusion() {
    let (_d, s, o, g) = nine();
    let h = s.handle();
    let other = h
        .change_recovery_grant(
            o,
            [3; 32],
            0,
            request(20),
            RecoveryChange::Grant {
                max_trial_ms: 40000,
            },
        )
        .unwrap();
    let r = reserve(
        &h,
        input(&g, 2, RecoveryExecutionAction::BeginWifi, 7, None),
    );
    seed(&h, &other, 2, "begin_wifi", "reserved", None);
    assert!(matches!(
        h.claim_recovery_execution_from_trusted_auth(r, Some("a".repeat(64))),
        Err(Error::Conflict)
    ));
    assert!(rows(&h).iter().all(|r| r.1 == "reserved"));
}
#[test]
fn old_trial_collision_and_malformed_input_leave_original_reservation() {
    let (_d, s, _o, g) = nine();
    let h = s.handle();
    let trial = "a".repeat(64);
    seed(&h, &g, 2, "begin_wifi", "rejected", Some(trial.clone()));
    let r = reserve(
        &h,
        input(&g, 3, RecoveryExecutionAction::BeginWifi, 7, None),
    );
    assert!(matches!(
        h.claim_recovery_execution_from_trusted_auth(duplicate(&r), Some(trial)),
        Err(Error::Conflict)
    ));
    for t in [None, Some("A".repeat(64)), Some("a".repeat(63))] {
        assert!(matches!(
            h.claim_recovery_execution_from_trusted_auth(duplicate(&r), t),
            Err(Error::InvalidInput)
        ));
    }
    assert!(matches!(
        TrustedRecoveryExecution::from_trusted_auth(
            g,
            "invalid UUID".into(),
            RecoveryExecutionAction::BeginWifi,
            [7; 32],
            None,
            fence()
        ),
        Err(Error::InvalidInput)
    ));
    assert_eq!(rows(&h)[1].1, "reserved");
}
#[test]
fn actual_sqlite_full_reserve_rolls_back_without_partial_or_retryable_claim() {
    let (_d, s, _o, g) = nine();
    let h = s.handle();
    let trial = "a".repeat(64);
    drop(start(&h, &g, 2, &trial));
    h.call(|db| {
        let pages: i64 = db
            .connection
            .pragma_query_value(None, "page_count", |r| r.get(0))?;
        db.connection.pragma_update(None, "max_page_count", pages)?;
        Ok(())
    })
    .unwrap();
    let mut failed = None;
    for n in 3..200 {
        match h.reserve_recovery_execution_from_trusted_auth(input(
            &g,
            n,
            RecoveryExecutionAction::ConfirmWifi,
            7,
            Some(trial.clone()),
        )) {
            Ok(_) => (),
            Err(Error::Full) => {
                failed = Some(n);
                break;
            }
            Err(e) => panic!("unexpected {e}"),
        }
    }
    let n = failed.expect("real SQLite max_page_count must fail before logical capacity");
    assert!(rows(&h).len() < 255);
    assert!(lookup(&h, &g, n).is_none());
    // Err returns no reservation ticket, so there is no public way to claim n.
}
#[test]
fn lookup_found_and_missing_are_byte_preserving_and_not_dispatch_permits() {
    let (_d, s, _o, g) = nine();
    let h = s.handle();
    drop(start(&h, &g, 2, &"a".repeat(64)));
    let b1 = directory();
    let b2 = directory();
    h.backup(b1.path()).unwrap();
    assert_eq!(
        lookup(&h, &g, 2).unwrap().state(),
        RecoveryExecutionState::Claimed
    );
    assert!(lookup(&h, &g, 99).is_none());
    h.backup(b2.path()).unwrap();
    assert_eq!(
        fs::read(b1.path().join("backup.sqlite3")).unwrap(),
        fs::read(b2.path().join("backup.sqlite3")).unwrap()
    );
}

#[test]
fn restored_old_realm_unknown_blocks_new_grant_even_after_owner_recovery() {
    let (_d, s, _o, g) = nine();
    let h = s.handle();
    drop(start(&h, &g, 2, &"a".repeat(64)));
    let backup = directory();
    h.backup(backup.path()).unwrap();
    let destination = directory();
    let restored = Store::restore_recovery_execution(backup.path(), destination.path()).unwrap();
    let h = restored.handle();
    let inactive = h
        .principals()
        .unwrap()
        .into_iter()
        .find(|p| p.public_key == [1; 32])
        .unwrap();
    assert!(!inactive.active);
    let owner = h.recover_owner(inactive).unwrap();
    let current = h
        .change_recovery_grant(
            owner,
            [2; 32],
            g.generation,
            request(90),
            RecoveryChange::Grant {
                max_trial_ms: 40000,
            },
        )
        .unwrap();
    assert_ne!(current.realm, g.realm);
    assert!(current.generation > g.generation);
    h.check_recovery_grant(current.clone()).unwrap();
    assert!(matches!(
        h.reserve_recovery_execution_from_trusted_auth(input(
            &current,
            3,
            RecoveryExecutionAction::BeginWifi,
            7,
            None
        )),
        Err(Error::Conflict)
    ));
    assert_eq!(rows(&h)[0].1, "unknown");
    assert_eq!(rows(&h).len(), 1);
}

#[test]
fn terminal_parent_and_old_grant_open_mode_do_not_admit_new_controls() {
    let (_d, s, _o, g) = nine();
    let h = s.handle();
    let trial = "a".repeat(64);
    seed(&h, &g, 2, "begin_wifi", "rolled_back", Some(trial.clone()));
    for action in [
        RecoveryExecutionAction::ConfirmWifi,
        RecoveryExecutionAction::CancelWifi,
    ] {
        assert!(matches!(
            h.reserve_recovery_execution_from_trusted_auth(input(
                &g,
                3,
                action,
                7,
                Some(trial.clone())
            )),
            Err(Error::Conflict)
        ));
    }
    let (_d, s, _o, g) = super::super::tests::seven();
    assert!(matches!(
        s.handle()
            .reserve_recovery_execution_from_trusted_auth(input(
                &g,
                2,
                RecoveryExecutionAction::BeginWifi,
                7,
                None
            )),
        Err(Error::InvalidTransition)
    ));
}
