// SPDX-License-Identifier: AGPL-3.0-only
//! Deterministic polling of the ORIGINAL actor: no wall-clock race or HTTP hook.
use super::*;
use std::{future::Future, sync::atomic::AtomicUsize, task::Poll};

struct ScheduledGate {
    inner: RecoveryContext,
    tick: Notify,
    checks: AtomicUsize,
    busy: AtomicUsize,
}
impl Gate for ScheduledGate {
    async fn check(&self) -> Result<()> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        let result = self.inner.check().await;
        if matches!(result, Err(Error::Busy)) {
            self.busy.fetch_add(1, Ordering::SeqCst);
        }
        result
    }
}
impl ProvisionGate for ScheduledGate {
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
    for confirm in [true, false] {
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
            busy: AtomicUsize::new(0),
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
        let schedule = AdmissionSchedule {
            held: Some(Box::new(move || {
                notify.notify_one();
                held.recv_timeout(Duration::from_secs(5)).unwrap();
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
        // Keep Auth held until the original Gate's real blocking worker has
        // actually returned Busy. The fixed actor never starts this extra Gate.
        let actor_ended = if during != before {
            timeout(
                Duration::from_secs(5),
                std::future::poll_fn(|cx| {
                    let ended = actor.as_mut().poll(cx).is_ready();
                    if gate.busy.load(Ordering::SeqCst) > 0 {
                        Poll::Ready(ended)
                    } else {
                        assert!(!ended);
                        Poll::Pending
                    }
                }),
            )
            .await
            .unwrap()
        } else {
            false
        };
        release.send(()).unwrap();
        let result = timeout(Duration::from_secs(5), async {
            if actor_ended {
                control.await
            } else {
                tokio::join!(actor, control).1
            }
        })
        .await
        .unwrap();
        eprintln!(
            "CONTROL_ORDER gate_busy={} control_error={:?} begin_row={:?} control_row={:?}",
            gate.busy.load(Ordering::SeqCst),
            result.as_ref().err(),
            row(&f, 1),
            row(&f, 2)
        );
        *ACTIVE.lock().await = None;
        assert_eq!(
            during, before,
            "background Gate ran against its own control's Auth guard"
        );
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
