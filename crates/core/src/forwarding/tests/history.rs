// SPDX-License-Identifier: AGPL-3.0-only
use super::*;

const SECOND: &str = "55555555-5555-4555-8555-555555555555";
fn confirmation(f: &Fixture, id: &str) -> String {
    let db = rusqlite::Connection::open(f.state.path().join("forwarding/outbox.sqlite3")).unwrap();
    let plan: String = db
        .query_row("SELECT plan FROM history_batches WHERE id=?", [id], |r| {
            r.get(0)
        })
        .unwrap();
    serde_json::from_str::<Value>(&plan).unwrap()["preview"]["confirmation_id"]
        .as_str()
        .unwrap()
        .to_owned()
}
fn prepare(f: &Fixture, id: &str, generation: u64) -> AuthCommand {
    AuthCommand::ForwardingHistoryPrepare {
        token: f.token.clone(),
        expected_generation: generation,
        request_id: id.into(),
        range: json!({"after":null,"limit":32,"targets":[TARGET]}),
    }
}
fn commit(f: &Fixture, id: &str, generation: u64, full: bool) -> AuthCommand {
    AuthCommand::ForwardingHistoryCommit {
        token: f.token.clone(),
        expected_generation: generation,
        request_id: id.into(),
        confirmation_id: confirmation(f, id),
        accept_full_text: full,
    }
}
fn query(f: &Fixture, id: &str) -> AuthCommand {
    AuthCommand::ForwardingHistoryRequest {
        token: f.token.clone(),
        request_id: id.into(),
    }
}
#[tokio::test]
async fn historical_commands_reject_a_real_delegated_principal_even_with_management_scope() {
    let f = Fixture::new();
    f.incoming("owner-only-history");
    f.request(f.configure(0, REQUEST, true)).await;
    f.request(prepare(&f, CHANGE, 1)).await;
    let token = delegated(&f);
    for mut command in [
        prepare(&f, SECOND, 1),
        commit(&f, CHANGE, 1, false),
        query(&f, CHANGE),
        AuthCommand::ForwardingHistoryCancel {
            token: f.token.clone(),
            request_id: CHANGE.into(),
            confirmation_id: confirmation(&f, CHANGE),
        },
        AuthCommand::ForwardingHistoryArchive {
            token: f.token.clone(),
            expected_generation: 1,
            request_id: CHANGE.into(),
            confirmation_id: confirmation(&f, CHANGE),
        },
    ] {
        match &mut command {
            AuthCommand::ForwardingHistoryPrepare { token: t, .. }
            | AuthCommand::ForwardingHistoryCommit { token: t, .. }
            | AuthCommand::ForwardingHistoryRequest { token: t, .. }
            | AuthCommand::ForwardingHistoryCancel { token: t, .. } => *t = token.clone(),
            AuthCommand::ForwardingHistoryArchive { token: t, .. } => *t = token.clone(),
            _ => unreachable!(),
        }
        assert_eq!(f.request(command).await["error"], "denied");
    }
    assert_eq!(
        f.request(query(&f, CHANGE)).await["receipt"]["state"],
        "prepared"
    );
    assert_eq!(f.request(query(&f, SECOND)).await["known"], false);
}
#[tokio::test]
async fn history_owner_ipc_previews_without_queueing_and_commits_once_across_lost_reply_and_restart(
) {
    let f = Fixture::new();
    f.incoming("history-a");
    f.incoming("history-b");
    assert_eq!(
        f.request(f.configure(0, REQUEST, true)).await["generation"],
        1
    );
    let before = {
        let s = f.control.0.lock().unwrap();
        s.queue.as_ref().unwrap().event_cursor().unwrap()
    };
    let preview = f.request(prepare(&f, CHANGE, 1)).await;
    assert_eq!(preview["state"], "prepared");
    assert_eq!(preview["preview"]["scanned"], 2);
    assert_eq!(
        preview["preview"]["deliveries"].as_array().unwrap().len(),
        2
    );
    for secret in [
        "synthetic private body",
        "synthetic sender",
        "path-secret",
        "synthetic-bearer-secret",
    ] {
        assert!(!preview.to_string().contains(secret));
    }
    assert_eq!(
        f.request(AuthCommand::ForwardingStatus {
            token: f.token.clone()
        })
        .await["queued"],
        0
    );
    let committed = f.request(commit(&f, CHANGE, 1, false)).await;
    assert_eq!(committed["state"], "submitted");
    assert_eq!(committed["receipts"].as_array().unwrap().len(), 2);
    assert_eq!(f.request(commit(&f, CHANGE, 1, false)).await, committed);
    let binding = Binding::from_owner(&mut f.auth.lock().unwrap(), &f.token).unwrap();
    let claim = {
        let mut s = f.control.0.lock().unwrap();
        s.queue
            .as_mut()
            .unwrap()
            .claim_current(&f.auth.lock().unwrap(), &f.store.handle(), 1)
            .unwrap()
            .unwrap()
    };
    assert!(claim.request.body.expose().contains("sms_received"));
    let old_confirmation = confirmation(&f, CHANGE);
    {
        let db =
            rusqlite::Connection::open(f.state.path().join("forwarding/outbox.sqlite3")).unwrap();
        let raw: String = db
            .query_row(
                "SELECT plan FROM history_batches WHERE id=?",
                [CHANGE],
                |r| r.get(0),
            )
            .unwrap();
        let mut plan: Value = serde_json::from_str(&raw).unwrap();
        plan["preview"]
            .as_object_mut()
            .unwrap()
            .remove("confirmation_id");
        db.execute(
            "UPDATE history_batches SET plan=? WHERE id=?",
            rusqlite::params![plan.to_string(), CHANGE],
        )
        .unwrap();
    }
    f.control.0.lock().unwrap().queue = None;
    let recovered = f.request(query(&f, CHANGE)).await;
    assert_eq!(recovered["receipt"]["state"], "submitted");
    assert_eq!(recovered["receipt"]["receipts"][0]["state"], "unknown");
    assert_ne!(
        recovered["receipt"]["preview"]["confirmation_id"],
        old_confirmation
    );
    let status = f
        .request(AuthCommand::SetForwardingEnabled {
            token: f.token.clone(),
            expected_generation: 1,
            request_id: SECOND.into(),
            enabled: false,
        })
        .await;
    assert_eq!(status["enabled"], false);
    assert_eq!(status["unknown"], 1);
    let retry = f.request(commit(&f, CHANGE, 1, false)).await;
    assert_eq!(retry["state"], "submitted");
    assert_eq!(retry["receipts"][0]["state"], "unknown");
    let mut s = f.control.0.lock().unwrap();
    let q = s.queue.as_mut().unwrap();
    assert!(q
        .finish(&binding, &claim, rustsimhub_forwarder::State::Confirmed)
        .is_err());
    assert_eq!(q.status().unwrap().queued, 0);
    assert_eq!(q.event_cursor().unwrap().seq, before.seq);
}
#[tokio::test]
async fn history_cancel_and_core_restart_never_turn_an_unconfirmed_preview_into_a_job() {
    let f = Fixture::new();
    f.incoming("historical-cancel");
    f.request(f.configure(0, REQUEST, true)).await;
    assert_eq!(f.request(prepare(&f, CHANGE, 1)).await["state"], "prepared");
    let cancelled = f
        .request(AuthCommand::ForwardingHistoryCancel {
            token: f.token.clone(),
            request_id: CHANGE.into(),
            confirmation_id: confirmation(&f, CHANGE),
        })
        .await;
    assert_eq!(cancelled["state"], "cancelled");
    assert_eq!(
        f.request(commit(&f, CHANGE, 1, false)).await["error"],
        "denied"
    );
    assert_eq!(f.request(prepare(&f, SECOND, 1)).await["state"], "prepared");
    let db = rusqlite::Connection::open(f.state.path().join("forwarding/outbox.sqlite3")).unwrap();
    let plan: String = db
        .query_row(
            "SELECT plan FROM history_batches WHERE id=?",
            [SECOND],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!plan.contains("synthetic private body"));
    assert!(!plan.contains("bearer-secret"));
    drop(db);
    f.control.0.lock().unwrap().queue = None;
    assert_eq!(
        f.request(query(&f, SECOND)).await["receipt"]["state"],
        "expired"
    );
    assert_eq!(
        f.request(commit(&f, SECOND, 1, false)).await["error"],
        "denied"
    );
    assert_eq!(
        f.request(AuthCommand::ForwardingStatus {
            token: f.token.clone()
        })
        .await["queued"],
        0
    );
}
#[tokio::test]
async fn history_full_text_needs_explicit_consent_and_trash_or_sim_changes_fence_commit() {
    let f = Fixture::new();
    f.incoming("historical-full");
    let mut p = policy(true);
    p["rules"][0]["share"] = "full_text".into();
    f.request(AuthCommand::ForwardingConfigure {
        token: f.token.clone(),
        expected_generation: 0,
        request_id: REQUEST.into(),
        policy: p,
    })
    .await;
    let preview = f.request(prepare(&f, CHANGE, 1)).await;
    assert_eq!(preview["preview"]["deliveries"][0]["share"], "full_text");
    assert_eq!(
        f.request(commit(&f, CHANGE, 1, false)).await["error"],
        "denied"
    );
    let id = preview["preview"]["deliveries"][0]["message_id"]
        .as_str()
        .unwrap()
        .to_owned();
    f.auth
        .lock()
        .unwrap()
        .set_sms_deleted(&f.token, id.clone(), true)
        .unwrap();
    assert_eq!(
        f.request(commit(&f, CHANGE, 1, true)).await["error"],
        "denied"
    );
    f.auth
        .lock()
        .unwrap()
        .set_sms_deleted(&f.token, id, false)
        .unwrap();
    assert_eq!(f.request(prepare(&f, SECOND, 1)).await["state"], "prepared");
    f.store
        .handle()
        .observe_line(LineObservation {
            backend: "synthetic modem".into(),
            fingerprint: Some([20; 32]),
            status: LineStatus::Ready,
        })
        .unwrap();
    assert_eq!(
        f.request(commit(&f, SECOND, 1, true)).await["error"],
        "denied"
    );
    assert_eq!(
        f.request(AuthCommand::ForwardingStatus {
            token: f.token.clone()
        })
        .await["queued"],
        0
    );
}
#[tokio::test]
async fn history_off_preview_never_enables_service_and_invalid_ranges_cannot_be_saved() {
    let f = Fixture::new();
    f.incoming("historical-off");
    f.request(f.configure(0, REQUEST, false)).await;
    assert_eq!(f.request(prepare(&f, CHANGE, 1)).await["state"], "prepared");
    assert_eq!(
        f.request(commit(&f, CHANGE, 1, false)).await["error"],
        "denied"
    );
    let mut command = prepare(&f, SECOND, 1);
    if let AuthCommand::ForwardingHistoryPrepare { ref mut range, .. } = command {
        range["limit"] = 33.into();
    }
    assert_eq!(f.request(command).await["error"], "denied");
    let mut command = prepare(&f, SECOND, 1);
    if let AuthCommand::ForwardingHistoryPrepare { ref mut range, .. } = command {
        range["targets"] = json!([TARGET, TARGET]);
    }
    assert_eq!(f.request(command).await["error"], "denied");
    assert_eq!(f.request(query(&f, SECOND)).await["known"], false);
    let status = f
        .request(AuthCommand::ForwardingStatus {
            token: f.token.clone(),
        })
        .await;
    assert_eq!(status["enabled"], false);
    assert_eq!(status["queued"], 0);
}

#[tokio::test]
async fn history_queue_write_failure_rolls_back_the_entire_batch_and_keeps_the_confirmation_key() {
    let f = Fixture::new();
    f.incoming("atomic-a");
    f.incoming("atomic-b");
    f.request(f.configure(0, REQUEST, true)).await;
    assert_eq!(
        f.request(prepare(&f, CHANGE, 1)).await["preview"]["deliveries"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let db = rusqlite::Connection::open(f.state.path().join("forwarding/outbox.sqlite3")).unwrap();
    db.execute_batch("CREATE TRIGGER synthetic_history_write_failure BEFORE INSERT ON jobs WHEN (SELECT count(*) FROM jobs)>0 BEGIN SELECT RAISE(ABORT,'synthetic write failure'); END;").unwrap();
    assert_eq!(
        f.request(commit(&f, CHANGE, 1, false)).await["error"],
        "denied"
    );
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT count(*) FROM jobs", [], |r| r.get(0))
            .unwrap(),
        0
    );
    assert_eq!(
        f.request(query(&f, CHANGE)).await["receipt"]["state"],
        "prepared"
    );
    db.execute_batch("DROP TRIGGER synthetic_history_write_failure;")
        .unwrap();
    assert_eq!(
        f.request(commit(&f, CHANGE, 1, false)).await["receipts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT count(*) FROM jobs", [], |r| r.get(0))
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn history_does_not_recreate_unknown_deliveries_after_a_new_configuration_generation() {
    let f = Fixture::new();
    f.incoming("old-unknown");
    f.request(f.configure(0, REQUEST, true)).await;
    f.request(prepare(&f, CHANGE, 1)).await;
    assert_eq!(
        f.request(commit(&f, CHANGE, 1, false)).await["state"],
        "submitted"
    );
    {
        let mut s = f.control.0.lock().unwrap();
        assert!(s
            .queue
            .as_mut()
            .unwrap()
            .claim_current(&f.auth.lock().unwrap(), &f.store.handle(), 1)
            .unwrap()
            .is_some());
    }
    f.request(AuthCommand::SetForwardingEnabled {
        token: f.token.clone(),
        expected_generation: 1,
        request_id: SECOND.into(),
        enabled: false,
    })
    .await;
    const THIRD: &str = "66666666-6666-4666-8666-666666666666";
    f.request(AuthCommand::SetForwardingEnabled {
        token: f.token.clone(),
        expected_generation: 2,
        request_id: THIRD.into(),
        enabled: true,
    })
    .await;
    let preview = f.request(prepare(&f, THIRD, 3)).await;
    assert_eq!(preview["state"], "prepared");
    assert_eq!(preview["preview"]["already_queued_or_recorded"], 1);
    assert!(preview["preview"]["deliveries"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        f.request(commit(&f, THIRD, 3, false)).await["error"],
        "denied"
    );
    let status = f
        .request(AuthCommand::ForwardingStatus {
            token: f.token.clone(),
        })
        .await;
    assert_eq!(status["queued"], 0);
    assert_eq!(status["unknown"], 1);
    assert_eq!(
        f.request(AuthCommand::ForwardingHistoryArchive {
            token: f.token.clone(),
            expected_generation: 3,
            request_id: CHANGE.into(),
            confirmation_id: confirmation(&f, CHANGE)
        })
        .await["error"],
        "denied"
    );
}

#[tokio::test]
async fn archived_preview_challenges_fence_late_commit_and_cancel_even_if_the_client_reuses_its_uuid(
) {
    let f = Fixture::new();
    f.incoming("archive-challenge");
    f.request(f.configure(0, REQUEST, true)).await;
    f.request(prepare(&f, CHANGE, 1)).await;
    let old = commit(&f, CHANGE, 1, false);
    let old_confirmation = confirmation(&f, CHANGE);
    assert_eq!(
        f.request(AuthCommand::ForwardingHistoryCancel {
            token: f.token.clone(),
            request_id: CHANGE.into(),
            confirmation_id: old_confirmation.clone()
        })
        .await["state"],
        "cancelled"
    );
    assert_eq!(
        f.request(AuthCommand::ForwardingHistoryArchive {
            token: f.token.clone(),
            expected_generation: 1,
            request_id: CHANGE.into(),
            confirmation_id: old_confirmation.clone()
        })
        .await["archived"],
        true
    );
    assert_eq!(f.request(query(&f, CHANGE)).await["known"], false);
    assert_eq!(f.request(prepare(&f, CHANGE, 1)).await["state"], "prepared");
    assert_ne!(confirmation(&f, CHANGE), old_confirmation);
    assert_eq!(f.request(old).await["error"], "denied");
    assert_eq!(
        f.request(AuthCommand::ForwardingHistoryCancel {
            token: f.token.clone(),
            request_id: CHANGE.into(),
            confirmation_id: old_confirmation
        })
        .await["error"],
        "denied"
    );
    let committed = f.request(commit(&f, CHANGE, 1, false)).await;
    assert_eq!(committed["state"], "submitted");
    let archive = || AuthCommand::ForwardingHistoryArchive {
        token: f.token.clone(),
        expected_generation: 1,
        request_id: CHANGE.into(),
        confirmation_id: confirmation(&f, CHANGE),
    };
    assert_eq!(f.request(archive()).await["error"], "denied");
    {
        let mut a = f.auth.lock().unwrap();
        let b = Binding::from_owner(&mut a, &f.token).unwrap();
        let mut s = f.control.0.lock().unwrap();
        let q = s.queue.as_mut().unwrap();
        let claim = q.claim_current(&a, &f.store.handle(), 1).unwrap().unwrap();
        q.finish(&b, &claim, rustsimhub_forwarder::State::Confirmed)
            .unwrap();
    }
    assert_eq!(f.request(archive()).await["jobs_preserved"], true);
    assert_eq!(f.request(query(&f, CHANGE)).await["known"], false);
    assert_eq!(
        f.control
            .0
            .lock()
            .unwrap()
            .queue
            .as_ref()
            .unwrap()
            .receipts(0, 32)
            .unwrap()[0]
            .state,
        rustsimhub_forwarder::State::Confirmed
    );
}
