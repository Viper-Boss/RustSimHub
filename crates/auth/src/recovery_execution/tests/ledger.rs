// SPDX-License-Identifier: AGPL-3.0-only
//! Real Auth + Store9. Only the external A verifier is a cfg(test) fixture.
use super::*;
use crate::{RecoveryExecutionAdmission, RecoveryExecutionPending};
use rustsimhub_storage::{
    RecoveryExecutionAction, RecoveryExecutionReservation, RecoveryExecutionState,
    TrustedRecoveryExecution,
};
fn prepare(f: &mut Fixture, c: &RecoveryConnection, n: u8) -> RecoveryExecutionRequest {
    let req = authorize(f, c, n);
    f.a.prepare_recovery_execution(req, None).unwrap()
}
fn admit(f: &Fixture, p: RecoveryExecutionRequest) -> Box<RecoveryExecutionPending> {
    match f.a.reserve_recovery_execution(p).unwrap() {
        RecoveryExecutionAdmission::Reserved(p) => p,
        _ => panic!("expected new private pending ticket"),
    }
}
fn query(
    f: &mut Fixture,
    n: u8,
    id: String,
) -> Option<rustsimhub_storage::RecoveryExecutionReceipt> {
    let body = format!("{{\"request_id\":\"{id}\"}}");
    let c = install(
        f,
        RecoveryAction::QueryOperation,
        body.as_bytes(),
        Duration::from_secs(30),
    );
    let q = authorize(f, &c, n);
    f.a.lookup_recovery_execution(&q, id).unwrap()
}
#[test]
fn actual_auth_reserves_then_mints_one_private_claim_without_effect_or_principal() {
    let mut f = fixture(true);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let p = prepare(&mut f, &c, 1);
    let pending = admit(&f, p);
    let claim =
        f.a.claim_recovery_execution(pending, Some("a".repeat(64)))
            .unwrap();
    assert_eq!(claim.receipt().state(), RecoveryExecutionState::Claimed);
    assert_eq!(claim.request().body(), BODY);
    f.a.check_recovery_execution_claim(&claim).unwrap();
    assert_eq!(claim.receipt().settings_saved(), None);
    assert_eq!(claim.receipt().rollback_verified(), None);
}
#[test]
fn reconnect_can_observe_existing_claim_but_never_claim_twice_or_change_body() {
    let mut f = fixture(true);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let p = prepare(&mut f, &c, 1);
    let fp = *p.fingerprint();
    let pending = admit(&f, p);
    drop(
        f.a.claim_recovery_execution(pending, Some("a".repeat(64)))
            .unwrap(),
    );
    drop(c);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let p = prepare(&mut f, &c, 1);
    assert_eq!(p.fingerprint(), &fp);
    let existing = f.a.reserve_recovery_execution(p).unwrap();
    assert!(matches!(existing, RecoveryExecutionAdmission::Existing(_)));
    assert_eq!(existing.receipt().trial_id(), Some("a".repeat(64).as_str()));
    drop(c);
    // Existing contains a readonly receipt and cannot be passed to claim.
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        b"DIFFERENT_SYNTHETIC_BODY",
        Duration::from_secs(30),
    );
    let p = prepare(&mut f, &c, 1);
    assert_ne!(p.fingerprint(), &fp);
    assert!(matches!(
        f.a.reserve_recovery_execution(p),
        Err(Error::Storage)
    ));
    assert_eq!(
        query(&mut f, 2, uuid::Uuid::from_bytes([1; 16]).to_string())
            .unwrap()
            .trial_id(),
        Some("a".repeat(64).as_str())
    );
}
#[test]
fn lookup_requires_actual_query_capability_is_readonly_and_does_not_create_missing() {
    let mut f = fixture(true);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let req = authorize(&mut f, &c, 1);
    assert!(matches!(
        f.a.lookup_recovery_execution(&req, id(99)),
        Err(Error::InvalidInput)
    ));
    let p = f.a.prepare_recovery_execution(req, None).unwrap();
    let request = p.request_id().to_owned();
    drop(admit(&f, p));
    drop(c);
    let body = format!("{{\"request_id\":\"{request}\"}}");
    let c = install(
        &mut f,
        RecoveryAction::QueryOperation,
        body.as_bytes(),
        Duration::from_secs(30),
    );
    let q = authorize(&mut f, &c, 2);
    let b1 = dir();
    let b2 = dir();
    f.s.handle().backup(b1.path()).unwrap();
    assert_eq!(
        f.a.lookup_recovery_execution(&q, request)
            .unwrap()
            .unwrap()
            .state(),
        RecoveryExecutionState::Reserved
    );
    assert!(f.a.lookup_recovery_execution(&q, id(99)).unwrap().is_none());
    assert!(matches!(
        f.a.lookup_recovery_execution(&q, "invalid UUID".into()),
        Err(Error::Storage)
    ));
    f.s.handle().backup(b2.path()).unwrap();
    assert_eq!(
        fs::read(b1.path().join("backup.sqlite3")).unwrap(),
        fs::read(b2.path().join("backup.sqlite3")).unwrap()
    );
    // Core still must validate requested-id semantics against the same body;
    // these calls exercise the Query capability/readonly Storage contract only.
}
#[test]
fn changed_authenticated_body_cannot_reserve_a_prepared_request() {
    let mut f = fixture(true);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let mut p = prepare(&mut f, &c, 1);
    let request = p.request_id().into();
    p.authorized.body.push(b' ');
    assert!(matches!(
        f.a.reserve_recovery_execution(p),
        Err(Error::Forbidden)
    ));
    assert!(query(&mut f, 2, request).is_none());
}
#[test]
fn disconnect_before_claim_preserves_reservation_without_returning_permit() {
    let mut f = fixture(true);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let p = prepare(&mut f, &c, 1);
    let id = p.request_id().into();
    let pending = admit(&f, p);
    drop(c);
    assert!(matches!(
        f.a.claim_recovery_execution(pending, Some("a".repeat(64))),
        Err(Error::Forbidden)
    ));
    assert_eq!(
        query(&mut f, 2, id).unwrap().state(),
        RecoveryExecutionState::Reserved
    );
}
#[test]
fn current_revoke_prevents_auth_claim_and_query_even_with_reserved_row() {
    let mut f = fixture(true);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let p = prepare(&mut f, &c, 1);
    let pending = admit(&f, p);
    f.a.revoke_recovery_key(
        f.token.expose(),
        f.grant.independent_public_key,
        f.grant.generation,
        id(90),
    )
    .unwrap();
    assert!(matches!(
        f.a.claim_recovery_execution(pending, Some("a".repeat(64))),
        Err(Error::Storage)
    ));
    assert!(matches!(
        f.a.prepare_recovery_verification(&c, f.grant.clone(), &[2; 16]),
        Err(Error::Storage)
    ));
}
#[test]
fn committed_claim_still_requires_live_unexpired_same_auth_capability() {
    for variant in 0..3 {
        let mut f = fixture(true);
        let c = install(
            &mut f,
            RecoveryAction::BeginWifi,
            BODY,
            Duration::from_secs(30),
        );
        let p = prepare(&mut f, &c, 1);
        let pending = admit(&f, p);
        let mut claim =
            f.a.claim_recovery_execution(pending, Some("a".repeat(64)))
                .unwrap();
        f.a.check_recovery_execution_claim(&claim).unwrap();
        match variant {
            0 => c.invalidate(),
            1 => f.a.set_recovery_verifier(None).unwrap(),
            _ => claim.expire_for_test(),
        };
        assert!(matches!(
            f.a.check_recovery_execution_claim(&claim),
            Err(Error::Forbidden | Error::Expired)
        ));
    }
}
#[test]
fn dropping_auth_invalidates_its_real_writer_fence_even_if_transport_token_survives() {
    let mut f = fixture(true);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let p = prepare(&mut f, &c, 1);
    let input = TrustedRecoveryExecution::from_trusted_auth(
        p.grant().clone(),
        p.request_id().into(),
        RecoveryExecutionAction::BeginWifi,
        *p.fingerprint(),
        None,
        f.a.recovery_execution_fence(p.capability()).unwrap(),
    )
    .unwrap();
    let pending = match f
        .s
        .handle()
        .reserve_recovery_execution_from_trusted_auth(input)
        .unwrap()
    {
        RecoveryExecutionReservation::Reserved(p) => p,
        _ => panic!("expected newly inserted"),
    };
    assert!(c.is_live());
    drop(f.a);
    assert!(!c.is_live());
    assert!(matches!(
        f.s.handle()
            .claim_recovery_execution_from_trusted_auth(pending, Some("a".repeat(64))),
        Err(rustsimhub_storage::Error::Forbidden)
    ));
}
#[test]
fn real_auth_and_store_reopen_report_unknown_and_refuse_to_redispatch() {
    let mut f = fixture(true);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let p = prepare(&mut f, &c, 1);
    let fp = *p.fingerprint();
    let request = p.request_id().to_owned();
    let pending = admit(&f, p);
    drop(
        f.a.claim_recovery_execution(pending, Some("a".repeat(64)))
            .unwrap(),
    );
    drop(c);
    drop(f.a);
    drop(f.s);
    let s = Store::open_recovery_execution(f._d.path()).unwrap();
    let a = Auth::open(f.i.path(), s.handle()).unwrap();
    f.a = a;
    f.s = s;
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let p = prepare(&mut f, &c, 1);
    assert_eq!(p.fingerprint(), &fp);
    let existing = f.a.reserve_recovery_execution(p).unwrap();
    assert!(matches!(existing, RecoveryExecutionAdmission::Existing(_)));
    assert_eq!(existing.receipt().state(), RecoveryExecutionState::Unknown);
    let p = prepare(&mut f, &c, 2);
    assert!(matches!(
        f.a.reserve_recovery_execution(p),
        Err(Error::Storage)
    ));
    assert_eq!(
        query(&mut f, 3, request).unwrap().state(),
        RecoveryExecutionState::Unknown
    );
}
#[test]
fn reconnect_existing_reserved_request_must_remain_readonly() {
    let mut f = fixture(true);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let p = prepare(&mut f, &c, 1);
    let pending = admit(&f, p);
    drop(c);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let p = prepare(&mut f, &c, 1);
    let existing = f.a.reserve_recovery_execution(p).unwrap();
    assert!(matches!(existing, RecoveryExecutionAdmission::Existing(_)));
    assert_eq!(existing.receipt().state(), RecoveryExecutionState::Reserved);
    // The new connection gets no Pending; the retained original Pending is
    // still fenced by its dead connection. No transparent claim takeover.
    assert!(matches!(
        f.a.claim_recovery_execution(pending, Some("a".repeat(64))),
        Err(Error::Forbidden)
    ));
}
