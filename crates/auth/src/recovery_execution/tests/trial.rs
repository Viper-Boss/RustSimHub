// SPDX-License-Identifier: AGPL-3.0-only
//! Real Auth/Identity/Store9; scripted A adapter only in this test module.
use super::*;
use crate::{RecoveryExecutionAdmission, RecoveryExecutionClaim, RecoveryTrialAuthority};
use std::collections::HashMap;
mod observations;
struct Script(HashMap<u8, (RecoveryAction, Vec<u8>)>, Duration);
impl RecoveryRequestVerifierV1 for Script {
    fn verify_request(
        &self,
        c: &RecoveryProofContext,
        e: &[u8],
    ) -> Result<VerifiedRecoveryRequest> {
        let r: [u8; 16] = e.try_into().map_err(|_| Error::InvalidInput)?;
        let (a, b) = self.0.get(&r[0]).ok_or(Error::InvalidInput)?;
        VerifiedRecoveryRequest::from_verified_adapter(
            c.binding().clone(),
            vec![8; 16],
            r,
            *a,
            Zeroizing::new(b.clone()),
            self.1,
        )
    }
}
fn script(f: &mut Fixture) -> RecoveryConnection {
    script_ttl(f, Duration::from_secs(30))
}
fn script_ttl(f: &mut Fixture, ttl: Duration) -> RecoveryConnection {
    let trial = "a".repeat(64);
    let body = format!("{{\"trial_id\":\"{trial}\"}}").into_bytes();
    f.a.set_recovery_verifier(Some(Arc::new(Script(
        HashMap::from([
            (1, (RecoveryAction::BeginWifi, BODY.into())),
            (2, (RecoveryAction::ConfirmWifi, body.clone())),
            (3, (RecoveryAction::CancelWifi, body)),
        ]),
        ttl,
    ))))
    .unwrap();
    f.a.open_recovery_connection_from_trusted_transport([11; 32])
        .unwrap()
}
fn claim(
    f: &mut Fixture,
    c: &RecoveryConnection,
    n: u8,
    trial: Option<String>,
) -> RecoveryExecutionClaim {
    let r = authorize(f, c, n);
    let p = f.a.prepare_recovery_execution(r, trial).unwrap();
    let p = match f.a.reserve_recovery_execution(p).unwrap() {
        RecoveryExecutionAdmission::Reserved(p) => p,
        _ => panic!("expected new ticket"),
    };
    f.a.claim_recovery_execution(p, (n == 1).then(|| "a".repeat(64)))
        .unwrap()
}
fn begin(f: &mut Fixture, c: &RecoveryConnection) -> RecoveryTrialAuthority {
    let b = claim(f, c, 1, None);
    f.a.begin_recovery_trial(b).unwrap()
}
#[test]
fn begin_derives_original_trial_without_principal_and_finite_original_deadline() {
    let mut f = fixture(true);
    let c = script(&mut f);
    let b = claim(&mut f, &c, 1, None);
    let capability_deadline = b.request().capability().execution_deadline();
    let before = std::time::Instant::now();
    let t = f.a.begin_recovery_trial(b).unwrap();
    assert_eq!(t.trial_id(), "a".repeat(64));
    assert_eq!(t.grant(), &f.grant);
    assert!(t.deadline() <= before + Duration::from_secs(40));
    assert_eq!(t.deadline(), capability_deadline);
    assert!(t.remaining() <= Duration::from_secs(30));
    f.a.check_recovery_trial(&t).unwrap();
}
#[test]
fn grant_budget_can_only_shorten_trial_and_confirmation_cannot_extend_it() {
    let mut f = fixture(true);
    let revoked =
        f.a.revoke_recovery_key(
            f.token.expose(),
            f.grant.independent_public_key,
            f.grant.generation,
            id(80),
        )
        .unwrap();
    f.grant =
        f.a.grant_recovery_key(
            f.token.expose(),
            revoked.independent_public_key,
            12340,
            revoked.generation,
            id(81),
        )
        .unwrap();
    let c = script(&mut f);
    let b = claim(&mut f, &c, 1, None);
    let before = std::time::Instant::now();
    let t = f.a.begin_recovery_trial(b).unwrap();
    let after = std::time::Instant::now();
    let original = t.deadline();
    assert!(original >= before + Duration::from_millis(12340));
    assert!(original <= after + Duration::from_millis(12340));
    let confirm = claim(&mut f, &c, 2, Some(t.trial_id().into()));
    let control = f.a.bind_recovery_trial_control(&t, confirm).unwrap();
    f.a.check_recovery_trial_control(&t, &control, RecoveryAction::ConfirmWifi)
        .unwrap();
    assert_eq!(t.deadline(), original);
}
#[test]
fn confirm_and_cancel_require_their_own_committed_action_capabilities() {
    let mut f = fixture(true);
    let c = script(&mut f);
    let t = begin(&mut f, &c);
    for (n, expected) in [
        (2, RecoveryAction::ConfirmWifi),
        (3, RecoveryAction::CancelWifi),
    ] {
        let claim = claim(&mut f, &c, n, Some(t.trial_id().into()));
        let control = f.a.bind_recovery_trial_control(&t, claim).unwrap();
        f.a.check_recovery_trial_control(&t, &control, expected)
            .unwrap();
        let wrong = if n == 2 {
            RecoveryAction::CancelWifi
        } else {
            RecoveryAction::ConfirmWifi
        };
        assert!(matches!(
            f.a.check_recovery_trial_control(&t, &control, wrong),
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            f.a.check_recovery_trial_control(&t, &control, RecoveryAction::BeginWifi),
            Err(Error::Forbidden)
        ));
    }
}
#[test]
fn confirm_cannot_be_reinterpreted_as_a_new_begin() {
    let mut f = fixture(true);
    let c = script(&mut f);
    let t = begin(&mut f, &c);
    let confirm = claim(&mut f, &c, 2, Some(t.trial_id().into()));
    assert!(matches!(
        f.a.begin_recovery_trial(confirm),
        Err(Error::InvalidInput)
    ));
}
#[test]
fn begin_claim_cannot_be_reinterpreted_as_confirmation() {
    let mut first = fixture(true);
    let c = script(&mut first);
    let t = begin(&mut first, &c);
    let mut other = fixture(true);
    let c2 = script(&mut other);
    let begin = claim(&mut other, &c2, 1, None);
    assert!(matches!(
        first.a.bind_recovery_trial_control(&t, begin),
        Err(Error::InvalidInput)
    ));
}
#[test]
fn foreign_auth_and_grant_cannot_control_an_original_trial() {
    let mut f = fixture(true);
    let c = script(&mut f);
    let t = begin(&mut f, &c);
    let mut other = fixture(true);
    let c2 = script(&mut other);
    let other_trial = begin(&mut other, &c2);
    let confirm = claim(&mut other, &c2, 2, Some(other_trial.trial_id().into()));
    assert!(matches!(
        f.a.bind_recovery_trial_control(&t, confirm),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        other.a.check_recovery_trial(&t),
        Err(Error::Forbidden)
    ));
}
#[test]
fn new_connection_does_not_take_over_original_trial_or_extend_deadline() {
    let mut f = fixture(true);
    let c = script(&mut f);
    let t = begin(&mut f, &c);
    let deadline = t.deadline();
    drop(c);
    let c = script(&mut f);
    let confirm = claim(&mut f, &c, 2, Some(t.trial_id().into()));
    assert!(matches!(
        f.a.bind_recovery_trial_control(&t, confirm),
        Err(Error::Forbidden)
    ));
    assert_eq!(t.deadline(), deadline);
}
#[test]
fn disconnect_verifier_replacement_and_revocation_invalidate_background_and_save_gates() {
    for variant in 0..3 {
        let mut f = fixture(true);
        let c = script(&mut f);
        let t = begin(&mut f, &c);
        let confirm = claim(&mut f, &c, 2, Some(t.trial_id().into()));
        let control = f.a.bind_recovery_trial_control(&t, confirm).unwrap();
        match variant {
            0 => c.invalidate(),
            1 => f.a.set_recovery_verifier(None).unwrap(),
            _ => {
                f.a.revoke_recovery_key(
                    f.token.expose(),
                    f.grant.independent_public_key,
                    f.grant.generation,
                    id(90),
                )
                .unwrap();
            }
        }
        assert!(f.a.check_recovery_trial(&t).is_err());
        assert!(f
            .a
            .check_recovery_trial_control(&t, &control, RecoveryAction::ConfirmWifi)
            .is_err());
    }
}
#[test]
fn monotonic_trial_expiry_rejects_a_still_live_confirmation_without_sleep() {
    let mut f = fixture(true);
    let c = script(&mut f);
    let mut t = begin(&mut f, &c);
    let confirm = claim(&mut f, &c, 2, Some(t.trial_id().into()));
    let control = f.a.bind_recovery_trial_control(&t, confirm).unwrap();
    t.expire_for_test();
    assert!(c.is_live());
    assert!(matches!(f.a.check_recovery_trial(&t), Err(Error::Expired)));
    assert!(matches!(
        f.a.check_recovery_trial_control(&t, &control, RecoveryAction::ConfirmWifi),
        Err(Error::Expired)
    ));
}
#[test]
fn expired_begin_claim_cannot_mint_a_fresh_trial_window() {
    let mut f = fixture(true);
    let c = script(&mut f);
    let mut b = claim(&mut f, &c, 1, None);
    b.expire_for_test();
    assert!(matches!(f.a.begin_recovery_trial(b), Err(Error::Expired)));
}

#[test]
fn longer_session_cannot_raise_the_original_forty_second_ceiling() {
    let mut f = fixture(true);
    let c = script_ttl(&mut f, Duration::from_secs(60));
    let b = claim(&mut f, &c, 1, None);
    let cap_deadline = b.request().capability().execution_deadline();
    let before = std::time::Instant::now();
    let t = f.a.begin_recovery_trial(b).unwrap();
    let after = std::time::Instant::now();
    assert!(t.deadline() >= before + Duration::from_secs(40));
    assert!(t.deadline() <= after + Duration::from_secs(40));
    assert!(t.deadline() < cap_deadline);
    assert!(t.remaining() <= Duration::from_secs(40));
}
