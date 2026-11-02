// SPDX-License-Identifier: AGPL-3.0-only
use super::*;

fn ready() -> (Fixture, RecoveryConnection, AuthorizedRecoveryRequest) {
    let mut f = fixture(true);
    let c = install(
        &mut f,
        RecoveryAction::QueryOperation,
        b"{}",
        Duration::from_secs(120),
    );
    let r = authorize(&mut f, &c, 2);
    (f, c, r)
}
#[test]
fn response_scope_retains_exact_private_binding_action_and_request() {
    let (f, _c, r) = ready();
    let s = f.a.prepare_recovery_response(&r).unwrap();
    assert_eq!(s.request_id(), &[2; 16]);
    assert_eq!(s.action(), RecoveryAction::QueryOperation);
    assert_eq!(s.binding().grant(), &f.grant);
    assert_eq!(s.binding().device_key(), &f.a.public_key());
    assert_eq!(s.binding().transport_nonce(), &[11; 32]);
    assert!(s.session_id().len() >= 16);
    s.check_current().unwrap();
}
#[test]
fn response_scope_never_survives_disconnect_verifier_replacement_or_expiry() {
    for mode in 0..3 {
        let (mut f, c, r) = ready();
        let mut s = f.a.prepare_recovery_response(&r).unwrap();
        match mode {
            0 => c.invalidate(),
            1 => f.a.set_recovery_verifier(None).unwrap(),
            _ => s.expire_for_test(),
        }
        assert!(matches!(
            s.check_current(),
            Err(Error::Forbidden | Error::Expired)
        ));
    }
}
#[test]
fn response_scope_rejects_revocation_and_original_store_closure() {
    let (mut f, _c, r) = ready();
    let s = f.a.prepare_recovery_response(&r).unwrap();
    f.a.revoke_recovery_key(
        f.token.expose(),
        f.grant.independent_public_key,
        f.grant.generation,
        id(9),
    )
    .unwrap();
    assert!(matches!(s.check_current(), Err(Error::Storage)));
    let (f, _c, r) = ready();
    let s = f.a.prepare_recovery_response(&r).unwrap();
    drop(f.s);
    assert!(matches!(s.check_current(), Err(Error::Storage)));
}
#[test]
fn response_scope_cannot_be_minted_from_modified_body_or_other_auth() {
    let (f, _c, mut r) = ready();
    let other = fixture(true);
    assert!(matches!(
        other.a.prepare_recovery_response(&r),
        Err(Error::Forbidden)
    ));
    r.body.push(b' ');
    assert!(matches!(
        f.a.prepare_recovery_response(&r),
        Err(Error::Forbidden)
    ));
}
#[test]
fn response_disconnect_guard_invalidates_without_an_auth_lock_and_disarm_keeps_channel() {
    let (f, c, r) = ready();
    let s = f.a.prepare_recovery_response(&r).unwrap();
    c.disconnect_guard().disarm();
    s.check_current().unwrap();
    let guard = c.disconnect_guard();
    let locked = std::sync::Mutex::new(f.a);
    let _held = locked.lock().unwrap();
    drop(guard);
    assert!(matches!(s.check_current(), Err(Error::Forbidden)));
}
