// SPDX-License-Identifier: AGPL-3.0-only
//! Deterministic polling of the ORIGINAL actor: no wall-clock race or HTTP hook.
use super::*;
use std::{future::Future, sync::atomic::AtomicUsize, task::Poll};

struct ScheduledGate {
    inner: RecoveryContext,
    tick: Notify,
    checks: AtomicUsize,
}
impl Gate for ScheduledGate {
    async fn check(&self) -> Result<()> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        self.inner.check().await
    }
}
impl ProvisionGate for ScheduledGate {
    async fn admit_control(
        &self,
        authority: ControlAuthority,
        confirm: bool,
        ready: bool,
    ) -> Result<Option<ControlAuthority>> {
        self.inner.admit_control(authority, confirm, ready).await
    }
    async fn wait_control_tick(&self) {
        self.tick.notified().await;
    }
    async fn check_control(&self, authority: &ControlAuthority, confirm: bool) -> Result<()> {
        self.inner.check_control(authority, confirm).await
    }
}

#[tokio::test]
async fn legitimate_control_admission_cannot_race_its_own_actor_gate() {
    let _serial = SERIAL.lock().await;
    for scenario in ["confirm", "cancel", "disconnect"] {
        let confirm = scenario != "cancel";
        let f = fixture(40000);
        let c = connect(&f);
        let w = write(&f, &c, 1, RecoveryAction::BeginWifi, None);
        let trial = {
            let a = f.auth.lock().unwrap();
            let CoreRecoveryAdmission::New(p) = w.reserve(&a).unwrap() else {
                panic!("new")
            };
            Arc::new(
                p.claim(&a, Some("a".repeat(64)))
                    .unwrap()
                    .begin(&a)
                    .unwrap(),
            )
        };
        let id = trial.authority().trial_id().to_owned();
        let gate = ScheduledGate {
            inner: RecoveryContext {
                auth: f.auth.clone(),
                trial: trial.clone(),
            },
            tick: Notify::new(),
            checks: AtomicUsize::new(0),
        };
        let state = Arc::new(tokio::sync::Mutex::new(State::Starting));
        let (tx, mut rx) = mpsc::channel(1);
        *ACTIVE.lock().await = Some(Session {
            id: id.clone(),
            key: crate::network_trial::provision::recovery::tests::id(1),
            owner: TrialOwner::Recovery(trial),
            state: state.clone(),
            control: tx,
            deadline: gate.inner.deadline(),
        });
        let probe = Arc::new(Probe::new());
        let mut backend = Mock(probe.clone());
        let credentials = Credentials {
            ssid: "SYNTHETIC_NATIVE".into(),
            passphrase: Zeroizing::new("SYNTHETIC_NATIVE_PASSWORD".into()),
        };
        let mut actor = Box::pin(run_trial(
            &mut backend,
            &gate,
            credentials,
            &id,
            &gate.inner,
            &state,
            &mut rx,
        ));
        timeout(
            Duration::from_secs(5),
            std::future::poll_fn(|cx| {
                assert!(actor.as_mut().poll(cx).is_pending());
                if state.try_lock().is_ok_and(|s| *s == State::Ready) {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            }),
        )
        .await
        .unwrap();
        let w = write(
            &f,
            &c,
            2,
            if confirm {
                RecoveryAction::ConfirmWifi
            } else {
                RecoveryAction::CancelWifi
            },
            Some(&id),
        );
        let entered = Arc::new(Notify::new());
        let notify = entered.clone();
        let (release, held) = std::sync::mpsc::channel();
        let held = Mutex::new(held);
        let schedule = AdmissionSchedule {
            held: Some(Box::new(move || {
                notify.notify_one();
                held.lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
            })),
        };
        let mut control = Box::pin(control_recovery_inner(f.auth.clone(), w, schedule));
        timeout(Duration::from_secs(5), async {
            tokio::select! {
                _=entered.notified()=>(),
                _=&mut actor=>panic!("actor ended before admission"),
                _=&mut control=>panic!("control ended before admission"),
            }
        })
        .await
        .unwrap();
        assert!(f.auth.try_lock().is_err(), "barrier actually owns Auth");
        let before = gate.checks.load(Ordering::SeqCst);
        gate.tick.notify_one(); // Make the old 150ms branch ready, deterministically.
        std::future::poll_fn(|cx| {
            assert!(actor.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        let during = gate.checks.load(Ordering::SeqCst);
        if scenario == "disconnect" {
            c.invalidate();
        }
        release.send(()).unwrap();
        let (_, result) = timeout(Duration::from_secs(5), async {
            tokio::join!(actor, control)
        })
        .await
        .unwrap();
        *ACTIVE.lock().await = None;
        assert_eq!(
            during, before,
            "background Gate ran against its own control's Auth guard"
        );
        if scenario == "disconnect" {
            assert!(result.is_err());
            assert!(row(&f, 2).is_none());
            assert_eq!(row(&f, 1).unwrap().0, "rolled_back");
            assert_eq!(&*probe.effects.lock().unwrap(), &["temporary", "restore"]);
        } else {
            completed(
                result.unwrap(),
                if confirm {
                    RecoveryExecutionState::Saved
                } else {
                    RecoveryExecutionState::RolledBack
                },
            );
            let expected = if confirm { "saved" } else { "rolled_back" };
            for n in [1, 2] {
                assert_eq!(row(&f, n).unwrap().0, expected);
            }
            assert_eq!(
                &*probe.effects.lock().unwrap(),
                if confirm {
                    &["temporary", "persist"][..]
                } else {
                    &["temporary", "restore"][..]
                }
            );
        }
    }
}

struct ManualActor {
    gate: ScheduledGate,
    state: Arc<tokio::sync::Mutex<State>>,
    tx: mpsc::Sender<Control>,
    rx: mpsc::Receiver<Control>,
    id: String,
    probe: Arc<Probe>,
    backend: Mock,
}
async fn manual(f: &Fixture, c: &RecoveryConnection) -> ManualActor {
    let w = write(f, c, 1, RecoveryAction::BeginWifi, None);
    let trial = {
        let a = f.auth.lock().unwrap();
        let CoreRecoveryAdmission::New(p) = w.reserve(&a).unwrap() else {
            panic!("new")
        };
        Arc::new(
            p.claim(&a, Some("b".repeat(64)))
                .unwrap()
                .begin(&a)
                .unwrap(),
        )
    };
    let id = trial.authority().trial_id().to_owned();
    let gate = ScheduledGate {
        inner: RecoveryContext {
            auth: f.auth.clone(),
            trial: trial.clone(),
        },
        tick: Notify::new(),
        checks: AtomicUsize::new(0),
    };
    let state = Arc::new(tokio::sync::Mutex::new(State::Starting));
    let (tx, rx) = mpsc::channel(1);
    *ACTIVE.lock().await = Some(Session {
        id: id.clone(),
        key: super::id(1),
        owner: TrialOwner::Recovery(trial),
        state: state.clone(),
        control: tx.clone(),
        deadline: gate.inner.deadline(),
    });
    let probe = Arc::new(Probe::new());
    ManualActor {
        gate,
        state,
        tx,
        rx,
        id,
        backend: Mock(probe.clone()),
        probe,
    }
}
fn credentials() -> Credentials {
    Credentials {
        ssid: "SYNTHETIC_NATIVE".into(),
        passphrase: Zeroizing::new("SYNTHETIC_NATIVE_PASSWORD".into()),
    }
}
async fn poll_pending(f: std::pin::Pin<&mut impl Future>) {
    let mut f = f;
    std::future::poll_fn(|cx| {
        assert!(f.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}
async fn ready_local(f: std::pin::Pin<&mut impl Future>, state: &tokio::sync::Mutex<State>) {
    let mut f = f;
    timeout(
        Duration::from_secs(5),
        std::future::poll_fn(|cx| {
            assert!(f.as_mut().poll(cx).is_pending());
            if state.try_lock().is_ok_and(|s| *s == State::Ready) {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        }),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn raw_control_reserves_capacity_before_decode_or_claim_and_queries_do_not_resubmit() {
    let _serial = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let mut m = manual(&f, &c).await;
    let mut actor = Box::pin(run_trial(
        &mut m.backend,
        &m.gate,
        credentials(),
        &m.id,
        &m.gate.inner,
        &m.state,
        &mut m.rx,
    ));
    ready_local(actor.as_mut(), &m.state).await;
    let body = format!("{{\"trial_id\":\"{}\"}}", m.id);
    let req = request(&f, &c, 2, RecoveryAction::ConfirmWifi, body.as_bytes());
    let second = request(&f, &c, 3, RecoveryAction::CancelWifi, body.as_bytes());
    let mut control = Box::pin(control_request(f.auth.clone(), req));
    poll_pending(control.as_mut()).await;
    assert_eq!(m.tx.capacity(), 0);
    assert!(row(&f, 2).is_none());
    assert!(matches!(
        control_request(f.auth.clone(), second).await,
        Err(Error::Busy)
    ));
    assert!(row(&f, 3).is_none());
    let (_, outcome) = timeout(Duration::from_secs(5), async {
        tokio::join!(actor, control)
    })
    .await
    .unwrap();
    completed(outcome.unwrap(), RecoveryExecutionState::Saved);
    for n in [1, 2] {
        assert_eq!(row(&f, n).unwrap().0, "saved");
    }
    for n in [4, 5] {
        let body = format!("{{\"target_request_id\":\"{}\"}}", super::id(2));
        let req = request(&f, &c, n, RecoveryAction::QueryOperation, body.as_bytes());
        let value = t5::execute(f.auth.clone(), req, |_| -> Mock {
            panic!("query cannot start backend")
        })
        .await;
        // Supply a concrete backend type without ever constructing it.
        let json = serde_json::to_value(value.unwrap()).unwrap();
        assert_eq!(json["operation"]["state"], "saved");
    }
    let old = request(&f, &c, 6, RecoveryAction::ConfirmWifi, body.as_bytes());
    assert!(matches!(
        control_request(f.auth.clone(), old).await,
        Err(Error::Forbidden)
    ));
    assert_eq!(&*m.probe.effects.lock().unwrap(), &["temporary", "persist"]);
    *f.plan.lock().unwrap() = (RecoveryAction::ConfirmWifi, body.as_bytes().to_vec());
    let duplicate = f
        .auth
        .lock()
        .unwrap()
        .prepare_recovery_verification(&c, f.grant.clone(), &[2; 16])
        .unwrap()
        .verify();
    assert_eq!(
        f.auth
            .lock()
            .unwrap()
            .finish_recovery_verification(&c, duplicate)
            .err(),
        Some(Error::Replay)
    );
    assert!(row(&f, 6).is_none());
    *ACTIVE.lock().await = None;
}

#[tokio::test]
async fn queued_disconnect_or_revoke_is_rechecked_before_any_control_claim() {
    let _serial = SERIAL.lock().await;
    for revoke in [false, true] {
        let f = fixture(40000);
        let c = connect(&f);
        let mut m = manual(&f, &c).await;
        let mut actor = Box::pin(run_trial(
            &mut m.backend,
            &m.gate,
            credentials(),
            &m.id,
            &m.gate.inner,
            &m.state,
            &mut m.rx,
        ));
        ready_local(actor.as_mut(), &m.state).await;
        let req = request(
            &f,
            &c,
            2,
            RecoveryAction::ConfirmWifi,
            format!("{{\"trial_id\":\"{}\"}}", m.id).as_bytes(),
        );
        let mut control = Box::pin(control_request(f.auth.clone(), req));
        poll_pending(control.as_mut()).await;
        assert!(row(&f, 2).is_none());
        if revoke {
            f.auth
                .lock()
                .unwrap()
                .revoke_recovery_key(
                    f.owner.expose(),
                    f.grant.independent_public_key,
                    f.grant.generation,
                    super::id(91),
                )
                .unwrap();
        } else {
            c.invalidate();
        }
        let (_, result) = timeout(Duration::from_secs(5), async {
            tokio::join!(actor, control)
        })
        .await
        .unwrap();
        assert!(result.is_err());
        assert!(row(&f, 2).is_none());
        assert_eq!(row(&f, 1).unwrap().0, "rolled_back");
        assert_eq!(&*m.probe.effects.lock().unwrap(), &["temporary", "restore"]);
        *ACTIVE.lock().await = None;
    }
}

#[tokio::test]
async fn rejected_pending_body_or_binding_does_not_claim_or_destroy_the_live_trial() {
    let _serial = SERIAL.lock().await;
    for case in ["target", "body", "auth", "action"] {
        let f = fixture(40000);
        let c = connect(&f);
        let mut m = manual(&f, &c).await;
        let mut actor = Box::pin(run_trial(
            &mut m.backend,
            &m.gate,
            credentials(),
            &m.id,
            &m.gate.inner,
            &m.state,
            &mut m.rx,
        ));
        ready_local(actor.as_mut(), &m.state).await;
        let other = fixture(40000);
        let body = match case {
            "target" => format!("{{\"trial_id\":\"{}\"}}", "c".repeat(64)),
            "body" => "{\"healthy\":true}".into(),
            _ => format!("{{\"trial_id\":\"{}\"}}", m.id),
        };
        let action = if case == "action" {
            RecoveryAction::BeginWifi
        } else {
            RecoveryAction::ConfirmWifi
        };
        let req = request(&f, &c, 2, action, body.as_bytes());
        let mut control = Box::pin(control_request(
            if case == "auth" {
                other.auth.clone()
            } else {
                f.auth.clone()
            },
            req,
        ));
        let result=timeout(Duration::from_secs(5),async {
            tokio::select! {r=&mut control=>r, _=&mut actor=>panic!("bad pending request killed trial")}
        }).await.unwrap();
        assert!(matches!(
            result,
            Err(Error::Forbidden | Error::InvalidInput)
        ));
        assert!(row(&f, 2).is_none());
        // Finish with trusted shutdown rather than making a second Auth caller
        // overlap the deliberately paused local actor's in-flight progress.
        let (reply, rx) = oneshot::channel();
        m.tx.try_send(Control {
            authority: ControlAuthority::Shutdown,
            confirm: false,
            reply,
        })
        .unwrap();
        timeout(Duration::from_secs(5), actor).await.unwrap();
        assert_eq!(rx.await.unwrap(), State::RolledBack);
        assert_eq!(&*m.probe.effects.lock().unwrap(), &["temporary", "restore"]);
        *ACTIVE.lock().await = None;
    }
}

#[tokio::test]
async fn original_deadline_ends_admission_and_late_auth_worker_cannot_start_a_save() {
    let _serial = SERIAL.lock().await;
    let f = fixture(1000);
    let c = connect(&f);
    let mut m = manual(&f, &c).await;
    let mut actor = Box::pin(run_trial(
        &mut m.backend,
        &m.gate,
        credentials(),
        &m.id,
        &m.gate.inner,
        &m.state,
        &mut m.rx,
    ));
    ready_local(actor.as_mut(), &m.state).await;
    let original = m.gate.inner.deadline();
    let w = write(&f, &c, 2, RecoveryAction::ConfirmWifi, Some(&m.id));
    let entered = Arc::new(Notify::new());
    let notify = entered.clone();
    let (release, held) = std::sync::mpsc::channel();
    let held = Mutex::new(held);
    let schedule = AdmissionSchedule {
        held: Some(Box::new(move || {
            notify.notify_one();
            held.lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
        })),
    };
    let mut control = Box::pin(control_recovery_inner(f.auth.clone(), w, schedule));
    timeout(Duration::from_secs(5),async {tokio::select!{_=entered.notified()=>(),_=&mut actor=>panic!("early"),_=&mut control=>panic!("early")}}).await.unwrap();
    timeout(Duration::from_secs(3), actor).await.unwrap();
    assert!(Instant::now() >= original);
    assert_eq!(m.gate.inner.deadline(), original);
    assert_eq!(*m.state.lock().await, State::RolledBack);
    release.send(()).unwrap();
    assert!(matches!(control.await, Err(Error::Busy)));
    let auth = f.auth.clone();
    timeout(
        Duration::from_secs(3),
        tokio::task::spawn_blocking(move || drop(auth.lock().unwrap())),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(row(&f, 2).is_none());
    assert_eq!(row(&f, 1).unwrap().0, "rolled_back");
    assert_eq!(&*m.probe.effects.lock().unwrap(), &["temporary", "restore"]);
    *ACTIVE.lock().await = None;
}

#[tokio::test]
async fn dropping_enqueued_reply_keeps_actor_ownership_and_reopen_does_not_create_a_second_commit()
{
    let _serial = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let mut m = manual(&f, &c).await;
    let mut actor = Box::pin(run_trial(
        &mut m.backend,
        &m.gate,
        credentials(),
        &m.id,
        &m.gate.inner,
        &m.state,
        &mut m.rx,
    ));
    ready_local(actor.as_mut(), &m.state).await;
    let req = request(
        &f,
        &c,
        2,
        RecoveryAction::ConfirmWifi,
        format!("{{\"trial_id\":\"{}\"}}", m.id).as_bytes(),
    );
    let mut control = Box::pin(control_request(f.auth.clone(), req));
    poll_pending(control.as_mut()).await;
    assert!(row(&f, 2).is_none());
    drop(control);
    timeout(Duration::from_secs(5), actor).await.unwrap();
    assert_eq!(row(&f, 2).unwrap().0, "saved");
    assert_eq!(&*m.probe.effects.lock().unwrap(), &["temporary", "persist"]);
    *ACTIVE.lock().await = None;
    drop(m);
    drop(c);
    let Fixture {
        auth,
        _store,
        d,
        _i,
        owner: _,
        grant: _,
        plan: _,
    } = f;
    drop(auth);
    drop(_store);
    let store = Store::open_recovery_execution(d.path()).unwrap();
    let auth = Auth::open(_i.path(), store.handle()).unwrap();
    let db = rusqlite::Connection::open_with_flags(
        d.path().join("store.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let rows: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM recovery_execution_receipts WHERE state='saved'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 2);
    drop(auth);
    drop(store);
}

#[tokio::test]
async fn queued_actor_loss_leaves_no_claim_and_reopen_fences_the_abandoned_begin() {
    let _serial = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let mut m = manual(&f, &c).await;
    let mut actor = Box::pin(run_trial(
        &mut m.backend,
        &m.gate,
        credentials(),
        &m.id,
        &m.gate.inner,
        &m.state,
        &mut m.rx,
    ));
    ready_local(actor.as_mut(), &m.state).await;
    let req = request(
        &f,
        &c,
        2,
        RecoveryAction::ConfirmWifi,
        format!("{{\"trial_id\":\"{}\"}}", m.id).as_bytes(),
    );
    let mut control = Box::pin(control_request(f.auth.clone(), req));
    poll_pending(control.as_mut()).await;
    assert!(row(&f, 2).is_none());
    // Simulate loss of actor/runtime ownership, then actually reopen Auth and
    // SQLite. This is a lifecycle fault, not an OS SIGKILL/device test.
    drop(actor);
    *ACTIVE.lock().await = None;
    drop(m);
    drop(c);
    assert!(matches!(control.await, Err(Error::Busy)));
    assert!(row(&f, 2).is_none());
    let Fixture {
        auth,
        _store,
        d,
        _i,
        owner: _,
        grant: _,
        plan: _,
    } = f;
    drop(auth);
    drop(_store);
    let store = Store::open_recovery_execution(d.path()).unwrap();
    let auth = Auth::open(_i.path(), store.handle()).unwrap();
    let db = rusqlite::Connection::open_with_flags(
        d.path().join("store.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let (count, state): (i64, String) = db
        .query_row(
            "SELECT COUNT(*),state FROM recovery_execution_receipts",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(state, "unknown");
    drop(auth);
    drop(store);
}
