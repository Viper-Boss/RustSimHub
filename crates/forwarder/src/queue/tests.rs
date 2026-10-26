// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

const REALM: &str = "00000000-0000-4000-8000-000000000001";
const TARGET: &str = "00000000-0000-4000-8000-000000000002";
const RULE: &str = "00000000-0000-4000-8000-000000000003";
fn binding() -> Binding {
    Binding {
        device_key: [1; 32],
        realm: REALM.into(),
        owner_key: [2; 32],
        owner_generation: 1,
    }
}
fn id() -> String {
    Uuid::new_v4().to_string()
}
fn head(seq: u64) -> InboxCursor {
    InboxCursor {
        db_epoch: REALM.into(),
        seq: seq.to_string(),
    }
}
fn policy() -> Policy {
    Policy {
        enabled: true,
        targets: vec![Target {
            id: TARGET.into(),
            destination: Destination::Webhook {
                url: Secret::new("https://example.invalid/synthetic".into()),
                bearer: Some(Secret::new("synthetic-bearer".into())),
            },
        }],
        rules: vec![Rule {
            id: RULE.into(),
            targets: vec![TARGET.into()],
            sender_equals: None,
            body_contains: None,
            line: None,
            share: Share::Notice,
            conditions: None,
        }],
    }
}
fn message() -> Message {
    Message {
        id: id(),
        line: LineEpoch(2),
        sender: "synthetic sender".into(),
        timestamp: "synthetic time".into(),
        body: "合成验证码 123456 🙂".into(),
        duplicate_candidate: false,
        binary: false,
    }
}
fn directory() -> TempDir {
    let d = TempDir::new().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    d
}
fn queue() -> (TempDir, Queue) {
    let d = directory();
    let q = Queue::open(d.path(), binding()).unwrap();
    (d, q)
}
fn enabled() -> (TempDir, Queue) {
    let (d, mut q) = queue();
    q.configure(&binding(), 0, &id(), policy(), head(5))
        .unwrap();
    (d, q)
}

#[test]
fn extended_rules_persist_filter_and_enqueue_only_sanitized_immutable_payloads() {
    let (d, mut q) = queue();
    let mut p = policy();
    p.rules[0].share = Share::MaskedText;
    p.rules[0].conditions = Some(RuleConditions {
        body_regex: Some(BodyPattern::new("验证码 [0-9]{6}".into()).unwrap()),
        time_window: Some(TimeWindow {
            start_minute: 540,
            end_minute: 600,
            utc_offset_minutes: 480,
        }),
    });
    q.configure(&binding(), 0, &id(), p, head(5)).unwrap();
    let mut m = message();
    m.timestamp = "2026-10-27T11:00:00+08:00".into();
    assert_eq!(q.ingest(&binding(), 1, head(6), Some(&m)).unwrap(), 0);
    assert_eq!(q.event_cursor().unwrap().seq, "6");
    m.id = id();
    m.timestamp = "2026-10-27T09:30:00+08:00".into();
    assert_eq!(q.ingest(&binding(), 1, head(7), Some(&m)).unwrap(), 1);
    // Opening the same real DB again preserves both filter semantics and the outbox.
    drop(q);
    let mut q = Queue::open(d.path(), binding()).unwrap();
    assert_eq!(q.policy_view().unwrap().rules[0].share, Share::MaskedText);
    let claim = q.claim(&binding(), 1, LineEpoch(2)).unwrap().unwrap();
    let body = claim.request.body.expose();
    assert!(body.contains("******"));
    assert!(!body.contains("123456"));
    assert!(!body.contains("synthetic sender"));
    assert_eq!(q.ingest(&binding(), 1, head(7), Some(&m)).unwrap(), 0);
    q.finish(&binding(), &claim, State::Confirmed).unwrap();
}

#[test]
fn failed_extended_edit_keeps_policy_cursor_and_receipts_unchanged() {
    let (_d, mut q) = enabled();
    let before = serde_json::to_value(q.policy_view().unwrap()).unwrap();
    let mut v = before.clone();
    v["rules"][0]["share"] = "masked_text".into();
    v["targets"][0] = serde_json::json!({"id":TARGET,"destination":{"kind":"keep"}});
    let edit: PolicyEdit = serde_json::from_value(v.clone()).unwrap();
    assert!(q.resolve_policy_edit(&binding(), edit).is_err());
    assert_eq!(
        serde_json::to_value(q.policy_view().unwrap()).unwrap(),
        before
    );
    assert_eq!(q.event_cursor().unwrap().seq, "5");
    v["rules_revision"] = 2.into();
    let edit: PolicyEdit = serde_json::from_value(v).unwrap();
    let p = q.resolve_policy_edit(&binding(), edit).unwrap();
    q.configure(&binding(), 1, &id(), p, head(5)).unwrap();
    let legacy = serde_json::json!({"enabled":true,"targets":[{"id":TARGET,"destination":{"kind":"keep"}}],"rules":[{"id":RULE,"targets":[TARGET],"share":"notice"}]});
    assert!(q
        .resolve_policy_edit(&binding(), serde_json::from_value(legacy).unwrap())
        .is_err());
    assert_eq!(q.status().unwrap().generation, 2);
    assert_eq!(q.status().unwrap().queued, 0);
}

#[test]
fn cached_policy_is_reused_but_never_hides_changed_or_corrupt_database_bytes() {
    let (_d, mut q) = enabled();
    let first = q.meta().unwrap().policy;
    let second = q.meta().unwrap().policy;
    assert!(std::sync::Arc::ptr_eq(&first, &second));
    let mut p = policy();
    p.rules[0].share = Share::MaskedText;
    q.configure(&binding(), 1, &id(), p, head(5)).unwrap();
    let changed = q.meta().unwrap().policy;
    assert!(!std::sync::Arc::ptr_eq(&first, &changed));
    assert_eq!(changed.rules[0].share, Share::MaskedText);
    // No generation change: even corruption outside the normal writer cannot
    // make a successful cache hit return stale policy or old compiled filters.
    q.conn
        .execute(
            "UPDATE meta SET policy=?",
            ["{\"enabled\":true,\"invalid\":true}"],
        )
        .unwrap();
    assert!(q.status().is_err());
    assert!(q.ingest(&binding(), 2, head(6), Some(&message())).is_err());
}

fn probe() -> (TempDir, Queue, String) {
    let (d, mut q) = queue();
    let mut p = policy();
    p.enabled = false;
    q.configure(&binding(), 0, &id(), p, head(5)).unwrap();
    let request = id();
    q.begin_probe(&binding(), 1, &request, TARGET).unwrap();
    (d, q, request)
}

#[test]
fn probe_never_enables_saved_policy_or_ingests_real_sms() {
    let (_d, mut q, request) = probe();
    assert!(q.status().unwrap().enabled);
    assert_eq!(
        q.ingest(&binding(), 1, head(6), Some(&message())).unwrap(),
        0
    );
    assert_eq!(q.event_cursor().unwrap().seq, "5");
    let (enabled, encoded): (bool, String) = q
        .conn
        .query_row("SELECT enabled,policy FROM meta", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert!(!enabled);
    assert!(!serde_json::from_str::<Policy>(&encoded).unwrap().enabled);
    let receipt = q
        .enqueue_test(&binding(), 1, &request, TARGET, LineEpoch(2))
        .unwrap();
    assert!(receipt.synthetic);
    assert!(q
        .enqueue_test(&binding(), 1, &id(), TARGET, LineEpoch(2))
        .is_err());
    assert!(q
        .configure(&binding(), 1, &id(), policy(), head(6))
        .is_err());
    assert!(q.end_probe(&request).unwrap());
    assert!(!q.status().unwrap().enabled);
    assert_eq!(
        q.test_receipt(&binding(), &request).unwrap().unwrap().state,
        State::Cancelled
    );
}

#[test]
fn probe_cleanup_is_exact_and_preserves_unknown_after_submit() {
    let (_d, mut q, request) = probe();
    q.enqueue_test(&binding(), 1, &request, TARGET, LineEpoch(2))
        .unwrap();
    let claim = q.claim(&binding(), 1, LineEpoch(2)).unwrap().unwrap();
    assert!(!q.end_probe(&id()).unwrap());
    assert!(q.probe_active());
    assert!(q.end_probe(&request).unwrap());
    assert_eq!(
        q.test_receipt(&binding(), &request).unwrap().unwrap().state,
        State::Unknown
    );
    assert!(q.finish(&binding(), &claim, State::Confirmed).is_err());
    assert!(q.begin_probe(&binding(), 1, &request, TARGET).is_err());
    assert!(q.begin_probe(&binding(), 1, &id(), TARGET).is_err());
}

#[test]
fn probe_has_a_fixed_deadline_and_no_renewal() {
    let (_d, mut q, request) = probe();
    q.enqueue_test(&binding(), 1, &request, TARGET, LineEpoch(2))
        .unwrap();
    q.probe.as_mut().unwrap().deadline = Instant::now() - Duration::from_millis(1);
    assert!(!q.status().unwrap().enabled);
    assert!(q.claim(&binding(), 1, LineEpoch(2)).is_err());
    assert!(q.begin_probe(&binding(), 1, &id(), TARGET).is_err());
    assert!(q.end_probe(&request).unwrap());
}

#[test]
fn probe_revocation_does_not_depend_on_receipt_write_success() {
    let (_d, mut q, request) = probe();
    q.enqueue_test(&binding(), 1, &request, TARGET, LineEpoch(2))
        .unwrap();
    q.conn.execute_batch("PRAGMA query_only=ON").unwrap();
    assert!(q.end_probe(&request).is_err());
    assert!(!q.status().unwrap().enabled);
    assert!(q.claim(&binding(), 1, LineEpoch(2)).is_err());
    q.conn.execute_batch("PRAGMA query_only=OFF").unwrap();
    assert!(q.end_probe(&request).unwrap());
    assert_eq!(
        q.test_receipt(&binding(), &request).unwrap().unwrap().state,
        State::Cancelled
    );
}

#[test]
fn probe_is_not_resumed_after_core_restart() {
    let (d, mut q, request) = probe();
    q.enqueue_test(&binding(), 1, &request, TARGET, LineEpoch(2))
        .unwrap();
    drop(q);
    let q = Queue::open(d.path(), binding()).unwrap();
    assert!(!q.status().unwrap().enabled);
    assert!(!q.probe_active());
    assert_eq!(
        q.test_receipt(&binding(), &request).unwrap().unwrap().state,
        State::Unknown
    );
    assert_eq!(q.event_cursor().unwrap().seq, "5");
}

#[test]
fn probe_rejects_an_enabled_service_stale_generation_or_binding() {
    let (_d, mut q) = enabled();
    assert!(q.begin_probe(&binding(), 1, &id(), TARGET).is_err());
    q.configure(
        &binding(),
        1,
        &id(),
        Policy {
            enabled: false,
            ..policy()
        },
        head(5),
    )
    .unwrap();
    assert!(q.begin_probe(&binding(), 1, &id(), TARGET).is_err());
    let mut wrong = binding();
    wrong.owner_generation += 1;
    assert!(q.begin_probe(&wrong, 2, &id(), TARGET).is_err());
    assert!(q.begin_probe(&binding(), 2, &id(), &id()).is_err());
    assert!(!q.status().unwrap().enabled);
}

#[test]
fn default_off_has_no_producer_read_and_cannot_claim() {
    let (_d, mut q) = queue();
    assert!(!q.status().unwrap().enabled);
    assert!(!q.status().unwrap().process_state_verified);
    assert!(matches!(
        q.ingest(&binding(), 0, head(1), Some(&message())),
        Err(Error::Disabled)
    ));
    assert!(matches!(
        q.claim(&binding(), 0, LineEpoch(2)),
        Err(Error::Disabled)
    ));
    assert_eq!(q.event_cursor().unwrap().seq, "0");
}

#[test]
fn live_line_read_is_fenced_and_failed_reads_never_reserve_a_delivery() {
    let (_d, mut q) = queue();
    assert!(matches!(
        q.claim_with_live_line(&binding(), 0, || panic!("off must not read")),
        Err(Error::Disabled)
    ));
    q.configure(&binding(), 0, &id(), policy(), head(5))
        .unwrap();
    q.ingest(&binding(), 1, head(6), Some(&message())).unwrap();
    assert!(matches!(
        q.claim_with_live_line(&binding(), 0, || panic!("stale generation must not read")),
        Err(Error::Stale)
    ));
    assert!(matches!(
        q.claim_with_live_line(&binding(), 1, || Err(Error::Storage)),
        Err(Error::Storage)
    ));
    assert_eq!(q.status().unwrap().queued, 1);
    assert_eq!(q.status().unwrap().in_flight, 0);
    assert!(q.claim(&binding(), 1, LineEpoch(2)).unwrap().is_some());
}
#[test]
fn configure_idempotency_generation_and_binding_are_exact() {
    let (_d, mut q) = queue();
    let request = id();
    assert_eq!(
        q.configure(&binding(), 0, &request, policy(), head(5))
            .unwrap(),
        1
    );
    assert_eq!(
        q.configure(&binding(), 0, &request, policy(), head(5))
            .unwrap(),
        1
    );
    assert_eq!(
        q.configure(&binding(), 0, &request, policy(), head(6))
            .unwrap(),
        1
    );
    assert_eq!(
        q.event_cursor().unwrap().seq,
        "5",
        "retry cannot move the first baseline"
    );
    assert!(matches!(
        q.configure(&binding(), 0, &request, Policy::default(), head(6)),
        Err(Error::Conflict)
    ));
    assert!(matches!(
        q.configure(&binding(), 0, &id(), policy(), head(5)),
        Err(Error::Stale)
    ));
    let mut foreign = binding();
    foreign.owner_generation = 2;
    assert!(matches!(
        q.configure(&foreign, 1, &id(), policy(), head(5)),
        Err(Error::Forbidden)
    ));
    let mut restored = head(5);
    restored.db_epoch = id();
    assert!(matches!(
        q.configure(&binding(), 1, &id(), policy(), restored),
        Err(Error::Stale)
    ));
    q.configure(&binding(), 1, &id(), Policy::default(), head(6))
        .unwrap();
    assert!(matches!(
        q.configure(&binding(), 0, &request, policy(), head(5)),
        Err(Error::Stale)
    ));
}
#[test]
fn capture_cursor_and_all_targets_commit_together_and_overlap_sends_once() {
    let (_d, mut q) = queue();
    let mut p = policy();
    let mut duplicate = p.rules[0].clone();
    duplicate.id = id();
    duplicate.share = Share::FullText;
    p.rules.push(duplicate);
    q.configure(&binding(), 0, &id(), p, head(5)).unwrap();
    let m = message();
    assert_eq!(q.ingest(&binding(), 1, head(6), Some(&m)).unwrap(), 1);
    assert_eq!(q.ingest(&binding(), 1, head(6), Some(&m)).unwrap(), 0);
    assert_eq!(q.ingest(&binding(), 1, head(7), Some(&m)).unwrap(), 0);
    let c = q.claim(&binding(), 1, LineEpoch(2)).unwrap().unwrap();
    let body = c.request.body.expose();
    assert!(!body.contains(&m.sender) && !body.contains("123456"));
    assert!(matches!(
        q.claim(&binding(), 1, LineEpoch(2)),
        Err(Error::Busy)
    ));
    q.finish(&binding(), &c, State::Confirmed).unwrap();
    assert!(matches!(
        q.finish(&binding(), &c, State::Confirmed),
        Err(Error::Stale)
    ));
    assert!(q.claim(&binding(), 1, LineEpoch(2)).unwrap().is_none());
    assert_eq!(q.event_cursor().unwrap().seq, "7");
    let receipt = q.receipts(0, 32).unwrap();
    assert_eq!(receipt.len(), 1);
    assert_eq!(receipt[0].state, State::Confirmed);
    assert!(q
        .receipts(receipt[0].seq.parse().unwrap(), 32)
        .unwrap()
        .is_empty());
    let text = serde_json::to_string(&receipt).unwrap();
    assert!(!receipt[0].synthetic);
    for private in [
        "synthetic sender",
        "synthetic time",
        "synthetic-bearer",
        "example.invalid",
        "123456",
    ] {
        assert!(!text.contains(private));
    }
    let payload: Option<String> = q
        .conn
        .query_row("SELECT request FROM jobs", [], |r| r.get(0))
        .unwrap();
    assert!(payload.is_none());
}
#[test]
fn binary_and_duplicate_candidates_never_enter_outbox() {
    let (_d, mut q) = enabled();
    let mut m = message();
    m.binary = true;
    assert_eq!(q.ingest(&binding(), 1, head(6), Some(&m)).unwrap(), 0);
    m.binary = false;
    m.duplicate_candidate = true;
    assert_eq!(q.ingest(&binding(), 1, head(7), Some(&m)).unwrap(), 0);
    assert!(q.receipts(0, 32).unwrap().is_empty());
    assert_eq!(q.event_cursor().unwrap().seq, "7");
}
#[test]
fn disabling_cancels_only_unsent_and_fences_late_result() {
    let (_d, mut q) = enabled();
    q.ingest(&binding(), 1, head(6), Some(&message())).unwrap();
    q.ingest(&binding(), 1, head(7), Some(&message())).unwrap();
    let c = q.claim(&binding(), 1, LineEpoch(2)).unwrap().unwrap();
    q.configure(&binding(), 1, &id(), Policy::default(), head(7))
        .unwrap();
    let r = q.receipts(0, 32).unwrap();
    assert_eq!(r[0].state, State::Unknown);
    assert_eq!(r[1].state, State::Cancelled);
    assert!(matches!(
        q.finish(&binding(), &c, State::Confirmed),
        Err(Error::Stale)
    ));
    assert!(matches!(
        q.claim(&binding(), 2, LineEpoch(2)),
        Err(Error::Disabled)
    ));
    q.configure(&binding(), 2, &id(), policy(), head(10))
        .unwrap();
    assert!(q.claim(&binding(), 3, LineEpoch(2)).unwrap().is_none());
    assert_eq!(q.status().unwrap().unknown, 1);
    assert_eq!(q.prune_terminal(&binding(), 3, i64::MAX).unwrap(), 1);
    assert_eq!(q.receipts(0, 32).unwrap()[0].state, State::Unknown);
}
#[test]
fn crash_after_mark_before_ack_is_unknown_and_never_reclaimed() {
    let (d, mut q) = enabled();
    q.ingest(&binding(), 1, head(6), Some(&message())).unwrap();
    let c = q.claim(&binding(), 1, LineEpoch(2)).unwrap().unwrap();
    drop(q);
    let mut q = Queue::open(d.path(), binding()).unwrap();
    assert_eq!(q.status().unwrap().unknown, 1);
    assert!(q.claim(&binding(), 1, LineEpoch(2)).unwrap().is_none());
    assert!(matches!(
        q.finish(&binding(), &c, State::Confirmed),
        Err(Error::Stale)
    ));
}
#[test]
fn sim_change_cancels_unsent_jobs_and_empty_or_stale_line_does_not_send() {
    let (_d, mut q) = enabled();
    q.ingest(&binding(), 1, head(6), Some(&message())).unwrap();
    assert!(matches!(
        q.claim(&binding(), 1, LineEpoch(0)),
        Err(Error::Invalid)
    ));
    assert!(q.claim(&binding(), 1, LineEpoch(3)).unwrap().is_none());
    assert_eq!(q.receipts(0, 32).unwrap()[0].state, State::Cancelled);
}
#[test]
fn actual_sqlite_failure_rolls_back_jobs_and_cursor() {
    let (_d, mut q) = enabled();
    q.conn.execute_batch("CREATE TRIGGER fail_cursor BEFORE UPDATE OF cursor ON meta BEGIN SELECT RAISE(ABORT,'synthetic');END;").unwrap();
    assert!(matches!(
        q.ingest(&binding(), 1, head(6), Some(&message())),
        Err(Error::Storage)
    ));
    assert!(q.receipts(0, 32).unwrap().is_empty());
    assert_eq!(q.event_cursor().unwrap().seq, "5");
}
#[test]
fn bounded_backpressure_keeps_cursor_and_unknown_receipts() {
    let (_d, mut q) = enabled();
    for seq in 6..6 + MAX_PENDING {
        q.ingest(&binding(), 1, head(seq as u64), Some(&message()))
            .unwrap();
    }
    let before = q.event_cursor().unwrap();
    assert!(matches!(
        q.ingest(&binding(), 1, head(300), Some(&message())),
        Err(Error::Full)
    ));
    assert_eq!(q.event_cursor().unwrap().seq, before.seq);
    // Skipped events can advance even when the queue is full.
    q.ingest(&binding(), 1, head(300), None).unwrap();
    assert_eq!(q.status().unwrap().queued, MAX_PENDING);
}
#[test]
fn rejects_foreign_owner_unsafe_files_and_second_writer() {
    let (d, q) = queue();
    assert!(matches!(Queue::open(d.path(), binding()), Err(Error::Busy)));
    drop(q);
    let mut b = binding();
    b.device_key = [3; 32];
    assert!(matches!(Queue::open(d.path(), b), Err(Error::Forbidden)));
    let other = directory();
    std::os::unix::fs::symlink(
        d.path().join("outbox.sqlite3"),
        other.path().join("outbox.sqlite3"),
    )
    .unwrap();
    assert!(Queue::open(other.path(), binding()).is_err());
    let other = directory();
    fs::hard_link(
        d.path().join("outbox.sqlite3"),
        other.path().join("outbox.sqlite3"),
    )
    .unwrap();
    assert!(matches!(
        Queue::open(other.path(), binding()),
        Err(Error::Forbidden)
    ));
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        Queue::open(d.path(), binding()),
        Err(Error::Forbidden)
    ));
}
#[test]
fn wrong_attempt_cannot_ack_or_prune_unconfirmed_payload() {
    let (_d, mut q) = enabled();
    q.ingest(&binding(), 1, head(6), Some(&message())).unwrap();
    let mut c = q.claim(&binding(), 1, LineEpoch(2)).unwrap().unwrap();
    let old = c.attempt_id.clone();
    c.attempt_id = id();
    assert!(matches!(
        q.finish(&binding(), &c, State::Confirmed),
        Err(Error::Stale)
    ));
    c.attempt_id = old;
    q.finish(&binding(), &c, State::Unknown).unwrap();
    assert_eq!(q.prune_terminal(&binding(), 1, i64::MAX).unwrap(), 0);
    let payload: Option<String> = q
        .conn
        .query_row("SELECT request FROM jobs", [], |r| r.get(0))
        .unwrap();
    assert!(payload.is_some());
}
#[test]
fn real_business_event_tail_skips_history_preserves_restart_and_sim_candidates() {
    use rustsimhub_storage::{IncomingSms, LineObservation, LineStatus, Store};
    let business = directory();
    let s = Store::open(business.path()).unwrap();
    let h = s.handle();
    let observed = |backend: &str| {
        h.observe_line(LineObservation {
            backend: backend.into(),
            fingerprint: Some([4; 32]),
            status: LineStatus::Ready,
        })
        .unwrap()
        .line
    };
    let line = observed("synthetic-modem-1");
    let input = |line, source: &str, text: &str| IncomingSms {
        line,
        source: source.into(),
        sender: "synthetic".into(),
        timestamp: "synthetic time".into(),
        text: text.into(),
        data: vec![],
    };
    h.receive_sms(input(line, "old", "old history")).unwrap();
    let tail = h.inbox_event_head().unwrap();
    let mut b = binding();
    b.realm = tail.db_epoch.clone();
    let outbox = directory();
    let mut q = Queue::open(outbox.path(), b.clone()).unwrap();
    q.configure(&b, 0, &id(), policy(), tail).unwrap();
    assert_eq!(q.capture(&b, 1, &h).unwrap(), 0);
    let received = h
        .receive_sms(input(line, "new", "new synthetic content"))
        .unwrap();
    assert_eq!(q.capture(&b, 1, &h).unwrap(), 1);
    assert_eq!(q.capture(&b, 1, &h).unwrap(), 0);
    assert_eq!(q.receipts(0, 32).unwrap()[0].message_id, received.id);
    drop(q);
    let mut q = Queue::open(outbox.path(), b.clone()).unwrap();
    assert_eq!(q.capture(&b, 1, &h).unwrap(), 0);
    let next = observed("synthetic-modem-2");
    assert_ne!(next, line);
    let candidate = h
        .receive_sms(input(next, "reobserved", "new synthetic content"))
        .unwrap();
    assert!(candidate.duplicate_candidate.is_some());
    assert_eq!(q.capture(&b, 1, &h).unwrap(), 0);
    assert!(q.claim(&b, 1, next).unwrap().is_none());
    assert_eq!(q.receipts(0, 32).unwrap()[0].state, State::Cancelled);
}
#[test]
fn unknown_provider_acknowledgements_and_json_escaping_are_not_delivery_success() {
    let mut p = policy();
    p.rules[0].share = Share::FullText;
    let m = message();
    let request = prepare(&p.targets[0], &p.rules[0], &m, &id()).unwrap();
    assert!(request.acknowledged(204, b""));
    assert!(!request.acknowledged(503, b""));
    let value: serde_json::Value = serde_json::from_str(request.body.expose()).unwrap();
    assert_eq!(value["text"], m.body);
    for destination in [
        Destination::Bark {
            server: "https://example.invalid".into(),
            key: Secret::new("synthetic-key".into()),
        },
        Destination::Telegram {
            bot_token: Secret::new("123:synthetic_token".into()),
            chat_id: Secret::new("synthetic-chat".into()),
        },
    ] {
        let target = Target {
            id: TARGET.into(),
            destination,
        };
        let r = prepare(&target, &p.rules[0], &m, &id()).unwrap();
        assert!(!r.acknowledged(200, br#"{"ok":false,"code":500}"#));
        assert!(r.acknowledged(200, br#"{"ok":true,"code":200}"#));
        assert!(!r.acknowledged(200, b"not json"));
    }
}
#[test]
fn rejects_unsafe_destinations_and_unbounded_rules_before_config_commit() {
    for url in [
        "http://example.invalid",
        "https://user:password@example.invalid",
        "https://example.invalid/#fragment",
        "https://example.invalid/\r\nHeader:value",
        "https://example.invalid:0/",
    ] {
        assert!(
            Destination::Webhook {
                url: Secret::new(url.into()),
                bearer: None
            }
            .validate()
            .is_err(),
            "{url}"
        );
    }
    let (_d, mut q) = queue();
    let mut p = policy();
    p.rules[0].targets.push(TARGET.into());
    assert!(q.configure(&binding(), 0, &id(), p, head(5)).is_err());
    assert_eq!(q.status().unwrap().generation, 0);
    let mut p = policy();
    p.targets.push(Target {
        id: TARGET.into(),
        destination: Destination::Webhook {
            url: Secret::new("https://example.invalid/".into()),
            bearer: None,
        },
    });
    assert!(p.validate().is_err());
    let mut p = policy();
    p.rules[0].body_contains = Some("x".repeat(257));
    assert!(p.validate().is_err());
}

#[test]
fn disabled_capture_never_reads_even_a_stopped_business_actor() {
    let business = directory();
    let s = rustsimhub_storage::Store::open(business.path()).unwrap();
    let h = s.handle();
    drop(s);
    let (_d, mut q) = queue();
    assert!(matches!(q.capture(&binding(), 0, &h), Err(Error::Disabled)));
}
#[test]
fn configuration_cannot_rewind_cursor_or_replay_evicted_old_request() {
    let (_d, mut q) = queue();
    let first = id();
    q.configure(&binding(), 0, &first, policy(), head(5))
        .unwrap();
    q.ingest(&binding(), 1, head(6), Some(&message())).unwrap();
    assert!(matches!(
        q.configure(&binding(), 1, &id(), Policy::default(), head(5)),
        Err(Error::Stale)
    ));
    for g in 1..66 {
        q.configure(&binding(), g, &id(), Policy::default(), head(6))
            .unwrap();
    }
    assert!(matches!(
        q.configure(&binding(), 0, &first, policy(), head(5)),
        Err(Error::Stale)
    ));
    let count: i64 = q
        .conn
        .query_row("SELECT count(*) FROM requests", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 64);
}
#[test]
fn recipient_insert_failure_rolls_back_entire_fanout_and_cursor() {
    let (_d, mut q) = queue();
    let mut p = policy();
    let other = id();
    p.targets.push(Target {
        id: other.clone(),
        destination: Destination::Webhook {
            url: Secret::new("https://example.invalid/second".into()),
            bearer: None,
        },
    });
    p.rules[0].targets.push(other.clone());
    q.configure(&binding(), 0, &id(), p, head(5)).unwrap();
    q.conn.execute_batch(&format!("CREATE TRIGGER fail_second BEFORE INSERT ON jobs WHEN NEW.target_id='{other}' BEGIN SELECT RAISE(ABORT,'synthetic');END;")).unwrap();
    assert!(matches!(
        q.ingest(&binding(), 1, head(6), Some(&message())),
        Err(Error::Storage)
    ));
    assert!(q.receipts(0, 32).unwrap().is_empty());
    assert_eq!(q.event_cursor().unwrap().seq, "5");
}
#[test]
fn failed_configuration_transaction_never_claims_the_process_has_stopped() {
    let (_d, mut q) = enabled();
    q.ingest(&binding(), 1, head(6), Some(&message())).unwrap();
    q.conn.execute_batch("CREATE TRIGGER fail_policy BEFORE UPDATE OF policy ON meta BEGIN SELECT RAISE(ABORT,'synthetic');END;").unwrap();
    assert!(matches!(
        q.configure(&binding(), 1, &id(), Policy::default(), head(6)),
        Err(Error::Storage)
    ));
    let s = q.status().unwrap();
    assert!(s.enabled);
    assert!(!s.process_state_verified);
    assert_eq!(s.generation, 1);
    assert_eq!(s.queued, 1);
}
#[test]
fn selectors_and_provider_notice_modes_do_not_leak_full_text_by_default() {
    let mut p = policy();
    let m = message();
    p.rules[0].sender_equals = Some("different".into());
    assert!(!p.rules[0].matches(&m));
    p.rules[0].sender_equals = Some(m.sender.clone());
    p.rules[0].body_contains = Some("123456".into());
    assert!(p.rules[0].matches(&m));
    p.rules[0].line = Some(LineEpoch(9));
    assert!(!p.rules[0].matches(&m));
    p.rules[0].line = None;
    for destination in [
        Destination::Bark {
            server: "https://example.invalid".into(),
            key: Secret::new("synthetic-key".into()),
        },
        Destination::Telegram {
            bot_token: Secret::new("123:synthetic_token".into()),
            chat_id: Secret::new("synthetic-chat".into()),
        },
    ] {
        let r = prepare(
            &Target {
                id: TARGET.into(),
                destination,
            },
            &p.rules[0],
            &m,
            &id(),
        )
        .unwrap();
        assert!(!r.body.expose().contains("123456") && !r.body.expose().contains(&m.sender));
    }
}

#[test]
fn maximum_telegram_body_remains_complete_and_disables_link_previews() {
    let mut p = policy();
    p.rules[0].share = Share::FullText;
    let mut m = message();
    m.body = "x".repeat(4096);
    let target = Target {
        id: TARGET.into(),
        destination: Destination::Telegram {
            bot_token: Secret::new("123:synthetic_token".into()),
            chat_id: Secret::new("synthetic-chat".into()),
        },
    };
    let r = prepare(&target, &p.rules[0], &m, &id()).unwrap();
    let v: serde_json::Value = serde_json::from_str(r.body.expose()).unwrap();
    assert_eq!(v["text"], m.body);
    assert_eq!(v["link_preview_options"]["is_disabled"], true);
    assert!(v.get("parse_mode").is_none());
}

#[test]
fn existing_store_accepted_sender_and_control_text_cannot_block_following_sms() {
    use rustsimhub_storage::{IncomingSms, LineObservation, LineStatus, Store};
    let business = directory();
    let s = Store::open(business.path()).unwrap();
    let h = s.handle();
    let line = h
        .observe_line(LineObservation {
            backend: "synthetic-modem".into(),
            fingerprint: Some([11; 32]),
            status: LineStatus::Ready,
        })
        .unwrap()
        .line;
    let tail = h.inbox_event_head().unwrap();
    let mut b = binding();
    b.realm = tail.db_epoch.clone();
    let outbox = directory();
    let mut q = Queue::open(outbox.path(), b.clone()).unwrap();
    q.configure(&b, 0, &id(), policy(), tail).unwrap();
    for (source, sender, body) in [
        ("first", "x".repeat(256), "合成\\n\0文本"),
        ("second", "synthetic".into(), "following synthetic SMS"),
    ] {
        h.receive_sms(IncomingSms {
            line,
            source: source.into(),
            sender,
            timestamp: "synthetic time".into(),
            text: body.into(),
            data: vec![],
        })
        .unwrap();
    }
    assert_eq!(q.capture(&b, 1, &h).unwrap(), 2);
    assert_eq!(q.receipts(0, 32).unwrap().len(), 2);
}

#[test]
fn synthetic_test_is_single_target_idempotent_and_survives_pruning_and_restart() {
    let (d, mut q) = enabled();
    let before = q.event_cursor().unwrap();
    let request = id();
    let receipt = q
        .enqueue_test(&binding(), 1, &request, TARGET, LineEpoch(2))
        .unwrap();
    assert!(receipt.synthetic);
    assert_eq!(receipt.id, request);
    assert_eq!(
        q.enqueue_test(&binding(), 1, &request, TARGET, LineEpoch(2))
            .unwrap()
            .id,
        request
    );
    assert_eq!(q.receipts(0, 32).unwrap().len(), 1);
    assert_eq!(q.event_cursor().unwrap().seq, before.seq);
    let claim = q.claim(&binding(), 1, LineEpoch(2)).unwrap().unwrap();
    let body: serde_json::Value = serde_json::from_str(claim.request.body.expose()).unwrap();
    assert_eq!(body["event"], "forwarding_test");
    assert_eq!(body["text"], SYNTHETIC_NOTICE);
    assert_eq!(body["test"], true);
    assert!(body.get("line").is_none() && body.get("sender").is_none());
    q.finish(&binding(), &claim, State::Confirmed).unwrap();
    assert_eq!(q.prune_terminal(&binding(), 1, i64::MAX).unwrap(), 0);
    drop(q);
    let mut q = Queue::open(d.path(), binding()).unwrap();
    assert_eq!(
        q.enqueue_test(&binding(), 1, &request, TARGET, LineEpoch(2))
            .unwrap()
            .state,
        State::Confirmed
    );
    assert!(q.claim(&binding(), 1, LineEpoch(2)).unwrap().is_none());
    assert!(matches!(
        q.enqueue_test(&binding(), 1, &request, &id(), LineEpoch(2)),
        Err(Error::Conflict)
    ));
    q.configure(&binding(), 1, &id(), policy(), head(5))
        .unwrap();
    assert!(matches!(
        q.enqueue_test(&binding(), 1, &request, TARGET, LineEpoch(2)),
        Err(Error::Stale)
    ));
    assert_eq!(q.prune_terminal(&binding(), 2, i64::MAX).unwrap(), 1);
    assert!(q.test_receipt(&binding(), &request).unwrap().is_none());
    assert!(q
        .enqueue_test(&binding(), 1, &request, TARGET, LineEpoch(2))
        .is_err());
}

#[test]
fn synthetic_test_has_a_bounded_generation_history_and_keeps_unknown() {
    let (_d, mut q) = enabled();
    for _ in 0..64 {
        q.enqueue_test(&binding(), 1, &id(), TARGET, LineEpoch(2))
            .unwrap();
    }
    assert!(matches!(
        q.enqueue_test(&binding(), 1, &id(), TARGET, LineEpoch(2)),
        Err(Error::Full)
    ));
    let claim = q.claim(&binding(), 1, LineEpoch(2)).unwrap().unwrap();
    q.finish(&binding(), &claim, State::Unknown).unwrap();
    q.configure(&binding(), 1, &id(), policy(), head(5))
        .unwrap();
    assert_eq!(q.prune_terminal(&binding(), 2, i64::MAX).unwrap(), 63);
    assert_eq!(
        q.test_receipt(&binding(), &claim.id)
            .unwrap()
            .unwrap()
            .state,
        State::Unknown
    );
    assert!(q.claim(&binding(), 2, LineEpoch(2)).unwrap().is_none());
    let mut foreign = binding();
    foreign.owner_generation += 1;
    assert!(q
        .enqueue_test(&foreign, 2, &id(), TARGET, LineEpoch(2))
        .is_err());
    assert!(q.test_receipt(&foreign, &claim.id).is_err());
    assert!(q
        .enqueue_test(&binding(), 2, &id(), TARGET, LineEpoch(0))
        .is_err());
    assert!(q
        .enqueue_test(&binding(), 2, &id(), &id(), LineEpoch(2))
        .is_err());
    let mut disabled = policy();
    disabled.enabled = false;
    q.configure(&binding(), 2, &id(), disabled, head(5))
        .unwrap();
    assert!(matches!(
        q.enqueue_test(&binding(), 3, &id(), TARGET, LineEpoch(2)),
        Err(Error::Disabled)
    ));
}

#[test]
fn format_one_migrates_only_after_binding_validation_preserving_sms_and_unknown() {
    let (d, mut q) = enabled();
    q.ingest(&binding(), 1, head(6), Some(&message())).unwrap();
    let claim = q.claim(&binding(), 1, LineEpoch(2)).unwrap().unwrap();
    let payload = claim.request.body.expose().to_owned();
    q.conn
        .execute_batch("ALTER TABLE jobs DROP COLUMN purpose; PRAGMA user_version=1;")
        .unwrap();
    drop(q);
    let mut foreign = binding();
    foreign.owner_generation += 1;
    assert!(Queue::open(d.path(), foreign).is_err());
    let c = Connection::open(d.path().join("outbox.sqlite3")).unwrap();
    assert_eq!(
        c.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        1
    );
    drop(c);
    let q = Queue::open(d.path(), binding()).unwrap();
    assert_eq!(
        q.conn
            .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        FORMAT
    );
    assert_eq!(q.event_cursor().unwrap().seq, "6");
    let receipts = q.receipts(0, 32).unwrap();
    assert!(!receipts[0].synthetic);
    assert_eq!(receipts[0].state, State::Unknown);
    assert_eq!(
        q.conn
            .query_row::<String, _, _>("SELECT request FROM jobs WHERE id=?", [&claim.id], |r| r
                .get(0))
            .unwrap(),
        serde_json::to_string(&claim.request).unwrap()
    );
    assert!(payload.contains("sms_received"));
}

#[test]
fn format_two_gains_history_metadata_only_after_owner_validation_and_preserves_old_claims() {
    let (d, mut q) = enabled();
    q.ingest(&binding(), 1, head(6), Some(&message())).unwrap();
    let claim = q.claim(&binding(), 1, LineEpoch(2)).unwrap().unwrap();
    let payload = serde_json::to_string(&claim.request).unwrap();
    q.conn
        .execute_batch("DROP TABLE history_batches; PRAGMA user_version=2;")
        .unwrap();
    drop(q);
    let mut wrong = binding();
    wrong.owner_generation += 1;
    assert!(Queue::open(d.path(), wrong).is_err());
    let db = Connection::open(d.path().join("outbox.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        2
    );
    drop(db);
    let q = Queue::open(d.path(), binding()).unwrap();
    assert_eq!(
        q.conn
            .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        3
    );
    assert_eq!(q.receipts(0, 32).unwrap()[0].state, State::Unknown);
    assert_eq!(
        q.conn
            .query_row::<String, _, _>("SELECT request FROM jobs WHERE id=?", [claim.id], |r| r
                .get(0))
            .unwrap(),
        payload
    );
    assert_eq!(q.event_cursor().unwrap().seq, "6");
    assert_eq!(
        q.conn
            .query_row::<i64, _, _>("SELECT count(*) FROM history_batches", [], |r| r.get(0))
            .unwrap(),
        0
    );
}

#[test]
fn synthetic_provider_messages_are_explicit_fixed_content() {
    for destination in [
        Destination::Bark {
            server: "https://example.invalid/".into(),
            key: Secret::new("synthetic-key".into()),
        },
        Destination::Telegram {
            bot_token: Secret::new("1:synthetic".into()),
            chat_id: Secret::new("synthetic-chat".into()),
        },
    ] {
        let request = prepare_test(
            &Target {
                id: TARGET.into(),
                destination,
            },
            &id(),
            LineEpoch(2),
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(request.body.expose()).unwrap();
        assert_eq!(
            value.get("text").or_else(|| value.get("body")).unwrap(),
            SYNTHETIC_NOTICE
        );
        assert!(request.acknowledged(200, b"{\"code\":200,\"ok\":true}"));
    }
}
