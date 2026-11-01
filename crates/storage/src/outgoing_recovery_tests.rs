// SPDX-License-Identifier: AGPL-3.0-only
//! Durable authorized send boundaries. No modem or actual message delivery.
use super::*;
use rustsimhub_policy::Principal;

fn binding(h: &Handle) -> SendContext {
    let line = h
        .observe_line(LineObservation {
            backend: "synthetic-send-recovery".into(),
            fingerprint: Some([61; 32]),
            status: LineStatus::Ready,
        })
        .unwrap()
        .line;
    SendContext {
        line,
        backend: "synthetic-send-recovery".into(),
        owner: ":1.20".into(),
        modem: "/org/freedesktop/ModemManager1/Modem/0".into(),
        fingerprint: [61; 32],
    }
}
fn fixture() -> (tempfile::TempDir, Store, Principal, SendContext) {
    let d = super::tests::directory();
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    let owner = h.bootstrap_owner([61; 32]).unwrap();
    let b = binding(&h);
    (d, store, owner, b)
}
fn draft(p: &Principal, b: &SendContext, key: &str) -> SmsRequest {
    SmsRequest {
        client_id: p.id.clone(),
        line: b.line,
        idempotency_key: key.into(),
        recipient: "+12025550123".into(),
        text: "synthetic outgoing".into(),
    }
}
fn prepare(h: &Handle, p: &Principal, b: &SendContext, key: &str) -> Operation {
    h.prepare_authorized(draft(p, b, key), p.clone(), b.clone(), || true)
        .unwrap()
}
fn bind(h: &Handle, p: &Principal, b: &SendContext, id: &str) {
    h.bind_created(
        id.into(),
        p.clone(),
        b.clone(),
        "/org/freedesktop/ModemManager1/SMS/6100".into(),
        7,
        || true,
    )
    .unwrap();
}
fn fail_event(h: &Handle, kind: &'static str) {
    h.call(move|db|{db.connection.execute_batch(&format!("CREATE TEMP TRIGGER synthetic_send_failure BEFORE INSERT ON main.events WHEN NEW.kind='{kind}' BEGIN SELECT RAISE(ABORT,'synthetic write failure'); END;"))?;Ok(())}).unwrap();
}
fn clear_fault(h: &Handle) {
    h.call(|db| {
        db.connection
            .execute_batch("DROP TRIGGER synthetic_send_failure")?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn prepare_and_begin_event_failures_roll_back_operation_attempt_and_cursor_together() {
    for kind in ["prepared", "submitting"] {
        let (_d, store, p, b) = fixture();
        let h = store.handle();
        let op = if kind == "submitting" {
            Some(prepare(&h, &p, &b, "synthetic-key"))
        } else {
            None
        };
        let head = h.cursor().unwrap();
        fail_event(&h, kind);
        if let Some(op) = op.as_ref() {
            assert!(h
                .begin_authorized(op.id.clone(), p.clone(), b.clone(), || true)
                .is_err());
            assert_eq!(
                h.sender_operation(op.id.clone(), p.clone()).unwrap().state,
                SmsState::Prepared
            );
            assert!(h.send_attempt(op.id.clone(), p.clone()).is_err());
        } else {
            assert!(h
                .prepare_authorized(draft(&p, &b, "synthetic-key"), p.clone(), b.clone(), || {
                    true
                })
                .is_err());
            assert_eq!(
                h.call(|db| Ok(db.connection.query_row(
                    "SELECT count(*) FROM sms_operations",
                    [],
                    |r| r.get::<_, i64>(0)
                )?))
                .unwrap(),
                0
            );
        }
        assert_eq!(h.cursor().unwrap(), head);
        clear_fault(&h);
        let retry = prepare(&h, &p, &b, "synthetic-key");
        if let Some(op) = op {
            assert_eq!(retry.id, op.id);
        }
        h.begin_authorized(retry.id.clone(), p.clone(), b.clone(), || true)
            .unwrap();
        assert!(h
            .begin_authorized(retry.id, p.clone(), b.clone(), || true)
            .is_err());
    }
}

#[test]
fn created_binding_and_send_permit_write_failures_never_return_a_send_permission() {
    for phase in ["created", "sending"] {
        let (_d, store, p, b) = fixture();
        let h = store.handle();
        let op = prepare(&h, &p, &b, "synthetic-key");
        h.begin_authorized(op.id.clone(), p.clone(), b.clone(), || true)
            .unwrap();
        if phase == "sending" {
            bind(&h, &p, &b, &op.id);
        }
        h.call(move|db|{db.connection.execute_batch(&format!("CREATE TEMP TRIGGER synthetic_send_failure BEFORE UPDATE ON main.send_attempts WHEN NEW.phase='{phase}' BEGIN SELECT RAISE(ABORT,'synthetic phase write failure'); END;"))?;Ok(())}).unwrap();
        let mut sends = 0;
        let result = if phase == "created" {
            h.bind_created(
                op.id.clone(),
                p.clone(),
                b.clone(),
                "/org/freedesktop/ModemManager1/SMS/6100".into(),
                7,
                || true,
            )
        } else {
            let result = h.permit_send(op.id.clone(), p.clone(), b.clone(), || true);
            if result.is_ok() {
                sends += 1;
            }
            result
        };
        assert!(result.is_err());
        assert_eq!(sends, 0);
        let attempt = h.send_attempt(op.id.clone(), p.clone()).unwrap();
        assert_eq!(
            attempt.phase,
            if phase == "created" {
                "creating"
            } else {
                "created"
            }
        );
        assert_eq!(attempt.sms_path.is_some(), phase == "sending");
        assert_eq!(attempt.operation.state, SmsState::Submitting);
        clear_fault(&h);
        h.cancel_unsent(op.id.clone(), p.id.clone()).unwrap();
        assert!(h.permit_send(op.id, p, b, || true).is_err());
    }
}

#[test]
fn failed_confirmation_recovers_unknown_then_resolves_without_a_second_send_permission() {
    let (d, store, p, b) = fixture();
    let h = store.handle();
    let op = prepare(&h, &p, &b, "synthetic-key");
    h.begin_authorized(op.id.clone(), p.clone(), b.clone(), || true)
        .unwrap();
    bind(&h, &p, &b, &op.id);
    h.permit_send(op.id.clone(), p.clone(), b.clone(), || true)
        .unwrap();
    let sends = 1; // mock side effect only after the real durable permit
    let head = h.cursor().unwrap();
    fail_event(&h, "submitted");
    assert!(h
        .resolve_sent(op.id.clone(), p.clone(), b.clone(), || true)
        .is_err());
    assert_eq!(h.cursor().unwrap(), head);
    assert_eq!(
        h.send_attempt(op.id.clone(), p.clone()).unwrap().phase,
        "sending"
    );
    assert_eq!(
        h.sender_operation(op.id.clone(), p.clone()).unwrap().state,
        SmsState::Submitting
    );
    drop(store);
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    let b = binding(&h);
    assert_eq!(
        h.sender_operation(op.id.clone(), p.clone()).unwrap().state,
        SmsState::Unknown
    );
    let same = h
        .prepare_authorized(draft(&p, &b, "synthetic-key"), p.clone(), b.clone(), || {
            false
        })
        .unwrap();
    assert_eq!(same.id, op.id);
    assert_eq!(same.state, SmsState::Unknown);
    assert!(h
        .begin_authorized(op.id.clone(), p.clone(), b.clone(), || true)
        .is_err());
    assert!(h
        .permit_send(op.id.clone(), p.clone(), b.clone(), || true)
        .is_err());
    // Trusted positive proof is modeled here; real peer evidence is tested in Core.
    assert_eq!(
        h.resolve_sent(op.id.clone(), p.clone(), b.clone(), || true)
            .unwrap()
            .state,
        SmsState::Submitted
    );
    assert!(h
        .resolve_sent(op.id.clone(), p.clone(), b, || true)
        .is_err());
    let mut cursor = h.cursor().unwrap();
    cursor.seq = "0".into();
    let events = h.events(cursor, 32).unwrap().events;
    assert_eq!(
        events
            .iter()
            .filter(|e| e.operation_id.as_ref() == Some(&op.id) && e.kind == "submitted")
            .count(),
        1
    );
    assert_eq!(sends, 1);
}

#[test]
fn authorized_crash_child() {
    let Ok(path) = std::env::var("RSH_SYNTHETIC_SEND_CRASH_DIR") else {
        return;
    };
    let phase = std::env::var("RSH_SYNTHETIC_SEND_CRASH_PHASE").unwrap();
    let store = Store::open(&path).unwrap();
    let h = store.handle();
    let p = h.bootstrap_owner([61; 32]).unwrap();
    let b = binding(&h);
    let op = prepare(&h, &p, &b, &phase);
    if phase == "prepared" {
        std::process::exit(91);
    }
    h.begin_authorized(op.id.clone(), p.clone(), b.clone(), || true)
        .unwrap();
    if phase == "creating" {
        std::process::exit(91);
    }
    bind(&h, &p, &b, &op.id);
    if phase == "created" {
        std::process::exit(91);
    }
    h.permit_send(op.id.clone(), p.clone(), b.clone(), || true)
        .unwrap();
    let mut count = std::fs::File::create(Path::new(&path).join("synthetic-send-count")).unwrap();
    std::io::Write::write_all(&mut count, b"1").unwrap();
    count.sync_all().unwrap();
    if phase == "sending" {
        std::process::exit(91);
    }
    if phase == "uncommitted-confirmation" {
        h.call(move |db| -> Result<()> {
            let tx = db.connection.transaction()?;
            tx.execute(
                "UPDATE sms_operations SET state='submitted' WHERE id=?",
                [&op.id],
            )?;
            tx.execute(
                "UPDATE send_attempts SET phase='resolved' WHERE operation_id=?",
                [&op.id],
            )?;
            event(&tx, Some(&op.id), checked_line(op.line)?, "submitted")?;
            std::process::exit(91);
        })
        .unwrap();
        unreachable!("synthetic child exits before the open transaction can unwind");
    }
    assert_eq!(phase, "submitted");
    h.resolve_sent(op.id, p, b, || true).unwrap();
    std::process::exit(91);
}

#[test]
fn actual_process_exit_at_six_authorized_boundaries_preserves_phase_and_never_reissues_permits() {
    for phase in [
        "prepared",
        "creating",
        "created",
        "sending",
        "uncommitted-confirmation",
        "submitted",
    ] {
        let d = super::tests::directory();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "outgoing_recovery_tests::authorized_crash_child",
                "--nocapture",
            ])
            .env("RSH_SYNTHETIC_SEND_CRASH_DIR", d.path())
            .env("RSH_SYNTHETIC_SEND_CRASH_PHASE", phase)
            .output()
            .unwrap();
        assert_eq!(
            child.status.code(),
            Some(91),
            "child phase {phase}: {}",
            String::from_utf8_lossy(&child.stderr)
        );
        let expected = match phase {
            "prepared" => SmsState::Prepared,
            "submitted" => SmsState::Submitted,
            _ => SmsState::Unknown,
        };
        let count = d.path().join("synthetic-send-count");
        assert_eq!(
            count.exists(),
            matches!(phase, "sending" | "uncommitted-confirmation" | "submitted")
        );
        let mut prior_head = None;
        for _ in 0..3 {
            let store = Store::open(d.path()).unwrap();
            let h = store.handle();
            let p = h.principal([61; 32]).unwrap().unwrap();
            let b = SendContext {
                line: h.line_epoch().unwrap(),
                backend: "synthetic-send-recovery".into(),
                owner: ":1.20".into(),
                modem: "/org/freedesktop/ModemManager1/Modem/0".into(),
                fingerprint: [61; 32],
            };
            let op = h
                .prepare_authorized(draft(&p, &b, phase), p.clone(), b.clone(), || false)
                .unwrap();
            assert_eq!(op.state, expected);
            assert!(h
                .permit_send(op.id.clone(), p.clone(), b.clone(), || true)
                .is_err());
            if expected != SmsState::Prepared {
                assert!(h
                    .begin_authorized(op.id.clone(), p.clone(), b, || true)
                    .is_err());
            }
            let head = h.cursor().unwrap();
            if let Some(prior) = prior_head {
                assert_eq!(head, prior);
            }
            prior_head = Some(head);
            let mut cursor = h.cursor().unwrap();
            cursor.seq = "0".into();
            let events = h.events(cursor, 32).unwrap().events;
            assert_eq!(
                events.iter().filter(|e| e.kind == "unknown").count(),
                usize::from(expected == SmsState::Unknown)
            );
            assert_eq!(
                events.iter().filter(|e| e.kind == "submitted").count(),
                usize::from(expected == SmsState::Submitted)
            );
            if count.exists() {
                assert_eq!(std::fs::read(&count).unwrap(), b"1");
            }
        }
    }
}

#[test]
fn backup_restore_quarantines_old_prepared_and_preserves_submitted_receipts() {
    let (_d, store, p, b) = fixture();
    let h = store.handle();
    let draft_op = prepare(&h, &p, &b, "prepared");
    let submitted = prepare(&h, &p, &b, "submitted");
    h.begin_authorized(submitted.id.clone(), p.clone(), b.clone(), || true)
        .unwrap();
    bind(&h, &p, &b, &submitted.id);
    h.permit_send(submitted.id.clone(), p.clone(), b.clone(), || true)
        .unwrap();
    h.resolve_sent(submitted.id.clone(), p.clone(), b.clone(), || true)
        .unwrap();
    let backup = super::tests::directory();
    h.backup(backup.path()).unwrap();
    let target = super::tests::directory();
    let restored = Store::restore(backup.path(), target.path()).unwrap();
    let rh = restored.handle();
    assert_eq!(
        rh.operation(&draft_op.id, &p.id).unwrap().state,
        SmsState::Unknown
    );
    assert_eq!(
        rh.operation(&submitted.id, &p.id).unwrap().state,
        SmsState::Submitted
    );
    assert!(rh.begin_authorized(draft_op.id, p, b, || true).is_err());
}
