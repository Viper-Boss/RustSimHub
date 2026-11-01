// SPDX-License-Identifier: AGPL-3.0-only
//! Synthetic external faults through the real trial transaction; no NM or Internet access.
use super::*;
use std::sync::atomic::AtomicUsize;
use tokio::sync::Notify;

#[derive(Default)]
struct Probe {
    events: Mutex<Vec<&'static str>>,
    variant: Mutex<String>,
    gates: AtomicUsize,
    guards: AtomicUsize,
    revoked: AtomicBool,
}
impl Probe {
    fn variant(&self, value: impl Into<String>) {
        *self.variant.lock().unwrap() = value.into();
    }
    fn record(&self, name: &'static str) {
        self.events.lock().unwrap().push(name);
    }
    fn check(&self, case: &str, effects: &[&str]) {
        let events = self.events.lock().unwrap();
        let actual: Vec<_> = events
            .iter()
            .copied()
            .filter(|e| !["gate", "guard"].contains(e))
            .collect();
        assert_eq!(actual, effects, "case={case}: {events:?}");
        let counts: Vec<_> = [
            "gate", "guard", "audit", "begin", "usable", "persist", "restore", "finish", "release",
        ]
        .map(|name| (name, events.iter().filter(|e| **e == name).count()))
        .into();
        let variant = self.variant.lock().unwrap();
        eprintln!("case={case} variant={variant} counts={counts:?} trace={events:?}");
    }
}
struct CheckedGate {
    p: Arc<Probe>,
    deny_at: usize,
}
impl Gate for CheckedGate {
    async fn check(&self) -> Result<()> {
        self.p.record("gate");
        let n = self.p.gates.fetch_add(1, Ordering::SeqCst) + 1;
        if n == self.deny_at || self.p.revoked.load(Ordering::SeqCst) {
            Err(Error::Denied)
        } else {
            Ok(())
        }
    }
}
struct CheckedGuard {
    p: Arc<Probe>,
    deny_at: usize,
}
impl TrialGuard for CheckedGuard {
    async fn verify_trial(&self) -> Result<()> {
        self.p.record("guard");
        let n = self.p.guards.fetch_add(1, Ordering::SeqCst) + 1;
        if n == self.deny_at {
            Err(Error::Denied)
        } else {
            Ok(())
        }
    }
}
struct FaultNetwork {
    p: Arc<Probe>,
    uuid: String,
    fail: &'static str,
    ready: bool,
    pause: &'static str,
    entered: watch::Sender<&'static str>,
    resume: Arc<Notify>,
}
impl FaultNetwork {
    async fn step(&self, name: &'static str, c: &StationCredentials) -> Result<()> {
        assert_eq!(c.uuid, self.uuid, "cleanup must use the original operation");
        self.p.record(name);
        self.entered.send_replace(name);
        if name == self.pause {
            self.resume.notified().await;
        }
        if name == self.fail {
            Err(Error::Io)
        } else {
            Ok(())
        }
    }
}
impl StationNetwork for FaultNetwork {
    async fn begin(&mut self, c: &StationCredentials) -> Result<()> {
        self.step("begin", c).await
    }
    async fn usable(&self, c: &StationCredentials) -> Result<bool> {
        self.step("usable", c).await?;
        Ok(self.ready)
    }
    async fn restore_ap(&mut self, c: &StationCredentials) -> Result<()> {
        self.step("restore", c).await?;
        self.p.record("restored");
        Ok(())
    }
}
fn matrix(c: &StationCredentials) -> (FaultNetwork, CheckedGuard, CheckedGate) {
    let p = Arc::new(Probe::default());
    let (entered, _) = watch::channel("idle");
    (
        FaultNetwork {
            p: p.clone(),
            uuid: c.uuid.clone(),
            fail: "",
            ready: true,
            pause: "",
            entered,
            resume: Arc::new(Notify::new()),
        },
        CheckedGuard {
            p: p.clone(),
            deny_at: 0,
        },
        CheckedGate { p, deny_at: 0 },
    )
}
async fn reached(rx: &mut watch::Receiver<&'static str>, stage: &'static str) {
    timeout(Duration::from_secs(3), async {
        while *rx.borrow_and_update() != stage {
            rx.changed().await.unwrap();
        }
    })
    .await
    .expect("fault injection stage was not reached");
}

#[tokio::test]
async fn positive_control_records_real_gate_guard_and_exact_restore_order() {
    let c = credentials();
    let (mut n, g, a) = matrix(&c);
    n.p.variant("immediate_local_success");
    let (_tx, rx) = watch::channel(false);
    assert_eq!(
        run(&mut n, &g, &a, &c, Duration::from_secs(1), rx)
            .await
            .unwrap(),
        TrialOutcome::UsableAndRestored
    );
    assert_eq!(
        *n.p.events.lock().unwrap(),
        [
            "gate", "guard", "gate", "guard", "begin", "gate", "guard", "usable", "gate",
            "restore", "restored", "guard", "gate"
        ]
    );
    n.p.check(
        "positive_control",
        &["begin", "usable", "restore", "restored"],
    );
}

#[tokio::test]
async fn wrong_password_and_ssid_missing_backend_errors_restore_once() {
    // These are generic backend error injections, not real WPA/SSID classification.
    for (case, stage, effects) in [
        (
            "wrong_password",
            "begin",
            vec!["begin", "restore", "restored"],
        ),
        (
            "ssid_missing",
            "usable",
            vec!["begin", "usable", "restore", "restored"],
        ),
    ] {
        let c = credentials();
        let (mut n, g, a) = matrix(&c);
        n.p.variant(stage);
        n.fail = stage;
        let (_tx, rx) = watch::channel(false);
        assert_eq!(
            run(&mut n, &g, &a, &c, Duration::from_secs(1), rx)
                .await
                .unwrap(),
            TrialOutcome::FailedAndRestored
        );
        n.p.check(case, &effects);
    }
}

#[tokio::test]
async fn dhcp_deadline_uses_real_window_without_saving_or_claiming_internet() {
    let c = credentials();
    let (mut n, g, a) = matrix(&c);
    n.p.variant("window_250ms");
    n.ready = false;
    let (_tx, rx) = watch::channel(false);
    assert_eq!(
        run(&mut n, &g, &a, &c, Duration::from_millis(250), rx)
            .await
            .unwrap(),
        TrialOutcome::FailedAndRestored
    );
    let polls =
        n.p.events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| **e == "usable")
            .count();
    assert!(polls > 0);
    let mut expected = vec!["begin"];
    expected.extend(vec!["usable"; polls]);
    expected.extend(["restore", "restored"]);
    n.p.check("dhcp_deadline", &expected);
}

#[tokio::test]
async fn cancel_before_start_or_closed_caller_has_no_preflight_or_network_calls() {
    for closed in [false, true] {
        let c = credentials();
        let (mut n, g, a) = matrix(&c);
        n.p.variant(format!("sender_closed={closed}"));
        let (tx, rx) = watch::channel(!closed);
        if closed {
            drop(tx);
        }
        assert!(matches!(
            run(&mut n, &g, &a, &c, Duration::from_secs(1), rx).await,
            Err(Error::Denied)
        ));
        assert!(n.p.events.lock().unwrap().is_empty());
        n.p.check("cancel_before_start", &[]);
    }
}

#[tokio::test]
async fn cancellation_and_caller_loss_during_work_await_original_restore() {
    for (stage, closed) in [("begin", false), ("usable", false), ("begin", true)] {
        let c = credentials();
        let (mut n, g, a) = matrix(&c);
        n.p.variant(format!("stage={stage},sender_closed={closed}"));
        n.pause = stage;
        let mut entered = n.entered.subscribe();
        let (tx, rx) = watch::channel(false);
        let work = run(&mut n, &g, &a, &c, Duration::from_secs(2), rx);
        let driver = async {
            reached(&mut entered, stage).await;
            if closed {
                drop(tx);
            } else {
                tx.send(true).unwrap();
            }
        };
        let (result, ()) = timeout(Duration::from_secs(5), async { tokio::join!(work, driver) })
            .await
            .unwrap();
        assert_eq!(result.unwrap(), TrialOutcome::FailedAndRestored);
        let expected = if stage == "begin" {
            vec!["begin", "restore", "restored"]
        } else {
            vec!["begin", "usable", "restore", "restored"]
        };
        n.p.check(
            if closed {
                "caller_dropped"
            } else {
                "cancel_during_trial"
            },
            &expected,
        );
    }
}

#[tokio::test]
async fn cancellation_during_restore_keeps_cleanup_awaited() {
    let c = credentials();
    let (mut n, g, a) = matrix(&c);
    n.p.variant("restore_blocked");
    n.pause = "restore";
    let p = n.p.clone();
    let resume = n.resume.clone();
    let mut entered = n.entered.subscribe();
    let (tx, rx) = watch::channel(false);
    let work = run(&mut n, &g, &a, &c, Duration::from_secs(1), rx);
    tokio::pin!(work);
    let driver = async {
        reached(&mut entered, "restore").await;
        tx.send(true).unwrap();
        assert!(!p.events.lock().unwrap().contains(&"restored"));
    };
    // Drive the transaction to the restore handshake before inspecting its pending state.
    let result = timeout(Duration::from_secs(5), async {
        tokio::select! { result = &mut work => panic!("finished before restore released: {result:?}"), () = driver => {} }
        assert!(std::future::poll_fn(|cx| std::task::Poll::Ready(work.as_mut().poll(cx).is_pending())).await);
        resume.notify_one();
        work.await
    }).await.unwrap();
    assert_eq!(result.unwrap(), TrialOutcome::FailedAndRestored);
    p.check(
        "cancel_during_trial",
        &["begin", "usable", "restore", "restored"],
    );
}

#[tokio::test]
async fn restore_error_timeout_and_post_restore_guard_failure_remain_unknown() {
    for fault in ["error", "timeout", "guard"] {
        let c = credentials();
        let (mut n, mut g, a) = matrix(&c);
        n.p.variant(fault);
        match fault {
            "error" => n.fail = "restore",
            "timeout" => n.pause = "restore",
            _ => g.deny_at = 4,
        }
        let (_tx, rx) = watch::channel(false);
        let result = timeout(
            Duration::from_secs(12),
            run(&mut n, &g, &a, &c, Duration::from_secs(1), rx),
        )
        .await
        .unwrap();
        assert_eq!(result.unwrap(), TrialOutcome::Unknown);
        let mut expected = vec!["begin", "usable", "restore"];
        if fault == "guard" {
            expected.push("restored");
        }
        n.p.check("restore_failed", &expected);
    }
}

#[tokio::test]
async fn revoked_gate_at_each_real_check_never_confirms_old_work() {
    for deny_at in [1, 2, 3, 4, 5] {
        let c = credentials();
        let (mut n, g, mut a) = matrix(&c);
        n.p.variant(format!("deny_gate_{deny_at}"));
        a.deny_at = deny_at;
        let (_tx, rx) = watch::channel(false);
        let result = run(&mut n, &g, &a, &c, Duration::from_secs(1), rx).await;
        let expected = match deny_at {
            1 => {
                assert!(matches!(result, Err(Error::Denied)));
                vec![]
            }
            2 => vec!["restore", "restored"],
            3 => vec!["begin", "restore", "restored"],
            _ => vec!["begin", "usable", "restore", "restored"],
        };
        if deny_at != 1 {
            assert_eq!(result.unwrap(), TrialOutcome::FailedAndRestored);
        }
        n.p.check("revoke_before_commit", &expected);
    }
}

#[tokio::test]
async fn stale_completion_revoked_during_restore_cannot_confirm_trial() {
    // No invented epoch publisher: this is only the low-level Gate boundary.
    let c = credentials();
    let (mut n, g, a) = matrix(&c);
    n.p.variant("revoked_during_restore");
    n.pause = "restore";
    let p = n.p.clone();
    let resume = n.resume.clone();
    let mut entered = n.entered.subscribe();
    let (_tx, rx) = watch::channel(false);
    let work = run(&mut n, &g, &a, &c, Duration::from_secs(1), rx);
    let driver = async {
        reached(&mut entered, "restore").await;
        p.revoked.store(true, Ordering::SeqCst);
        resume.notify_one();
    };
    let (result, ()) = timeout(Duration::from_secs(5), async { tokio::join!(work, driver) })
        .await
        .unwrap();
    assert_eq!(result.unwrap(), TrialOutcome::FailedAndRestored);
    n.p.check(
        "stale_completion",
        &["begin", "usable", "restore", "restored"],
    );
}
