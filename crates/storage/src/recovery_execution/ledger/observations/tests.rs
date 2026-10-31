// SPDX-License-Identifier: AGPL-3.0-only
//! Actual actor/SQLite/Backup; synthetic observations are trusted test inputs,
//! not claims of NetworkManager or hardware acceptance.
use super::*;
use crate::recovery_execution::tests::{directory, nine, request, rows};
use std::sync::Barrier;
fn input(
    g: &RecoveryGrant,
    n: u32,
    action: RecoveryExecutionAction,
    target: Option<String>,
) -> TrustedRecoveryExecution {
    TrustedRecoveryExecution::from_trusted_auth(
        g.clone(),
        request(n),
        action,
        [n as u8; 32],
        target,
        RecoveryExecutionFence::from_trusted_auth(
            Arc::new(AtomicBool::new(true)),
            Instant::now() + Duration::from_secs(60),
        ),
    )
    .unwrap()
}
fn claim(
    h: &Handle,
    g: &RecoveryGrant,
    n: u32,
    action: RecoveryExecutionAction,
    target: Option<String>,
) -> StorageRecoveryExecutionClaim {
    let i = input(g, n, action, target);
    let r = match h.reserve_recovery_execution_from_trusted_auth(i).unwrap() {
        RecoveryExecutionReservation::Reserved(r) => r,
        _ => panic!("new ticket expected"),
    };
    h.claim_recovery_execution_from_trusted_auth(
        r,
        (action == RecoveryExecutionAction::BeginWifi).then(|| "a".repeat(64)),
    )
    .unwrap()
}
fn progress(
    h: &Handle,
    g: &RecoveryGrant,
    b: &StorageRecoveryExecutionClaim,
    p: RecoveryExecutionProgress,
) -> Result<RecoveryExecutionReceipt> {
    h.record_recovery_progress_from_trusted_core(
        input(g, 2, RecoveryExecutionAction::BeginWifi, None),
        b,
        p,
    )
}
fn ready(h: &Handle, g: &RecoveryGrant) -> StorageRecoveryExecutionClaim {
    let b = claim(h, g, 2, RecoveryExecutionAction::BeginWifi, None);
    for p in [
        RecoveryExecutionProgress::Starting,
        RecoveryExecutionProgress::Connecting,
        RecoveryExecutionProgress::Ready,
    ] {
        progress(h, g, &b, p).unwrap();
    }
    b
}
fn confirm(h: &Handle, g: &RecoveryGrant) -> StorageRecoveryExecutionClaim {
    claim(
        h,
        g,
        3,
        RecoveryExecutionAction::ConfirmWifi,
        Some("a".repeat(64)),
    )
}
fn saving(
    h: &Handle,
    g: &RecoveryGrant,
    b: &StorageRecoveryExecutionClaim,
    c: &StorageRecoveryExecutionClaim,
) -> Result<()> {
    h.record_recovery_saving_from_trusted_core(
        input(g, 2, RecoveryExecutionAction::BeginWifi, None),
        b,
        input(
            g,
            3,
            RecoveryExecutionAction::ConfirmWifi,
            Some("a".repeat(64)),
        ),
        c,
    )
}
fn read(h: &Handle, g: &RecoveryGrant, n: u32) -> RecoveryExecutionReceipt {
    h.lookup_recovery_execution_from_trusted_auth(
        g.clone(),
        request(n),
        RecoveryExecutionFence::from_trusted_auth(
            Arc::new(AtomicBool::new(true)),
            Instant::now() + Duration::from_secs(60),
        ),
    )
    .unwrap()
    .unwrap()
}
#[test]
fn actual_progress_remains_authorized_and_saving_then_saved_updates_both_rows() {
    let (_d, s, _o, g) = nine();
    let h = s.handle();
    let b = ready(&h, &g);
    h.check_recovery_execution_claim_from_trusted_auth(
        input(&g, 2, RecoveryExecutionAction::BeginWifi, None),
        &b,
    )
    .unwrap();
    assert!(matches!(
        h.check_recovery_execution_start_from_trusted_auth(
            input(&g, 2, RecoveryExecutionAction::BeginWifi, None),
            &b
        ),
        Err(Error::Forbidden)
    ));
    let c = confirm(&h, &g);
    saving(&h, &g, &b, &c).unwrap();
    saving(&h, &g, &b, &c).unwrap();
    for n in [2, 3] {
        assert_eq!(read(&h, &g, n).state(), RecoveryExecutionState::Saving);
    }
    h.check_recovery_execution_claim_from_trusted_auth(
        input(
            &g,
            3,
            RecoveryExecutionAction::ConfirmWifi,
            Some("a".repeat(64)),
        ),
        &c,
    )
    .unwrap();
    let r = h
        .finish_recovery_trial_from_trusted_core(&b, Some(&c), RecoveryExecutionCompletion::Saved)
        .unwrap();
    assert_eq!(r.state(), RecoveryExecutionState::Saved);
    assert_eq!(r.settings_saved(), Some(true));
    assert_eq!(r.rollback_verified(), None);
    assert_eq!(read(&h, &g, 3).state(), RecoveryExecutionState::Saved);
    assert!(h
        .check_recovery_execution_claim_from_trusted_auth(
            input(&g, 2, RecoveryExecutionAction::BeginWifi, None),
            &b
        )
        .is_err());
    h.finish_recovery_trial_from_trusted_core(&b, Some(&c), RecoveryExecutionCompletion::Saved)
        .unwrap();
}
#[test]
fn skipped_reverse_or_control_progress_and_unconfirmed_save_are_denied() {
    let (_d, s, _o, g) = nine();
    let h = s.handle();
    let b = claim(&h, &g, 2, RecoveryExecutionAction::BeginWifi, None);
    for p in [
        RecoveryExecutionProgress::Connecting,
        RecoveryExecutionProgress::Ready,
    ] {
        assert!(matches!(
            progress(&h, &g, &b, p),
            Err(Error::InvalidTransition)
        ));
    }
    assert!(h
        .finish_recovery_trial_from_trusted_core(&b, None, RecoveryExecutionCompletion::Saved)
        .is_err());
    progress(&h, &g, &b, RecoveryExecutionProgress::Starting).unwrap();
    progress(&h, &g, &b, RecoveryExecutionProgress::Starting).unwrap();
    progress(&h, &g, &b, RecoveryExecutionProgress::Connecting).unwrap();
    progress(&h, &g, &b, RecoveryExecutionProgress::Ready).unwrap();
    assert!(matches!(
        progress(&h, &g, &b, RecoveryExecutionProgress::Starting),
        Err(Error::InvalidTransition)
    ));
    progress(&h, &g, &b, RecoveryExecutionProgress::Connecting).unwrap();
    progress(&h, &g, &b, RecoveryExecutionProgress::Ready).unwrap();
    let c = confirm(&h, &g);
    assert!(matches!(
        h.record_recovery_progress_from_trusted_core(
            input(
                &g,
                3,
                RecoveryExecutionAction::ConfirmWifi,
                Some("a".repeat(64))
            ),
            &c,
            RecoveryExecutionProgress::Starting
        ),
        Err(Error::InvalidTransition)
    ));
    assert!(h
        .finish_recovery_trial_from_trusted_core(&b, Some(&c), RecoveryExecutionCompletion::Saved)
        .is_err());
    let cancel = claim(
        &h,
        &g,
        4,
        RecoveryExecutionAction::CancelWifi,
        Some("a".repeat(64)),
    );
    assert!(h
        .record_recovery_saving_from_trusted_core(
            input(&g, 2, RecoveryExecutionAction::BeginWifi, None),
            &b,
            input(
                &g,
                4,
                RecoveryExecutionAction::CancelWifi,
                Some("a".repeat(64))
            ),
            &cancel
        )
        .is_err());
    assert_eq!(read(&h, &g, 2).state(), RecoveryExecutionState::Ready);
}
#[test]
fn real_mid_pair_sqlite_failure_rolls_back_first_row_and_keeps_both_original_states() {
    for finishing in [false, true] {
        let (_d, s, _o, g) = nine();
        let h = s.handle();
        let b = ready(&h, &g);
        let c = confirm(&h, &g);
        if finishing {
            saving(&h, &g, &b, &c).unwrap();
        }
        h.call(|db|{db.connection.execute_batch("CREATE TEMP TRIGGER test_abort_confirm BEFORE UPDATE ON main.recovery_execution_receipts WHEN NEW.action='confirm_wifi' BEGIN SELECT RAISE(ABORT,'synthetic confirmation write failure'); END;")?;Ok(())}).unwrap();
        let result = if finishing {
            h.finish_recovery_trial_from_trusted_core(
                &b,
                Some(&c),
                RecoveryExecutionCompletion::Saved,
            )
            .map(|_| ())
        } else {
            saving(&h, &g, &b, &c)
        };
        assert_eq!(result, Err(Error::Database));
        assert_eq!(
            read(&h, &g, 2).state(),
            if finishing {
                RecoveryExecutionState::Saving
            } else {
                RecoveryExecutionState::Ready
            }
        );
        assert_eq!(
            read(&h, &g, 3).state(),
            if finishing {
                RecoveryExecutionState::Saving
            } else {
                RecoveryExecutionState::Claimed
            }
        );
        for n in [2, 3] {
            assert_eq!(read(&h, &g, n).settings_saved(), None);
            assert_eq!(read(&h, &g, n).rollback_verified(), None);
        }
        h.call(|db| {
            db.connection
                .execute_batch("DROP TRIGGER temp.test_abort_confirm")?;
            Ok(())
        })
        .unwrap();
        if finishing {
            h.finish_recovery_trial_from_trusted_core(
                &b,
                Some(&c),
                RecoveryExecutionCompletion::Saved,
            )
            .unwrap();
        } else {
            saving(&h, &g, &b, &c).unwrap();
        }
    }
}

#[test]
fn revoked_authority_cannot_advance_but_verified_rollback_is_accounted_without_new_permission() {
    let (_d, s, o, g) = nine();
    let h = s.handle();
    let b = ready(&h, &g);
    let c = confirm(&h, &g);
    saving(&h, &g, &b, &c).unwrap();
    h.change_recovery_grant(
        o,
        [2; 32],
        g.generation,
        request(90),
        RecoveryChange::Revoke,
    )
    .unwrap();
    assert!(progress(&h, &g, &b, RecoveryExecutionProgress::Connecting).is_err());
    let r = h
        .finish_recovery_trial_from_trusted_core(
            &b,
            Some(&c),
            RecoveryExecutionCompletion::RolledBack,
        )
        .unwrap();
    assert_eq!(r.state(), RecoveryExecutionState::RolledBack);
    assert_eq!(r.settings_saved(), Some(false));
    assert_eq!(r.rollback_verified(), Some(true));
    h.finish_recovery_trial_from_trusted_core(
        &b,
        Some(&c),
        RecoveryExecutionCompletion::RolledBack,
    )
    .unwrap();
    assert!(h
        .check_recovery_execution_claim_from_trusted_auth(
            input(&g, 2, RecoveryExecutionAction::BeginWifi, None),
            &b
        )
        .is_err());
    assert!(h
        .finish_recovery_trial_from_trusted_core(&b, Some(&c), RecoveryExecutionCompletion::Saved)
        .is_err());
}
#[test]
fn cancel_has_rollback_evidence_and_competing_unexecuted_request_remains_unknown() {
    let (_d, s, _o, g) = nine();
    let h = s.handle();
    let b = ready(&h, &g);
    let c = confirm(&h, &g);
    let cancel = claim(
        &h,
        &g,
        4,
        RecoveryExecutionAction::CancelWifi,
        Some("a".repeat(64)),
    );
    h.finish_recovery_trial_from_trusted_core(
        &b,
        Some(&cancel),
        RecoveryExecutionCompletion::RolledBack,
    )
    .unwrap();
    assert_eq!(read(&h, &g, 4).rollback_verified(), Some(true));
    assert_eq!(read(&h, &g, 3).state(), RecoveryExecutionState::Unknown);
    assert!(h
        .finish_recovery_trial_from_trusted_core(&b, Some(&c), RecoveryExecutionCompletion::Saved)
        .is_err());
    assert!(h
        .reserve_recovery_execution_from_trusted_auth(input(
            &g,
            5,
            RecoveryExecutionAction::BeginWifi,
            None
        ))
        .is_err());
}
#[test]
fn unknown_and_reopen_never_accept_late_success_or_rewind_to_a_live_state() {
    for reopen in [false, true] {
        let (d, s, _o, g) = nine();
        let h = s.handle();
        let b = ready(&h, &g);
        let c = confirm(&h, &g);
        saving(&h, &g, &b, &c).unwrap();
        let s = if reopen {
            drop(s);
            Store::open_recovery_execution(d.path()).unwrap()
        } else {
            h.finish_recovery_trial_from_trusted_core(
                &b,
                Some(&c),
                RecoveryExecutionCompletion::Unknown,
            )
            .unwrap();
            s
        };
        let h = s.handle();
        assert_eq!(read(&h, &g, 2).state(), RecoveryExecutionState::Unknown);
        for outcome in [
            RecoveryExecutionCompletion::Saved,
            RecoveryExecutionCompletion::RolledBack,
        ] {
            assert!(matches!(
                h.finish_recovery_trial_from_trusted_core(&b, Some(&c), outcome),
                Err(Error::InvalidTransition)
            ));
        }
        assert!(progress(&h, &g, &b, RecoveryExecutionProgress::Connecting).is_err());
        h.finish_recovery_trial_from_trusted_core(
            &b,
            Some(&c),
            RecoveryExecutionCompletion::Unknown,
        )
        .unwrap();
    }
}
#[test]
fn foreign_claim_and_changed_fingerprint_cannot_record_a_fact_for_original_trial() {
    let (_d, s, _o, g) = nine();
    let h = s.handle();
    let b = ready(&h, &g);
    let c = confirm(&h, &g);
    let (_d2, s2, _o2, g2) = nine();
    let h2 = s2.handle();
    let b2 = ready(&h2, &g2);
    let c2 = confirm(&h2, &g2);
    assert!(h
        .finish_recovery_trial_from_trusted_core(
            &b,
            Some(&c2),
            RecoveryExecutionCompletion::RolledBack
        )
        .is_err());
    assert!(h2
        .finish_recovery_trial_from_trusted_core(
            &b2,
            Some(&c),
            RecoveryExecutionCompletion::RolledBack
        )
        .is_err());
    let bad = StorageRecoveryExecutionClaim {
        receipt: RecoveryExecutionReceipt(Receipt {
            fingerprint: vec![99; 32],
            ..b.receipt.0.clone()
        }),
    };
    assert!(matches!(
        h.finish_recovery_trial_from_trusted_core(
            &bad,
            None,
            RecoveryExecutionCompletion::RolledBack
        ),
        Err(Error::Forbidden)
    ));
    assert_eq!(read(&h, &g, 2).state(), RecoveryExecutionState::Ready);
}
#[test]
fn competing_terminal_writes_have_one_outcome_and_no_split_begin_confirm_evidence() {
    let (_d, s, _o, g) = nine();
    let h = s.handle();
    let b = ready(&h, &g);
    let c = confirm(&h, &g);
    saving(&h, &g, &b, &c).unwrap();
    let barrier = Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let jobs: Vec<_> = [
            RecoveryExecutionCompletion::Saved,
            RecoveryExecutionCompletion::RolledBack,
        ]
        .into_iter()
        .map(|outcome| {
            let h = h.clone();
            let (b, c, barrier) = (&b, &c, &barrier);
            scope.spawn(move || {
                barrier.wait();
                h.finish_recovery_trial_from_trusted_core(b, Some(c), outcome)
            })
        })
        .collect();
        jobs.into_iter()
            .map(|j| j.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(read(&h, &g, 2).state(), read(&h, &g, 3).state());
}
#[test]
fn durable_saved_pair_and_exact_evidence_survive_backup_restore_as_history() {
    let (_d, s, _o, g) = nine();
    let h = s.handle();
    let b = ready(&h, &g);
    let c = confirm(&h, &g);
    saving(&h, &g, &b, &c).unwrap();
    h.finish_recovery_trial_from_trusted_core(&b, Some(&c), RecoveryExecutionCompletion::Saved)
        .unwrap();
    let backup = directory();
    h.backup(backup.path()).unwrap();
    let destination = directory();
    let restored = Store::restore_recovery_execution(backup.path(), destination.path()).unwrap();
    let states = rows(&restored.handle());
    assert!(states.iter().all(|r| r.1 == "saved"));
    assert!(restored
        .handle()
        .check_recovery_execution_claim_from_trusted_auth(
            input(&g, 2, RecoveryExecutionAction::BeginWifi, None),
            &b
        )
        .is_err());
}
#[test]
fn disconnect_blocks_saving_and_recording_progress_but_not_bounded_cleanup_accounting() {
    let (_d, s, _o, g) = nine();
    let h = s.handle();
    let b = ready(&h, &g);
    let c = confirm(&h, &g);
    let begin = input(&g, 2, RecoveryExecutionAction::BeginWifi, None);
    begin.fence.live.store(false, Ordering::Release);
    assert!(h
        .record_recovery_saving_from_trusted_core(
            begin,
            &b,
            input(
                &g,
                3,
                RecoveryExecutionAction::ConfirmWifi,
                Some("a".repeat(64))
            ),
            &c
        )
        .is_err());
    let begin = input(&g, 2, RecoveryExecutionAction::BeginWifi, None);
    begin.fence.live.store(false, Ordering::Release);
    assert!(h
        .record_recovery_progress_from_trusted_core(
            begin,
            &b,
            RecoveryExecutionProgress::Connecting
        )
        .is_err());
    h.finish_recovery_trial_from_trusted_core(
        &b,
        Some(&c),
        RecoveryExecutionCompletion::RolledBack,
    )
    .unwrap();
}
