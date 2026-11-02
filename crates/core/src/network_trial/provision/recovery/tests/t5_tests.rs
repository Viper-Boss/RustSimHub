// SPDX-License-Identifier: AGPL-3.0-only
//! Serialized real dispatcher results, original actor and real Auth/SQLite.
use super::super::t5;
use super::*;

fn json(value: impl serde::Serialize) -> Value {
    let v = serde_json::to_value(value).unwrap();
    // Public synthetic evidence consumed by the separate unchanged JS contract
    // validator. Ordinary tests capture output; never print bodies/credentials.
    println!("T5_CORE_RESULT {}", serde_json::to_string(&v).unwrap());
    v
}
async fn execute(
    f: &Fixture,
    c: &RecoveryConnection,
    n: u8,
    action: RecoveryAction,
    body: &[u8],
    probe: Arc<Probe>,
) -> Value {
    let request = request(f, c, n, action, body);
    json(
        t5::execute(f.auth.clone(), request, move |trial| {
            *probe.trial.lock().unwrap() = Arc::downgrade(&trial);
            Mock(probe)
        })
        .await
        .unwrap(),
    )
}
async fn query(f: &Fixture, c: &RecoveryConnection, n: u8, target: u8) -> Value {
    let body = format!("{{\"target_request_id\":\"{}\"}}", id(target));
    execute(
        f,
        c,
        n,
        RecoveryAction::QueryOperation,
        body.as_bytes(),
        Arc::new(Probe::new()),
    )
    .await
}
async fn control(
    f: &Fixture,
    c: &RecoveryConnection,
    n: u8,
    action: RecoveryAction,
    trial: &str,
) -> Value {
    execute(
        f,
        c,
        n,
        action,
        format!("{{\"trial_id\":\"{trial}\"}}").as_bytes(),
        Arc::new(Probe::new()),
    )
    .await
}
fn unknown(op: &Value) {
    assert_eq!(op["state"], "unknown");
    assert_eq!(op["authoritative"], false);
    for key in ["settings_saved", "rollback_verified", "expires_in_ms"] {
        assert_eq!(op[key], Value::Null);
    }
}
fn receipt(f: &Fixture, c: &RecoveryConnection, n: u8, target: u8) -> RecoveryExecutionReceipt {
    let body = format!("{{\"target_request_id\":\"{}\"}}", id(target));
    let r = request(f, c, n, RecoveryAction::QueryOperation, body.as_bytes());
    let a = f.auth.lock().unwrap();
    let DecodedRecoveryRequest::Lookup(q) = recovery_body::decode_v1(&a, r).unwrap() else {
        panic!("query")
    };
    q.lookup(&a).unwrap().unwrap()
}

#[tokio::test]
async fn t5_original_begin_ready_confirm_and_persisted_lookup_match_frozen_fields() {
    let _serial = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let p = Arc::new(Probe::new());
    let begin = execute(&f, &c, 1, RecoveryAction::BeginWifi, BODY, p.clone()).await;
    let trial = begin["trial_id"].as_str().unwrap().to_owned();
    assert_eq!(begin["state"], "starting");
    assert_eq!(begin["settings_saved"], false);
    assert!(begin["expires_in_ms"].as_u64().unwrap() <= 40000);
    drop(ready_state().await);
    let ready = query(&f, &c, 3, 1).await;
    assert_eq!(ready["lookup"], "found");
    assert_eq!(ready["target_request_id"], id(1));
    assert_eq!(ready["operation"]["state"], "ready");
    assert_eq!(ready["operation"]["authoritative"], true);
    assert_eq!(ready["operation"]["settings_saved"], false);
    assert_eq!(ready["operation"]["rollback_verified"], Value::Null);
    assert!(
        ready["operation"]["expires_in_ms"].as_u64().unwrap()
            <= begin["expires_in_ms"].as_u64().unwrap()
    );
    let saved = control(&f, &c, 2, RecoveryAction::ConfirmWifi, &trial).await;
    drained().await;
    assert_eq!(saved["request_id"], id(2));
    assert_eq!(saved["action"], "confirm_wifi");
    assert_eq!(saved["state"], "saved");
    assert_eq!(saved["authoritative"], true);
    assert_eq!(saved["settings_saved"], true);
    assert_eq!(saved["rollback_verified"], Value::Null);
    assert_eq!(saved["expires_in_ms"], 0);
    for (n, target) in [(4, 1), (5, 2)] {
        let q = query(&f, &c, n, target).await;
        assert_eq!(q["operation"]["state"], "saved");
        assert_eq!(q["operation"]["request_id"], id(target));
    }
    assert_eq!(&*p.effects.lock().unwrap(), &["temporary", "persist"]);
    let frozen: Value = serde_json::from_str(include_str!(
        "../../../../../../../docs/team/interfaces/t5-offline-v1.json"
    ))
    .unwrap();
    for (actual, fields) in [(&saved, "operation_fields"), (&ready, "lookup_fields")] {
        let mut actual = actual
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let mut expected = frozen["recovery"][fields]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
    }
    let text = serde_json::to_string(&[begin, ready, saved]).unwrap();
    for secret in [
        "SYNTHETIC_NATIVE",
        "PASSWORD",
        "passphrase",
        "ssid",
        "generation",
        "realm",
        "fingerprint",
    ] {
        assert!(!text.contains(secret));
    }
}
#[tokio::test]
async fn t5_cancel_reports_only_original_verified_compensation() {
    let _serial = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let p = Arc::new(Probe::new());
    let trial = started(start(&f, &c, 1, p.clone()).await.unwrap());
    drop(ready_state().await);
    let rolled = control(&f, &c, 2, RecoveryAction::CancelWifi, &trial).await;
    drained().await;
    assert_eq!(rolled["action"], "cancel_wifi");
    assert_eq!(rolled["state"], "rolled_back");
    assert_eq!(rolled["authoritative"], true);
    assert_eq!(rolled["settings_saved"], false);
    assert_eq!(rolled["rollback_verified"], true);
    assert_eq!(
        query(&f, &c, 3, 1).await["operation"]["state"],
        "rolled_back"
    );
    assert_eq!(&*p.effects.lock().unwrap(), &["temporary", "restore"]);
}
#[tokio::test]
async fn t5_partial_save_and_failed_compensation_keep_unknown_evidence() {
    let _serial = SERIAL.lock().await;
    for fail_rollback in [false, true] {
        let f = fixture(40000);
        let c = connect(&f);
        let p = Arc::new(Probe {
            fail_commit: true,
            fail_rollback,
            ..Probe::new()
        });
        let trial = started(start(&f, &c, 1, p).await.unwrap());
        drop(ready_state().await);
        let result = control(&f, &c, 2, RecoveryAction::ConfirmWifi, &trial).await;
        drained().await;
        let q = query(&f, &c, 3, 2).await;
        if fail_rollback {
            unknown(&result);
            unknown(&q["operation"]);
        } else {
            assert_eq!(result["state"], "rolled_back");
            assert_eq!(q["operation"]["rollback_verified"], true);
        }
    }
}
#[tokio::test]
async fn t5_reserved_claimed_and_orphan_ready_never_invent_a_live_trial() {
    let _serial = SERIAL.lock().await;
    *ACTIVE.lock().await = None;
    for stage in 0..3 {
        let f = fixture(40000);
        let c = connect(&f);
        let w = write(&f, &c, 1, RecoveryAction::BeginWifi, None);
        {
            let a = f.auth.lock().unwrap();
            let CoreRecoveryAdmission::New(p) = w.reserve(&a).unwrap() else {
                panic!("new")
            };
            if stage > 0 {
                let claim = p.claim(&a, Some("a".repeat(64))).unwrap();
                if stage == 2 {
                    let t = claim.begin(&a).unwrap();
                    for progress in [Progress::Starting, Progress::Connecting, Progress::Ready] {
                        a.record_recovery_trial_progress_from_trusted_core(t.authority(), progress)
                            .unwrap();
                    }
                }
            }
        }
        let q = query(&f, &c, 3, 1).await;
        assert_eq!(q["lookup"], "found");
        unknown(&q["operation"]);
        let not_found = query(&f, &c, 4, 2).await;
        assert_eq!(not_found["lookup"], "not_found");
        assert_eq!(not_found["operation"], Value::Null);
        assert!(row(&f, 2).is_none());
        // Query never consumes the reservation or starts the original actor.
        assert_eq!(
            row(&f, 1).unwrap().0,
            ["reserved", "claimed", "ready"][stage]
        );
        assert_eq!(TRIALS.available_permits(), 1);
    }
}

fn reopen(f: Fixture) -> Fixture {
    let Fixture {
        auth,
        _store,
        d,
        _i,
        owner,
        grant,
        plan,
    } = f;
    drop(auth);
    drop(_store);
    let store = Store::open_recovery_execution(d.path()).unwrap();
    let mut a = Auth::open(_i.path(), store.handle()).unwrap();
    a.set_recovery_verifier(Some(Arc::new(Adapter(plan.clone()))))
        .unwrap();
    Fixture {
        auth: Arc::new(Mutex::new(a)),
        _store: store,
        d,
        _i,
        owner,
        grant,
        plan,
    }
}
#[tokio::test]
async fn t5_reopened_saved_history_and_abandoned_claim_preserve_original_ids() {
    let _serial = SERIAL.lock().await;
    for save in [false, true] {
        let f = fixture(40000);
        let c = connect(&f);
        if save {
            let trial = started(start(&f, &c, 1, Arc::new(Probe::new())).await.unwrap());
            drop(ready_state().await);
            control_recovery(
                f.auth.clone(),
                write(&f, &c, 2, RecoveryAction::ConfirmWifi, Some(&trial)),
            )
            .await
            .unwrap();
            drained().await;
        } else {
            let w = write(&f, &c, 1, RecoveryAction::BeginWifi, None);
            let a = f.auth.lock().unwrap();
            let CoreRecoveryAdmission::New(p) = w.reserve(&a).unwrap() else {
                panic!("new")
            };
            drop(p.claim(&a, Some("e".repeat(64))).unwrap());
        }
        drop(c);
        *ACTIVE.lock().await = None;
        let f = reopen(f);
        let c = connect(&f);
        let q = query(&f, &c, 3, 1).await;
        assert_eq!(q["operation"]["request_id"], id(1));
        if save {
            assert_eq!(q["operation"]["state"], "saved");
            assert_eq!(q["operation"]["settings_saved"], true);
            assert_eq!(q["operation"]["expires_in_ms"], 0);
        } else {
            unknown(&q["operation"]);
            assert_eq!(q["operation"]["trial_id"], "e".repeat(64));
            assert_eq!(row(&f, 1).unwrap().0, "unknown");
        }
        assert_eq!(TRIALS.available_permits(), 1);
    }
}
#[tokio::test]
async fn t5_reconnection_cannot_extend_or_authorize_original_saving_actor() {
    let _serial = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let p = Arc::new(Probe {
        hold_commit: true,
        ..Probe::new()
    });
    let trial = started(start(&f, &c, 1, p.clone()).await.unwrap());
    drop(ready_state().await);
    let w = write(&f, &c, 2, RecoveryAction::ConfirmWifi, Some(&trial));
    let a = f.auth.clone();
    let task = tokio::spawn(async move { control_recovery(a, w).await });
    p.commit_entered.notified().await;
    let before = query(&f, &c, 3, 2).await;
    assert_eq!(before["operation"]["state"], "saving");
    assert_eq!(before["operation"]["settings_saved"], Value::Null);
    assert_eq!(before["operation"]["rollback_verified"], Value::Null);
    c.invalidate();
    let fresh = connect(&f);
    let after = query(&f, &fresh, 4, 1).await;
    unknown(&after["operation"]);
    assert_eq!(row(&f, 1).unwrap().0, "saving");
    assert_eq!(TRIALS.available_permits(), 0);
    p.release_commit.notify_one();
    task.await.unwrap().unwrap();
    drained().await;
    assert_eq!(
        query(&f, &fresh, 5, 2).await["operation"]["state"],
        "rolled_back"
    );
}
#[tokio::test]
async fn t5_live_deadline_and_normal_owner_metadata_cannot_resurrect_ready() {
    let _serial = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let p = Arc::new(Probe::new());
    let trial = started(start(&f, &c, 1, p).await.unwrap());
    drop(ready_state().await);
    let r = receipt(&f, &c, 3, 1);
    let old_deadline = {
        let mut a = ACTIVE.lock().await;
        let s = a.as_mut().unwrap();
        let old = s.deadline;
        s.deadline = Instant::now();
        old
    };
    unknown(&json(t5::from_receipt(f.auth.clone(), r.clone()).await));
    {
        let mut a = ACTIVE.lock().await;
        let s = a.as_mut().unwrap();
        s.deadline = old_deadline;
        let owner = std::mem::replace(&mut s.owner, TrialOwner::RecoveryClosed);
        s.owner = TrialOwner::Normal {
            principal: f
                .auth
                .lock()
                .unwrap()
                .network_manager(f.owner.expose())
                .unwrap(),
            fingerprint: [0; 32],
        };
        drop(a);
        unknown(&json(t5::from_receipt(f.auth.clone(), r).await));
        ACTIVE.lock().await.as_mut().unwrap().owner = owner;
    }
    control_recovery(
        f.auth.clone(),
        write(&f, &c, 2, RecoveryAction::CancelWifi, Some(&trial)),
    )
    .await
    .unwrap();
    drained().await;
}
#[tokio::test]
async fn t5_full_binding_rejects_same_trial_and_request_strings_from_another_store() {
    let _serial = SERIAL.lock().await;
    *ACTIVE.lock().await = None;
    let first = fixture(40000);
    let second = fixture(40000);
    let c1 = connect(&first);
    let c2 = connect(&second);
    fn claimed(f: &Fixture, c: &RecoveryConnection) -> CoreRecoveryTrial {
        let w = write(f, c, 1, RecoveryAction::BeginWifi, None);
        let a = f.auth.lock().unwrap();
        let CoreRecoveryAdmission::New(p) = w.reserve(&a).unwrap() else {
            panic!("new")
        };
        p.claim(&a, Some("b".repeat(64)))
            .unwrap()
            .begin(&a)
            .unwrap()
    }
    let t1 = claimed(&first, &c1);
    let t2 = claimed(&second, &c2);
    let r1 = receipt(&first, &c1, 3, 1);
    let r2 = receipt(&second, &c2, 3, 1);
    assert_eq!(r1.trial_id(), r2.trial_id());
    assert_eq!(r1.request_id(), r2.request_id());
    assert!(t1.authority().matches_receipt(&r1));
    assert!(r1.same_trial_as(&r1));
    assert!(!r1.same_trial_as(&r2));
    assert!(!t1.authority().matches_receipt(&r2));
    assert!(!t2.authority().matches_receipt(&r1));
}
#[tokio::test]
async fn t5_unknown_history_never_upgrades_from_the_live_actor() {
    let _serial = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    start(&f, &c, 1, Arc::new(Probe::new())).await.unwrap();
    let t = ready_state().await;
    t.authority()
        .finish_from_trusted_core(None, Completion::Unknown)
        .unwrap();
    let q = query(&f, &c, 3, 1).await;
    unknown(&q["operation"]);
    assert_eq!(drained().await, State::Unknown);
    assert_eq!(row(&f, 1).unwrap().0, "unknown");
}
#[test]
fn t5_real_blocking_worker_queue_obeys_three_second_read_deadline_without_writes() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    rt.block_on(async {
        let _serial = SERIAL.lock().await;
        let f = fixture(40000);
        let c = connect(&f);
        let request = request(
            &f,
            &c,
            3,
            RecoveryAction::QueryOperation,
            format!("{{\"target_request_id\":\"{}\"}}", id(1)).as_bytes(),
        );
        let (release, held) = std::sync::mpsc::channel();
        let entered = Arc::new(Notify::new());
        let ready = entered.clone();
        let blocker = tokio::task::spawn_blocking(move || {
            ready.notify_one();
            held.recv().unwrap();
        });
        entered.notified().await;
        let started = Instant::now();
        let result = t5::execute(f.auth.clone(), request, |_| -> Mock {
            panic!("query must never run backend factory")
        })
        .await;
        assert_eq!(
            json(result.err().unwrap()),
            serde_json::json!({"contract":1,"code":"deadline"})
        );
        assert!(started.elapsed() >= Duration::from_secs(3));
        assert!(started.elapsed() < Duration::from_secs(5));
        release.send(()).unwrap();
        blocker.await.unwrap();
        // Wait behind the late readonly decode on the same actual worker pool.
        tokio::task::spawn_blocking(|| {}).await.unwrap();
        assert!(row(&f, 1).is_none());
        assert!(row(&f, 3).is_none());
    });
}
#[tokio::test]
async fn t5_private_capability_body_rejection_and_default_disabled_errors_are_finite() {
    let _serial = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let r = request(&f, &c, 1, RecoveryAction::BeginWifi, BODY);
    let disabled = recovery_v1(f.auth.clone(), r).await.err().unwrap();
    assert_eq!(
        json(disabled),
        serde_json::json!({"contract":1,"code":"permission_denied"})
    );
    let mut r = request(&f, &c, 2, RecoveryAction::BeginWifi, BODY);
    r.body.push(b' ');
    let rejected = t5::execute(f.auth.clone(), r, |_| -> Mock { panic!("tampered body") })
        .await
        .err()
        .unwrap();
    assert_eq!(
        json(rejected),
        serde_json::json!({"contract":1,"code":"permission_denied"})
    );
    assert!(row(&f, 1).is_none());
    assert!(row(&f, 2).is_none());
    for (error, code) in [
        (Error::Busy, "busy"),
        (Error::InvalidInput, "invalid_input"),
        (Error::Expired, "deadline"),
        (Error::Replay, "permission_denied"),
        (Error::Io, "unknown"),
        (Error::Storage, "unknown"),
        (Error::Entropy, "unavailable"),
        (Error::CorruptIdentity, "unavailable"),
    ] {
        assert_eq!(
            json(t5::RecoveryErrorV1::from(error)),
            serde_json::json!({"contract":1,"code":code})
        );
    }
}
