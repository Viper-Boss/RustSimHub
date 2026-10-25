// SPDX-License-Identifier: AGPL-3.0-only
//! Transaction failure/ordering fixtures; not a substitute for product Owner admission.
use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::sync::Notify;
type Events = Arc<Mutex<Vec<&'static str>>>;
struct Authority(Arc<AtomicBool>);
impl Gate for Authority {
    async fn check(&self) -> Result<()> {
        if self.0.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(Error::Denied)
        }
    }
}
struct Audit {
    used: AtomicBool,
    events: Events,
    fail: bool,
}
impl CommitAudit for Audit {
    fn consume(&self) -> Result<()> {
        if self.fail || self.used.swap(true, Ordering::SeqCst) {
            return Err(Error::Denied);
        }
        self.events.lock().unwrap().push("consumed");
        Ok(())
    }
}
struct Network {
    events: Events,
    fail: &'static str,
    ready: bool,
    revoke: Option<Arc<AtomicBool>>,
    block: Option<(&'static str, Arc<Notify>)>,
    entered: watch::Sender<&'static str>,
}
impl Network {
    async fn step(&self, name: &'static str) -> Result<()> {
        self.events.lock().unwrap().push(name);
        self.entered.send_replace(name);
        if let Some((stage, wait)) = &self.block {
            if *stage == name {
                wait.notified().await;
            }
        }
        if self.fail == name {
            Err(Error::Io)
        } else {
            Ok(())
        }
    }
}
impl StationNetwork for Network {
    async fn begin(&mut self, _: &StationCredentials) -> Result<()> {
        self.step("begin").await
    }
    async fn usable(&self, _: &StationCredentials) -> Result<bool> {
        Ok(self.ready)
    }
    async fn restore_ap(&mut self, _: &StationCredentials) -> Result<()> {
        self.step("restore").await
    }
}
impl CommitNetwork for Network {
    async fn persist(&mut self, _: &StationCredentials) -> Result<()> {
        self.step("persist").await?;
        if let Some(flag) = &self.revoke {
            flag.store(false, Ordering::SeqCst);
        }
        Ok(())
    }
    async fn finish<A: Gate + Sync>(&mut self, _: &StationCredentials, gate: &A) -> Result<()> {
        gate.check().await?;
        self.step("finish").await
    }
}
struct Isolation {
    events: Events,
    allowed: bool,
    release_ok: bool,
}
impl TrialGuard for Isolation {
    async fn verify_trial(&self) -> Result<()> {
        if self.allowed {
            Ok(())
        } else {
            Err(Error::Denied)
        }
    }
}
impl Guard for Isolation {
    async fn install(&mut self) -> Result<()> {
        panic!("guard was already installed by parent AP")
    }
    async fn verify(&self) -> Result<()> {
        self.verify_trial().await
    }
    async fn release(&mut self) -> Result<()> {
        self.events.lock().unwrap().push("release");
        if self.release_ok {
            Ok(())
        } else {
            Err(Error::Io)
        }
    }
}
fn credentials() -> StationCredentials {
    StationCredentials::new(
        "synthetic station".into(),
        zeroize::Zeroizing::new("synthetic-password".into()),
    )
    .unwrap()
}
fn fixture() -> (
    Network,
    Isolation,
    Authority,
    Audit,
    watch::Receiver<&'static str>,
) {
    let e = Arc::new(Mutex::new(Vec::new()));
    let (tx, rx) = watch::channel("idle");
    (
        Network {
            events: e.clone(),
            fail: "",
            ready: true,
            revoke: None,
            block: None,
            entered: tx,
        },
        Isolation {
            events: e.clone(),
            allowed: true,
            release_ok: true,
        },
        Authority(Arc::new(AtomicBool::new(true))),
        Audit {
            used: AtomicBool::new(false),
            events: e,
            fail: false,
        },
        rx,
    )
}
#[tokio::test]
async fn only_explicit_consumed_confirmation_can_save_and_release_after_final_cleanup() {
    let (mut n, mut g, a, j, _) = fixture();
    let (_tx, rx) = watch::channel(false);
    assert_eq!(
        run(
            &mut n,
            &mut g,
            &a,
            &j,
            &credentials(),
            Duration::from_secs(1),
            rx
        )
        .await
        .unwrap(),
        CommitOutcome::Saved
    );
    assert_eq!(
        *n.events.lock().unwrap(),
        ["consumed", "begin", "persist", "finish", "release"]
    );
    let (_tx, rx) = watch::channel(false);
    assert!(run(
        &mut n,
        &mut g,
        &a,
        &j,
        &credentials(),
        Duration::from_secs(1),
        rx
    )
    .await
    .is_err());
    assert_eq!(
        *n.events.lock().unwrap(),
        ["consumed", "begin", "persist", "finish", "release"]
    );
}
#[tokio::test]
async fn invalid_gate_guard_stop_audit_and_window_never_start_the_network() {
    for mode in 0..6 {
        let (mut n, mut g, a, mut j, _) = fixture();
        let (tx, rx) = watch::channel(mode == 2);
        if mode == 0 {
            a.0.store(false, Ordering::SeqCst);
        }
        if mode == 1 {
            g.allowed = false;
        }
        if mode == 3 {
            j.fail = true;
        }
        if mode == 4 {
            drop(tx);
        }
        let window = if mode == 5 {
            Duration::ZERO
        } else {
            Duration::from_secs(1)
        };
        assert!(run(&mut n, &mut g, &a, &j, &credentials(), window, rx)
            .await
            .is_err());
        assert!(n.events.lock().unwrap().is_empty());
    }
}
#[tokio::test]
async fn begin_or_save_failure_and_revocation_restore_exact_ap_without_releasing_guard() {
    for mode in 0..4 {
        let (mut n, mut g, a, j, _) = fixture();
        let (_tx, rx) = watch::channel(false);
        if mode == 0 {
            n.fail = "begin";
        }
        if mode == 1 {
            n.fail = "persist";
        }
        if mode == 2 {
            n.revoke = Some(a.0.clone());
        }
        if mode == 3 {
            n.ready = false;
        }
        assert_eq!(
            run(
                &mut n,
                &mut g,
                &a,
                &j,
                &credentials(),
                Duration::from_millis(20),
                rx
            )
            .await
            .unwrap(),
            CommitOutcome::RestoredAp
        );
        let e = n.events.lock().unwrap();
        assert_eq!(e.last(), Some(&"restore"));
        assert!(!e.contains(&"finish") && !e.contains(&"release"));
    }
}
#[tokio::test]
async fn failed_restore_keeps_unknown_and_isolation() {
    let (mut n, mut g, a, j, _) = fixture();
    n.ready = false;
    n.fail = "restore";
    let (_tx, rx) = watch::channel(false);
    assert_eq!(
        run(
            &mut n,
            &mut g,
            &a,
            &j,
            &credentials(),
            Duration::from_millis(20),
            rx
        )
        .await
        .unwrap(),
        CommitOutcome::Unknown
    );
    assert_eq!(*n.events.lock().unwrap(), ["consumed", "begin", "restore"]);
}
#[tokio::test]
async fn lost_finalization_or_guard_release_reply_never_invents_rollback_or_replays_save() {
    for fail_finish in [true, false] {
        let (mut n, mut g, a, j, _) = fixture();
        let (_tx, rx) = watch::channel(false);
        if fail_finish {
            n.fail = "finish";
        } else {
            g.release_ok = false;
        }
        assert_eq!(
            run(
                &mut n,
                &mut g,
                &a,
                &j,
                &credentials(),
                Duration::from_secs(1),
                rx
            )
            .await
            .unwrap(),
            CommitOutcome::Unknown
        );
        let e = n.events.lock().unwrap();
        assert!(!e.contains(&"restore"));
        assert_eq!(e.iter().filter(|v| **v == "persist").count(), 1);
        assert_eq!(e.contains(&"release"), !fail_finish);
    }
}
async fn wait_for(rx: &mut watch::Receiver<&'static str>, stage: &str) {
    while *rx.borrow_and_update() != stage {
        rx.changed().await.unwrap();
    }
}
#[tokio::test]
async fn cancellation_during_save_waits_for_ap_restoration() {
    let (mut n, mut g, a, j, mut entered) = fixture();
    let wait = Arc::new(Notify::new());
    n.block = Some(("persist", wait));
    let (tx, rx) = watch::channel(false);
    let c = credentials();
    let work = run(&mut n, &mut g, &a, &j, &c, Duration::from_secs(1), rx);
    tokio::pin!(work);
    tokio::select! {v=&mut work=>panic!("completed too early: {v:?}"),_=wait_for(&mut entered,"persist")=>{}}
    tx.send_replace(true);
    assert_eq!(work.await.unwrap(), CommitOutcome::RestoredAp);
    assert_eq!(
        *j.events.lock().unwrap(),
        ["consumed", "begin", "persist", "restore"]
    );
}
#[tokio::test]
async fn cancellation_after_finalization_started_does_not_drop_cleanup_or_guess_rollback() {
    let (mut n, mut g, a, j, mut entered) = fixture();
    let wait = Arc::new(Notify::new());
    n.block = Some(("finish", wait.clone()));
    let (tx, rx) = watch::channel(false);
    let c = credentials();
    let work = run(&mut n, &mut g, &a, &j, &c, Duration::from_secs(1), rx);
    tokio::pin!(work);
    tokio::select! {v=&mut work=>panic!("completed too early: {v:?}"),_=wait_for(&mut entered,"finish")=>{}}
    tx.send_replace(true);
    wait.notify_one();
    assert_eq!(work.await.unwrap(), CommitOutcome::Saved);
    assert_eq!(
        *j.events.lock().unwrap(),
        ["consumed", "begin", "persist", "finish", "release"]
    );
}
