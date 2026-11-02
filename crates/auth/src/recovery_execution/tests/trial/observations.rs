// SPDX-License-Identifier: AGPL-3.0-only
//! Real Auth/Store9. Backend observations and A proofs are synthetic inputs;
//! this verifies the actual bookkeeping/gates, not real Wi-Fi save evidence.
use super::*;
use rustsimhub_storage::{
    RecoveryExecutionCompletion as Completion, RecoveryExecutionProgress as Progress,
    RecoveryExecutionState as State,
};
fn ready(f: &Fixture, t: &RecoveryTrialAuthority) {
    for p in [Progress::Starting, Progress::Connecting, Progress::Ready] {
        f.a.record_recovery_trial_progress_from_trusted_core(t, p)
            .unwrap();
        f.a.check_recovery_trial(t).unwrap();
    }
}
fn control(
    f: &mut Fixture,
    c: &RecoveryConnection,
    t: &RecoveryTrialAuthority,
    n: u8,
) -> crate::RecoveryTrialControl {
    let claim = claim(f, c, n, Some(t.trial_id().into()));
    f.a.bind_recovery_trial_control(t, claim).unwrap()
}
#[test]
fn real_authority_survives_progress_and_saving_but_not_saved_terminal() {
    let mut f = fixture(true);
    let c = script(&mut f);
    let t = begin(&mut f, &c);
    let deadline = t.deadline();
    ready(&f, &t);
    let confirm = control(&mut f, &c, &t, 2);
    f.a.record_recovery_trial_saving_from_trusted_core(&t, &confirm)
        .unwrap();
    f.a.check_recovery_trial_control(&t, &confirm, RecoveryAction::ConfirmWifi)
        .unwrap();
    assert_eq!(t.deadline(), deadline);
    let r =
        f.a.finish_recovery_trial_from_trusted_core(&t, Some(&confirm), Completion::Saved)
            .unwrap();
    assert_eq!(r.state(), State::Saved);
    assert_eq!(r.settings_saved(), Some(true));
    assert!(f.a.check_recovery_trial(&t).is_err());
    assert!(f
        .a
        .check_recovery_trial_control(&t, &confirm, RecoveryAction::ConfirmWifi)
        .is_err());
    f.a.finish_recovery_trial_from_trusted_core(&t, Some(&confirm), Completion::Saved)
        .unwrap();
}
#[test]
fn no_confirmation_or_cancel_cannot_be_used_as_save_accounting() {
    let mut f = fixture(true);
    let c = script(&mut f);
    let t = begin(&mut f, &c);
    ready(&f, &t);
    assert!(matches!(
        f.a.finish_recovery_trial_from_trusted_core(&t, None, Completion::Saved),
        Err(Error::InvalidInput)
    ));
    let confirm = control(&mut f, &c, &t, 2);
    assert!(matches!(
        f.a.finish_recovery_trial_from_trusted_core(&t, Some(&confirm), Completion::Saved),
        Err(Error::Storage)
    ));
    let cancel = control(&mut f, &c, &t, 3);
    assert!(matches!(
        f.a.record_recovery_trial_saving_from_trusted_core(&t, &cancel),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        f.a.finish_recovery_trial_from_trusted_core(&t, Some(&cancel), Completion::Saved),
        Err(Error::InvalidInput)
    ));
    f.a.check_recovery_trial(&t).unwrap();
}
#[test]
fn actual_revoke_before_cleanup_keeps_accounting_but_never_restores_authority() {
    let mut f = fixture(true);
    let c = script(&mut f);
    let t = begin(&mut f, &c);
    ready(&f, &t);
    let confirm = control(&mut f, &c, &t, 2);
    f.a.record_recovery_trial_saving_from_trusted_core(&t, &confirm)
        .unwrap();
    f.a.revoke_recovery_key(
        f.token.expose(),
        f.grant.independent_public_key,
        f.grant.generation,
        id(90),
    )
    .unwrap();
    assert!(f
        .a
        .check_recovery_trial_control(&t, &confirm, RecoveryAction::ConfirmWifi)
        .is_err());
    let r =
        f.a.finish_recovery_trial_from_trusted_core(&t, Some(&confirm), Completion::RolledBack)
            .unwrap();
    assert_eq!(r.settings_saved(), Some(false));
    assert_eq!(r.rollback_verified(), Some(true));
    assert!(f.a.check_recovery_trial(&t).is_err());
}
#[test]
fn disconnect_and_expiry_allow_only_fact_recording_and_unknown_cannot_be_cleared() {
    for expire in [false, true] {
        let mut f = fixture(true);
        let c = script(&mut f);
        let mut t = begin(&mut f, &c);
        ready(&f, &t);
        let confirm = control(&mut f, &c, &t, 2);
        if expire {
            t.expire_for_test()
        } else {
            c.invalidate()
        }
        assert!(f
            .a
            .record_recovery_trial_saving_from_trusted_core(&t, &confirm)
            .is_err());
        let r =
            f.a.finish_recovery_trial_from_trusted_core(&t, Some(&confirm), Completion::Unknown)
                .unwrap();
        assert_eq!(r.state(), State::Unknown);
        assert_eq!(r.settings_saved(), None);
        assert_eq!(r.rollback_verified(), None);
        assert!(f
            .a
            .finish_recovery_trial_from_trusted_core(&t, Some(&confirm), Completion::RolledBack)
            .is_err());
        assert!(f
            .a
            .record_recovery_trial_progress_from_trusted_core(&t, Progress::Connecting)
            .is_err());
    }
}
#[test]
fn progressed_private_claim_cannot_mint_a_new_trial_window() {
    let mut f = fixture(true);
    let c = script(&mut f);
    let b = claim(&mut f, &c, 1, None);
    f.s.handle()
        .record_recovery_progress_from_trusted_core(
            f.a.trusted_execution(b.request()).unwrap(),
            b.storage_claim(),
            Progress::Starting,
        )
        .unwrap();
    assert!(matches!(f.a.begin_recovery_trial(b), Err(Error::Storage)));
}
#[test]
fn other_auth_or_bound_control_cannot_finish_an_original_trial() {
    let mut f = fixture(true);
    let c = script(&mut f);
    let t = begin(&mut f, &c);
    ready(&f, &t);
    let mut other = fixture(true);
    let c2 = script(&mut other);
    let t2 = begin(&mut other, &c2);
    ready(&other, &t2);
    let control2 = control(&mut other, &c2, &t2, 2);
    assert!(matches!(
        other
            .a
            .finish_recovery_trial_from_trusted_core(&t, None, Completion::RolledBack),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        f.a.finish_recovery_trial_from_trusted_core(&t, Some(&control2), Completion::RolledBack),
        Err(Error::Forbidden)
    ));
    f.a.check_recovery_trial(&t).unwrap();
}
#[test]
fn selected_confirm_succeeds_but_unexecuted_cancel_keeps_its_unknown_record() {
    let mut f = fixture(true);
    let c = script(&mut f);
    let t = begin(&mut f, &c);
    ready(&f, &t);
    let confirm = control(&mut f, &c, &t, 2);
    let cancel = control(&mut f, &c, &t, 3);
    f.a.record_recovery_trial_saving_from_trusted_core(&t, &confirm)
        .unwrap();
    f.a.finish_recovery_trial_from_trusted_core(&t, Some(&confirm), Completion::Saved)
        .unwrap();
    assert!(f
        .a
        .check_recovery_trial_control(&t, &cancel, RecoveryAction::CancelWifi)
        .is_err());
    assert!(f
        .a
        .finish_recovery_trial_from_trusted_core(&t, Some(&cancel), Completion::RolledBack)
        .is_err());
}
#[test]
fn stopping_after_ready_without_control_can_record_actual_rollback() {
    let mut f = fixture(true);
    let c = script(&mut f);
    let t = begin(&mut f, &c);
    ready(&f, &t);
    drop(c);
    let r =
        f.a.finish_recovery_trial_from_trusted_core(&t, None, Completion::RolledBack)
            .unwrap();
    assert_eq!(r.state(), State::RolledBack);
    assert_eq!(r.rollback_verified(), Some(true));
    assert!(f
        .a
        .finish_recovery_trial_from_trusted_core(&t, None, Completion::Saved)
        .is_err());
}
