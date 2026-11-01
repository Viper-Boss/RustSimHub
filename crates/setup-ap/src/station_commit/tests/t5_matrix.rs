// SPDX-License-Identifier: AGPL-3.0-only
//! Synthetic external faults through the real commit transaction; no NM or Internet access.
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
    saved: AtomicBool,
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
    release_fail: bool,
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
    restore_fail: bool,
    ready: bool,
    pause: &'static str,
    entered: watch::Sender<&'static str>,
    resume: Arc<Notify>,
}
impl FaultNetwork {
    async fn step(&self, name: &'static str, c: &StationCredentials) -> Result<()> {
        assert_eq!(c.uuid, self.uuid, "cleanup must use the original operation");
        self.p.record(name);
        if name == "persist" {
            self.p.saved.store(true, Ordering::SeqCst);
            self.p.record("partial_write");
        }
        self.entered.send_replace(name);
        if name == self.pause {
            self.resume.notified().await;
        }
        if name == self.fail || name == "restore" && self.restore_fail {
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
        self.p.saved.store(false, Ordering::SeqCst);
        self.p.record("restored");
        Ok(())
    }
}
fn matrix(c: &StationCredentials) -> (FaultNetwork, CheckedGuard, CheckedGate, CheckedAudit) {
    let p = Arc::new(Probe::default());
    let (entered, _) = watch::channel("idle");
    (
        FaultNetwork {
            p: p.clone(),
            uuid: c.uuid.clone(),
            fail: "",
            restore_fail: false,
            ready: true,
            pause: "",
            entered,
            resume: Arc::new(Notify::new()),
        },
        CheckedGuard {
            p: p.clone(),
            deny_at: 0,
            release_fail: false,
        },
        CheckedGate {
            p: p.clone(),
            deny_at: 0,
        },
        CheckedAudit {
            p,
            used: AtomicBool::new(false),
            deny: false,
        },
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

impl CommitNetwork for FaultNetwork {
    async fn persist(&mut self, c: &StationCredentials) -> Result<()> {
        // Side effect precedes both a failed reply and an interrupted reply.
        self.step("persist", c).await
    }
    async fn finish<A: Gate + Sync>(&mut self, c: &StationCredentials, gate: &A) -> Result<()> {
        self.step("finish", c).await?;
        // Real provider contract: verify current authority before final destruction.
        gate.check().await
    }
}
impl Guard for CheckedGuard {
    async fn install(&mut self) -> Result<()> {
        panic!("parent owns guard installation")
    }
    async fn verify(&self) -> Result<()> {
        self.verify_trial().await
    }
    async fn release(&mut self) -> Result<()> {
        self.p.record("release");
        if self.release_fail {
            Err(Error::Io)
        } else {
            Ok(())
        }
    }
}
struct CheckedAudit {
    p: Arc<Probe>,
    used: AtomicBool,
    deny: bool,
}
impl CommitAudit for CheckedAudit {
    fn consume(&self) -> Result<()> {
        self.p.record("audit");
        if self.deny || self.used.swap(true, Ordering::SeqCst) {
            Err(Error::Denied)
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn positive_control_consumes_once_before_save_finish_and_release() {
    let c = credentials();
    let (mut n, mut g, a, j) = matrix(&c);
    n.p.variant("saved_then_audit_replay_denied");
    let (_tx, rx) = watch::channel(false);
    assert_eq!(
        run(&mut n, &mut g, &a, &j, &c, Duration::from_secs(1), rx)
            .await
            .unwrap(),
        CommitOutcome::Saved
    );
    assert_eq!(
        *n.p.events.lock().unwrap(),
        [
            "gate",
            "guard",
            "audit",
            "gate",
            "guard",
            "begin",
            "gate",
            "guard",
            "usable",
            "gate",
            "persist",
            "partial_write",
            "gate",
            "guard",
            "finish",
            "gate",
            "release"
        ]
    );
    n.p.check(
        "positive_control",
        &[
            "audit",
            "begin",
            "usable",
            "persist",
            "partial_write",
            "finish",
            "release",
        ],
    );
    assert!(n.p.saved.load(Ordering::SeqCst));
    // The same consumed audit cannot authorize a second network transaction.
    let before = n.p.events.lock().unwrap().len();
    let (_tx2, rx2) = watch::channel(false);
    assert!(matches!(
        run(&mut n, &mut g, &a, &j, &c, Duration::from_secs(1), rx2).await,
        Err(Error::Denied)
    ));
    assert_eq!(
        &n.p.events.lock().unwrap()[before..],
        &["gate", "guard", "audit"]
    );
}

#[tokio::test]
async fn initial_stop_caller_loss_gate_guard_audit_and_invalid_windows_do_not_begin() {
    for fault in ["stop", "caller", "gate", "guard", "audit", "zero", "over40"] {
        let c = credentials();
        let (mut n, mut g, mut a, mut j) = matrix(&c);
        n.p.variant(fault);
        let (tx, rx) = watch::channel(fault == "stop");
        if fault == "caller" {
            drop(tx);
        }
        if fault == "gate" {
            a.deny_at = 1;
        }
        if fault == "guard" {
            g.deny_at = 1;
        }
        if fault == "audit" {
            j.deny = true;
        }
        let window = match fault {
            "zero" => Duration::ZERO,
            "over40" => Duration::from_secs(41),
            _ => Duration::from_secs(1),
        };
        let result = run(&mut n, &mut g, &a, &j, &c, window, rx).await;
        if ["zero", "over40"].contains(&fault) {
            assert!(matches!(result, Err(Error::Invalid)));
        } else {
            assert!(matches!(result, Err(Error::Denied)));
        }
        let expected: &[&str] = if fault == "audit" { &["audit"] } else { &[] };
        n.p.check("cancel_before_start", expected);
        let checks: &[&str] = match fault {
            "gate" => &["gate"],
            "guard" => &["gate", "guard"],
            "audit" => &["gate", "guard", "audit"],
            _ => &[],
        };
        assert_eq!(*n.p.events.lock().unwrap(), checks);
    }
}

#[tokio::test]
async fn wrong_password_and_ssid_missing_errors_never_reach_persistence() {
    for (case, stage) in [("wrong_password", "begin"), ("ssid_missing", "usable")] {
        let c = credentials();
        let (mut n, mut g, a, j) = matrix(&c);
        n.p.variant(stage);
        n.fail = stage;
        let (_tx, rx) = watch::channel(false);
        assert_eq!(
            run(&mut n, &mut g, &a, &j, &c, Duration::from_secs(1), rx)
                .await
                .unwrap(),
            CommitOutcome::RestoredAp
        );
        let mut expected = vec!["audit", "begin"];
        if stage == "usable" {
            expected.push("usable");
        }
        expected.extend(["restore", "restored"]);
        n.p.check(case, &expected);
        assert!(!n.p.saved.load(Ordering::SeqCst));
    }
}

#[tokio::test]
async fn dhcp_deadline_never_persists_an_unusable_station() {
    let c = credentials();
    let (mut n, mut g, a, j) = matrix(&c);
    n.p.variant("window_250ms");
    n.ready = false;
    let (_tx, rx) = watch::channel(false);
    assert_eq!(
        run(&mut n, &mut g, &a, &j, &c, Duration::from_millis(250), rx)
            .await
            .unwrap(),
        CommitOutcome::RestoredAp
    );
    let polls =
        n.p.events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| **e == "usable")
            .count();
    assert!(polls > 0);
    let mut expected = vec!["audit", "begin"];
    expected.extend(vec!["usable"; polls]);
    expected.extend(["restore", "restored"]);
    n.p.check("dhcp_deadline", &expected);
}

#[tokio::test]
async fn cancellation_or_caller_loss_during_partial_save_awaits_exact_restoration() {
    for closed in [false, true] {
        let c = credentials();
        let (mut n, mut g, a, j) = matrix(&c);
        n.p.variant(format!("persist,sender_closed={closed}"));
        n.pause = "persist";
        let mut entered = n.entered.subscribe();
        let p = n.p.clone();
        let (tx, rx) = watch::channel(false);
        let work = run(&mut n, &mut g, &a, &j, &c, Duration::from_secs(2), rx);
        let driver = async {
            reached(&mut entered, "persist").await;
            assert!(p.saved.load(Ordering::SeqCst));
            if closed {
                drop(tx);
            } else {
                tx.send(true).unwrap();
            }
        };
        let (result, ()) = timeout(Duration::from_secs(5), async { tokio::join!(work, driver) })
            .await
            .unwrap();
        assert_eq!(result.unwrap(), CommitOutcome::RestoredAp);
        assert!(!n.p.saved.load(Ordering::SeqCst));
        n.p.check(
            if closed {
                "caller_dropped"
            } else {
                "cancel_during_trial"
            },
            &[
                "audit",
                "begin",
                "usable",
                "persist",
                "partial_write",
                "restore",
                "restored",
            ],
        );
    }
}

#[tokio::test]
async fn partial_persist_error_preserves_evidence_until_restore_is_verified() {
    for restore in ["ok", "error", "timeout", "guard"] {
        let c = credentials();
        let (mut n, mut g, a, j) = matrix(&c);
        n.p.variant(format!("persist_error,restore={restore}"));
        n.fail = "persist";
        // Two faults need separate injection points: persist returns an error after writing;
        // an independent guard/restore failure makes cleanup uncertain.
        if restore == "error" {
            n.restore_fail = true;
        }
        if restore == "timeout" {
            n.pause = "restore";
        }
        if restore == "guard" {
            g.deny_at = 4;
        }
        let (_tx, rx) = watch::channel(false);
        let result = timeout(
            Duration::from_secs(12),
            run(&mut n, &mut g, &a, &j, &c, Duration::from_secs(1), rx),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            result,
            if restore == "ok" {
                CommitOutcome::RestoredAp
            } else {
                CommitOutcome::Unknown
            }
        );
        let uncertain_backend = ["error", "timeout"].contains(&restore);
        assert_eq!(n.p.saved.load(Ordering::SeqCst), uncertain_backend);
        let mut expected = vec![
            "audit",
            "begin",
            "usable",
            "persist",
            "partial_write",
            "restore",
        ];
        if !uncertain_backend {
            expected.push("restored");
        }
        n.p.check("partial_persist", &expected);
    }
}

#[tokio::test]
async fn restore_error_timeout_and_guard_failure_do_not_release_isolation() {
    for fault in ["error", "timeout", "guard"] {
        let c = credentials();
        let (mut n, mut g, a, j) = matrix(&c);
        n.p.variant(fault);
        // Fail the post-begin gate so there has been a network mutation but no save.
        let a = CheckedGate { deny_at: 3, ..a };
        match fault {
            "error" => n.fail = "restore",
            "timeout" => n.pause = "restore",
            _ => g.deny_at = 3,
        }
        let (_tx, rx) = watch::channel(false);
        assert_eq!(
            timeout(
                Duration::from_secs(12),
                run(&mut n, &mut g, &a, &j, &c, Duration::from_secs(1), rx)
            )
            .await
            .unwrap()
            .unwrap(),
            CommitOutcome::Unknown
        );
        let mut expected = vec!["audit", "begin", "restore"];
        if fault == "guard" {
            expected.push("restored");
        }
        n.p.check("restore_failed", &expected);
    }
}

#[tokio::test]
async fn revoke_before_commit_distinguishes_unconsumed_pre_save_and_post_save() {
    for deny_at in [1, 2, 3, 4, 5] {
        let c = credentials();
        let (mut n, mut g, mut a, j) = matrix(&c);
        n.p.variant(format!("deny_gate_{deny_at}"));
        a.deny_at = deny_at;
        let (_tx, rx) = watch::channel(false);
        let result = run(&mut n, &mut g, &a, &j, &c, Duration::from_secs(1), rx).await;
        let mut expected = vec![];
        if deny_at == 1 {
            assert!(matches!(result, Err(Error::Denied)));
        } else {
            assert_eq!(result.unwrap(), CommitOutcome::RestoredAp);
            expected.push("audit");
            if deny_at >= 3 {
                expected.push("begin");
            }
            if deny_at >= 4 {
                expected.push("usable");
            }
            if deny_at == 5 {
                expected.extend(["persist", "partial_write"]);
            }
            expected.extend(["restore", "restored"]);
        }
        n.p.check("revoke_before_commit", &expected);
    }
}

#[tokio::test]
async fn caller_loss_or_cancel_after_finish_starts_does_not_drop_finalizer() {
    for (closed, fail) in [(false, false), (true, false), (true, true)] {
        let c = credentials();
        let (mut n, mut g, a, j) = matrix(&c);
        n.p.variant(format!("finish,sender_closed={closed},reply_failed={fail}"));
        n.pause = "finish";
        if fail {
            n.fail = "finish";
        }
        let resume = n.resume.clone();
        let mut entered = n.entered.subscribe();
        let (tx, rx) = watch::channel(false);
        let work = run(&mut n, &mut g, &a, &j, &c, Duration::from_secs(1), rx);
        let driver = async {
            reached(&mut entered, "finish").await;
            if closed {
                drop(tx);
            } else {
                tx.send(true).unwrap();
            }
            resume.notify_one();
        };
        let (result, ()) = timeout(Duration::from_secs(5), async { tokio::join!(work, driver) })
            .await
            .unwrap();
        assert_eq!(
            result.unwrap(),
            if fail {
                CommitOutcome::Unknown
            } else {
                CommitOutcome::Saved
            }
        );
        let mut expected = vec![
            "audit",
            "begin",
            "usable",
            "persist",
            "partial_write",
            "finish",
        ];
        if !fail {
            expected.push("release");
        }
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
async fn stale_completion_finalizer_rechecks_real_gate_without_blind_rollback() {
    let c = credentials();
    let (mut n, mut g, a, j) = matrix(&c);
    n.p.variant("finish_gate_revoked");
    n.pause = "finish";
    let p = n.p.clone();
    let resume = n.resume.clone();
    let mut entered = n.entered.subscribe();
    let (_tx, rx) = watch::channel(false);
    let work = run(&mut n, &mut g, &a, &j, &c, Duration::from_secs(1), rx);
    let driver = async {
        reached(&mut entered, "finish").await;
        p.revoked.store(true, Ordering::SeqCst);
        resume.notify_one();
    };
    let (result, ()) = timeout(Duration::from_secs(5), async { tokio::join!(work, driver) })
        .await
        .unwrap();
    assert_eq!(result.unwrap(), CommitOutcome::Unknown);
    assert!(n.p.saved.load(Ordering::SeqCst));
    n.p.check(
        "stale_completion",
        &[
            "audit",
            "begin",
            "usable",
            "persist",
            "partial_write",
            "finish",
        ],
    );
}

#[tokio::test]
async fn finalizer_timeout_and_release_failure_remain_unknown_without_save_replay() {
    for fault in ["finish_timeout", "release"] {
        let c = credentials();
        let (mut n, mut g, a, j) = matrix(&c);
        n.p.variant(fault);
        if fault == "finish_timeout" {
            n.pause = "finish";
        } else {
            g.release_fail = true;
        }
        let (_tx, rx) = watch::channel(false);
        assert_eq!(
            timeout(
                Duration::from_secs(12),
                run(&mut n, &mut g, &a, &j, &c, Duration::from_secs(1), rx)
            )
            .await
            .unwrap()
            .unwrap(),
            CommitOutcome::Unknown
        );
        let mut expected = vec![
            "audit",
            "begin",
            "usable",
            "persist",
            "partial_write",
            "finish",
        ];
        if fault == "release" {
            expected.push("release");
        }
        assert!(n.p.saved.load(Ordering::SeqCst));
        n.p.check("partial_persist", &expected);
    }
}
