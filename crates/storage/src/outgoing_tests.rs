// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use rustsimhub_policy::{mask, Kind, Scope};
fn fixture() -> (
    tempfile::TempDir,
    Store,
    rustsimhub_policy::Principal,
    SendContext,
) {
    let d = super::tests::directory();
    let s = Store::open(d.path()).unwrap();
    let h = s.handle();
    let p = h.bootstrap_owner([41; 32]).unwrap();
    let line = h
        .observe_line(LineObservation {
            backend: "synthetic-backend".into(),
            fingerprint: Some([3; 32]),
            status: LineStatus::Ready,
        })
        .unwrap()
        .line;
    let b = SendContext {
        line,
        backend: "synthetic-backend".into(),
        owner: ":1.20".into(),
        modem: "/org/freedesktop/ModemManager1/Modem/0".into(),
        fingerprint: [3; 32],
    };
    (d, s, p, b)
}
fn draft(p: &rustsimhub_policy::Principal, b: &SendContext, key: &str) -> SmsRequest {
    SmsRequest {
        client_id: p.id.clone(),
        line: b.line,
        idempotency_key: key.into(),
        recipient: "+12025550123".into(),
        text: "synthetic outgoing".into(),
    }
}
#[test]
fn send_permit_is_single_use_scoped_and_revocation_fences_before_send() {
    let (_d, s, p, b) = fixture();
    let h = s.handle();
    let reader = h
        .approve_client(
            p.clone(),
            [42; 32],
            Kind::Client,
            mask(&[Scope::SmsRead]).unwrap(),
        )
        .unwrap();
    assert!(matches!(
        h.prepare_authorized(draft(&reader, &b, "a"), reader, b.clone(), || true),
        Err(Error::Forbidden)
    ));
    let op = h
        .prepare_authorized(draft(&p, &b, "a"), p.clone(), b.clone(), || true)
        .unwrap();
    assert_eq!(
        h.prepare_authorized(draft(&p, &b, "a"), p.clone(), b.clone(), || false)
            .unwrap(),
        op
    );
    let mut changed = draft(&p, &b, "a");
    changed.text = "different".into();
    assert!(matches!(
        h.prepare_authorized(changed, p.clone(), b.clone(), || true),
        Err(Error::Conflict)
    ));
    assert!(matches!(
        h.begin_authorized(op.id.clone(), p.clone(), b.clone(), || false),
        Err(Error::StaleLine)
    ));
    assert_eq!(
        h.operation(&op.id, &p.id).unwrap().state,
        SmsState::Prepared
    );
    h.begin_authorized(op.id.clone(), p.clone(), b.clone(), || true)
        .unwrap();
    assert!(h
        .permit_send(op.id.clone(), p.clone(), b.clone(), || true)
        .is_err());
    let path = "/org/freedesktop/ModemManager1/SMS/001".to_owned();
    h.bind_created(op.id.clone(), p.clone(), b.clone(), path, 0, || true)
        .unwrap();
    assert!(h
        .permit_send(op.id.clone(), p.clone(), b.clone(), || false)
        .is_err());
    h.permit_send(op.id.clone(), p.clone(), b.clone(), || true)
        .unwrap();
    assert!(h
        .permit_send(op.id.clone(), p.clone(), b.clone(), || true)
        .is_err());
    assert!(h.cancel_unsent(op.id.clone(), p.id.clone()).is_err());
    h.mark_unknown(&op.id, &p.id).unwrap();
    assert_eq!(
        h.resolve_sent(op.id, p, b, || true).unwrap().state,
        SmsState::Submitted
    );
}
#[test]
fn revoke_and_reapprove_cannot_reuse_an_old_sending_generation() {
    let (_d, s, owner, b) = fixture();
    let h = s.handle();
    let p = h
        .approve_client(
            owner.clone(),
            [43; 32],
            Kind::Client,
            mask(&[Scope::SmsSend]).unwrap(),
        )
        .unwrap();
    let op = h
        .prepare_authorized(draft(&p, &b, "a"), p.clone(), b.clone(), || true)
        .unwrap();
    h.begin_authorized(op.id.clone(), p.clone(), b.clone(), || true)
        .unwrap();
    h.bind_created(
        op.id.clone(),
        p.clone(),
        b.clone(),
        "/org/freedesktop/ModemManager1/SMS/001".into(),
        0,
        || true,
    )
    .unwrap();
    h.revoke_client(owner.clone(), p.public_key).unwrap();
    assert!(matches!(
        h.permit_send(op.id.clone(), p.clone(), b.clone(), || true),
        Err(Error::Forbidden)
    ));
    let current = h
        .approve_client_if(
            owner,
            p.public_key,
            Kind::Client,
            p.scopes,
            p.generation + 1,
        )
        .unwrap();
    assert!(current.generation > p.generation);
    assert!(h.permit_send(op.id.clone(), current, b, || true).is_err());
    h.cancel_unsent(op.id, p.id).unwrap();
}
#[test]
fn crash_during_create_and_send_keeps_unknown_and_never_returns_a_new_permit() {
    for phase in ["creating", "sending"] {
        let (d, s, p, b) = fixture();
        let h = s.handle();
        let op = h
            .prepare_authorized(draft(&p, &b, "a"), p.clone(), b.clone(), || true)
            .unwrap();
        h.begin_authorized(op.id.clone(), p.clone(), b.clone(), || true)
            .unwrap();
        if phase == "sending" {
            h.bind_created(
                op.id.clone(),
                p.clone(),
                b.clone(),
                "/org/freedesktop/ModemManager1/SMS/001".into(),
                0,
                || true,
            )
            .unwrap();
            h.permit_send(op.id.clone(), p.clone(), b.clone(), || true)
                .unwrap();
        }
        drop(s);
        let s = Store::open(d.path()).unwrap();
        let h = s.handle();
        assert_eq!(
            h.sender_operation(op.id.clone(), p.clone()).unwrap().state,
            SmsState::Unknown
        );
        assert!(h.begin_authorized(op.id, p, b, || true).is_err());
    }
}
#[test]
fn transaction_failure_cannot_persist_an_attempt_or_consume_a_draft() {
    let (_d, s, p, b) = fixture();
    let h = s.handle();
    let op = h
        .prepare_authorized(draft(&p, &b, "a"), p.clone(), b.clone(), || true)
        .unwrap();
    h.call(|db|{db.connection.execute_batch("CREATE TEMP TRIGGER fail_send AFTER INSERT ON main.send_attempts BEGIN SELECT RAISE(ABORT,'synthetic'); END;")?;Ok(())}).unwrap();
    assert!(h
        .begin_authorized(op.id.clone(), p.clone(), b.clone(), || true)
        .is_err());
    assert_eq!(
        h.operation(&op.id, &p.id).unwrap().state,
        SmsState::Prepared
    );
    assert!(h.send_attempt(op.id.clone(), p.clone()).is_err());
    h.call(|db| {
        db.connection.execute_batch("DROP TRIGGER fail_send;")?;
        Ok(())
    })
    .unwrap();
    h.begin_authorized(op.id.clone(), p.clone(), b.clone(), || true)
        .unwrap();
    let mut swapped = b;
    swapped.fingerprint = [9; 32];
    assert!(h
        .bind_created(
            op.id.clone(),
            p.clone(),
            swapped,
            "/org/freedesktop/ModemManager1/SMS/001".into(),
            0,
            || true
        )
        .is_err());
    h.cancel_unsent(op.id, p.id).unwrap();
}
