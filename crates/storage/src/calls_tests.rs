// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use rustsimhub_domain::CallAction;
use rustsimhub_policy::{mask, Kind, Scope};
#[test]
fn public_operation_shape_matches_browser_reconciliation_fixture() {
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/fixtures/call-operation-unknown.json"
    ))
    .unwrap();
    let operation = CallOperation {
        id: "00000000-0000-4000-8000-000000000001".into(),
        call_id: Some("00000000-0000-4000-8000-000000000002".into()),
        line: LineEpoch(2),
        state: SmsState::Unknown,
    };
    assert_eq!(serde_json::to_value(operation).unwrap(), expected);
}
fn fixture() -> (
    tempfile::TempDir,
    Store,
    rustsimhub_policy::Principal,
    SendContext,
) {
    let d = super::tests::directory();
    let s = Store::open(d.path()).unwrap();
    let h = s.handle();
    let p = h.bootstrap_owner([61; 32]).unwrap();
    let line = h
        .observe_line(LineObservation {
            backend: "synthetic-calls".into(),
            fingerprint: Some([4; 32]),
            status: LineStatus::Ready,
        })
        .unwrap()
        .line;
    let b = SendContext {
        line,
        backend: "synthetic-calls".into(),
        owner: ":1.20".into(),
        modem: "/org/freedesktop/ModemManager1/Modem/0".into(),
        fingerprint: [4; 32],
    };
    (d, s, p, b)
}
fn observation(source: &str, state: i32) -> CallObservation {
    CallObservation {
        source: source.into(),
        number: "+12025550123".into(),
        direction: 2,
        state,
        reason: 0,
    }
}
fn request(b: &SendContext) -> CallRequest {
    CallRequest {
        idempotency_key: "dial-a".into(),
        line: b.line,
        action: CallAction::Dial,
        call_id: None,
        payload: "+12025550123".into(),
    }
}
#[test]
fn cancellation_is_atomic_before_permit_owned_and_persistent() {
    let (d, s, p, b) = fixture();
    let h = s.handle();
    let op = h
        .prepare_call(request(&b), p.clone(), b.clone(), || true)
        .unwrap();
    let foreign = h
        .approve_client(
            p.clone(),
            [71; 32],
            Kind::Client,
            mask(&[Scope::CallsControl]).unwrap(),
        )
        .unwrap();
    assert!(h.cancel_prepared_call(op.id.clone(), foreign).is_err());
    let mut stale = p.clone();
    stale.generation += 1;
    assert!(h.cancel_prepared_call(op.id.clone(), stale).is_err());
    assert_eq!(
        h.cancel_prepared_call(op.id.clone(), p.clone())
            .unwrap()
            .state,
        SmsState::Cancelled
    );
    assert_eq!(
        h.cancel_prepared_call(op.id.clone(), p.clone())
            .unwrap()
            .state,
        SmsState::Cancelled
    );
    assert!(h
        .begin_call(op.id.clone(), p.clone(), b.clone(), || true)
        .is_err());
    assert_eq!(
        h.prepare_call(request(&b), p.clone(), b.clone(), || true)
            .unwrap()
            .state,
        SmsState::Cancelled
    );
    let mut next = request(&b);
    next.idempotency_key = "next".into();
    let busy = h.prepare_call(next, p.clone(), b.clone(), || true).unwrap();
    h.begin_call(busy.id.clone(), p.clone(), b, || true)
        .unwrap();
    assert!(h.cancel_prepared_call(busy.id.clone(), p.clone()).is_err());
    h.finish_call(busy.id.clone(), p.id.clone(), SmsState::Unknown)
        .unwrap();
    assert!(h.cancel_prepared_call(busy.id, p.clone()).is_err());
    drop(s);
    let s = Store::open(d.path()).unwrap();
    assert_eq!(
        s.handle().call_operation(op.id, p).unwrap().state,
        SmsState::Cancelled
    );
}
#[test]
fn calls_preserve_history_without_inventing_ended_after_disconnect_or_restart() {
    let (d, s, _p, b) = fixture();
    let h = s.handle();
    let c = h
        .observe_call(b.clone(), observation("call/1#1", 4), || true)
        .unwrap();
    assert_eq!(
        h.call_detail(c.id.clone()).unwrap().state,
        rustsimhub_domain::CallState::Active
    );
    assert!(h
        .observe_call(b.clone(), observation("stale", 7), || false)
        .is_err());
    h.calls_unavailable(vec![c.id.clone()]).unwrap();
    let old = h.call_detail(c.id.clone()).unwrap();
    assert_eq!(old.modem_state, 4);
    assert_eq!(old.state, rustsimhub_domain::CallState::Reconciling);
    let reused = h
        .observe_call(b.clone(), observation("call/1#2", 3), || true)
        .unwrap();
    assert_ne!(c.id, reused.id);
    assert_eq!(h.calls(None, 32).unwrap().calls.len(), 2);
    drop(s);
    let s = Store::open(d.path()).unwrap();
    let h = s.handle();
    assert_eq!(
        h.call_detail(reused.id).unwrap().state,
        rustsimhub_domain::CallState::Reconciling
    );
    assert_eq!(h.call_events(None, 32).unwrap().events.len(), 4);
}
#[test]
fn call_actions_require_scope_current_generation_and_a_committed_unique_permit() {
    let (_d, s, p, b) = fixture();
    let h = s.handle();
    let read = h
        .approve_client(
            p.clone(),
            [62; 32],
            Kind::Client,
            mask(&[Scope::CallsRead]).unwrap(),
        )
        .unwrap();
    assert!(matches!(
        h.prepare_call(request(&b), read, b.clone(), || true),
        Err(Error::Forbidden)
    ));
    let op = h
        .prepare_call(request(&b), p.clone(), b.clone(), || true)
        .unwrap();
    assert_eq!(
        h.prepare_call(request(&b), p.clone(), b.clone(), || false)
            .unwrap()
            .id,
        op.id
    );
    let mut changed = request(&b);
    changed.payload = "+12025550124".into();
    assert!(matches!(
        h.prepare_call(changed, p.clone(), b.clone(), || true),
        Err(Error::Conflict)
    ));
    assert!(h
        .begin_call(op.id.clone(), p.clone(), b.clone(), || false)
        .is_err());
    h.begin_call(op.id.clone(), p.clone(), b.clone(), || true)
        .unwrap();
    let c = h
        .observe_call(b.clone(), observation("call/1#1", 0), || true)
        .unwrap();
    let path = "/org/freedesktop/ModemManager1/Call/1".to_owned();
    h.permit_call(
        op.id.clone(),
        p.clone(),
        b.clone(),
        c.id.clone(),
        (path.clone(), 1),
        || true,
    )
    .unwrap();
    assert!(h
        .permit_call(op.id.clone(), p.clone(), b.clone(), c.id, (path, 1), || {
            true
        })
        .is_err());
    assert!(h
        .finish_call(op.id.clone(), p.id.clone(), SmsState::Cancelled)
        .is_err());
    h.finish_call(op.id.clone(), p.id.clone(), SmsState::Unknown)
        .unwrap();
    assert!(h
        .confirm_call(op.id.clone(), p.clone(), b.clone(), || false)
        .is_err());
    h.confirm_call(op.id, p, b, || true).unwrap();
}
#[test]
fn call_transaction_failure_and_revocation_never_yield_control_permission() {
    let (_d, s, owner, b) = fixture();
    let h = s.handle();
    let p = h
        .approve_client(
            owner.clone(),
            [63; 32],
            Kind::Client,
            mask(&[Scope::CallsControl]).unwrap(),
        )
        .unwrap();
    let op = h
        .prepare_call(request(&b), p.clone(), b.clone(), || true)
        .unwrap();
    h.call(|db|{db.connection.execute_batch("CREATE TEMP TRIGGER fail_call AFTER UPDATE ON main.call_operations BEGIN SELECT RAISE(ABORT,'synthetic'); END;")?;Ok(())}).unwrap();
    assert!(h
        .begin_call(op.id.clone(), p.clone(), b.clone(), || true)
        .is_err());
    assert_eq!(
        h.call_operation(op.id.clone(), p.clone()).unwrap().state,
        SmsState::Prepared
    );
    h.call(|db| {
        db.connection.execute_batch("DROP TRIGGER fail_call;")?;
        Ok(())
    })
    .unwrap();
    h.begin_call(op.id.clone(), p.clone(), b.clone(), || true)
        .unwrap();
    h.revoke_client(owner, p.public_key).unwrap();
    assert!(h.call_operation(op.id, p).is_err());
}
#[test]
fn restart_and_sim_change_quarantine_operations_and_control_records() {
    let (d, s, p, b) = fixture();
    let h = s.handle();
    let op = h
        .prepare_call(request(&b), p.clone(), b.clone(), || true)
        .unwrap();
    h.begin_call(op.id.clone(), p.clone(), b.clone(), || true)
        .unwrap();
    drop(s);
    let s = Store::open(d.path()).unwrap();
    let h = s.handle();
    assert_eq!(
        h.call_operation(op.id.clone(), p.clone()).unwrap().state,
        SmsState::Unknown
    );
    assert!(h.begin_call(op.id, p.clone(), b.clone(), || true).is_err());
    let line = h
        .observe_line(LineObservation {
            backend: "next-call-backend".into(),
            fingerprint: Some([5; 32]),
            status: LineStatus::Ready,
        })
        .unwrap()
        .line;
    let mut next = b;
    next.line = line;
    next.backend = "next-call-backend".into();
    next.fingerprint = [5; 32];
    let mut next_request = request(&next);
    next_request.idempotency_key = "dial-b".into();
    let op = h
        .prepare_call(next_request, p.clone(), next.clone(), || true)
        .unwrap();
    h.change_line(LineEpoch(line.0 + 1)).unwrap();
    assert_eq!(
        h.call_operation(op.id, p).unwrap().state,
        SmsState::Cancelled
    );
}
