// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    sync::{
        atomic::{AtomicBool, AtomicUsize},
        Mutex as StdMutex,
    },
};
type Events = Arc<StdMutex<Vec<&'static str>>>;
fn directory() -> (tempfile::TempDir, Directory) {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o2750)).unwrap();
    let m = fs::metadata(d.path()).unwrap();
    let p = Directory::open(d.path(), m.uid(), m.gid(), 0o2750).unwrap();
    (d, p)
}
struct Fixture {
    _input: tempfile::TempDir,
    _output: tempfile::TempDir,
    _journal: tempfile::TempDir,
    worker: Worker,
    hint: Hint,
}
impl Fixture {
    fn new(ttl_ms: u64) -> Self {
        let (input, i) = directory();
        let (output, o) = directory();
        let j = tempfile::tempdir().unwrap();
        fs::set_permissions(j.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let m = fs::metadata(j.path()).unwrap();
        let journal =
            Journal::open(Directory::open(j.path(), m.uid(), m.gid(), 0o700).unwrap()).unwrap();
        let now = channel::uptime_ms().unwrap();
        let hint = Hint {
            format: 1,
            binding: Binding {
                request_id: "00000000-0000-4000-8000-000000000001".into(),
                boot_id: channel::boot_id().unwrap(),
                realm: "00000000-0000-4000-8000-000000000002".into(),
                device_key_sha256: "1".repeat(64),
                material_sha256: "2".repeat(64),
                core_instance: "3".repeat(64),
                nonce: "4".repeat(64),
                created_uptime_ms: now,
                deadline_uptime_ms: now + 30_000,
            },
            action: Action::Start,
            sequence: 1,
            valid_until_uptime_ms: now + ttl_ms,
        };
        i.publish(Slot::Hint, &hint).unwrap();
        let worker = Worker {
            input: i,
            output: o,
            journal,
            boot: hint.binding.boot_id.clone(),
            realm: hint.binding.realm.clone(),
            key: hint.binding.device_key_sha256.clone(),
            material: hint.binding.material_sha256.clone(),
        };
        Self {
            _input: input,
            _output: output,
            _journal: j,
            worker,
            hint,
        }
    }
    fn credentials(&self) -> Credentials {
        Credentials::generate().unwrap()
    }
}
struct Readiness {
    allowed: AtomicBool,
    ap: AtomicBool,
    checks: AtomicUsize,
}
impl Readiness {
    fn good() -> Self {
        Self {
            allowed: AtomicBool::new(true),
            ap: AtomicBool::new(true),
            checks: AtomicUsize::new(0),
        }
    }
}
impl Ready for Readiness {
    async fn check(&self) -> Result<()> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        if self.allowed.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(Error::Denied)
        }
    }
    async fn ap_check(&self) -> Result<()> {
        if self.ap.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(Error::Denied)
        }
    }
}
struct Firewall {
    events: Events,
    bad: bool,
}
impl Guard for Firewall {
    async fn install(&mut self) -> Result<()> {
        self.events.lock().unwrap().push("guard");
        if self.bad {
            Err(Error::Io)
        } else {
            Ok(())
        }
    }
    async fn verify(&self) -> Result<()> {
        Ok(())
    }
    async fn release(&mut self) -> Result<()> {
        self.events.lock().unwrap().push("release");
        Ok(())
    }
}
struct Wifi {
    events: Events,
    rollback_bad: bool,
    blocked: bool,
    cancel_on_ready: Option<(Directory, Hint)>,
}
impl Network for Wifi {
    async fn preflight(&mut self) -> Result<()> {
        self.events.lock().unwrap().push("preflight");
        Ok(())
    }
    async fn checkpoint(&mut self, _seconds: u32) -> Result<()> {
        self.events.lock().unwrap().push("checkpoint");
        Ok(())
    }
    async fn activate(&mut self, _c: &Credentials) -> Result<()> {
        self.events.lock().unwrap().push("activate");
        if self.blocked {
            std::future::pending::<()>().await;
        }
        Ok(())
    }
    async fn ready(&self, _c: &Credentials) -> Result<bool> {
        self.events.lock().unwrap().push("ready");
        if let Some((dir, h)) = &self.cancel_on_ready {
            let mut h = h.clone();
            h.sequence = 2;
            h.action = Action::Cancel;
            h.valid_until_uptime_ms = channel::uptime_ms()? + 1000;
            dir.publish(Slot::Hint, &h)?;
        }
        Ok(true)
    }
    async fn rollback(&mut self, _c: &Credentials) -> Result<()> {
        self.events.lock().unwrap().push("rollback");
        if self.rollback_bad {
            Err(Error::Io)
        } else {
            Ok(())
        }
    }
}
fn backends(f: &Fixture, mode: &str) -> (Wifi, Firewall, Events) {
    let e = Arc::new(StdMutex::new(Vec::new()));
    (
        Wifi {
            events: e.clone(),
            rollback_bad: mode == "rollback",
            blocked: mode == "blocked",
            cancel_on_ready: if mode == "blocked" {
                None
            } else {
                Some((f.worker.input.clone(), f.hint.clone()))
            },
        },
        Firewall {
            events: e.clone(),
            bad: mode == "guard",
        },
        e,
    )
}
#[tokio::test]
async fn root_consumer_durable_start_single_activation_and_cancel_cleanup() {
    let f = Fixture::new(15_000);
    let c = f.credentials();
    let ready = Readiness::good();
    let (stop, rx) = watch::channel(false);
    let (mut n, mut g, e) = backends(&f, "normal");
    let status = timeout(
        Duration::from_secs(5),
        execute(&f.worker, &mut n, &mut g, &ready, &c, f.hint.clone(), rx),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(status.state, State::Closed);
    assert!(status.rollback_verified && status.guard_released);
    assert!(f.worker.journal.pending().unwrap().is_none());
    assert!(f
        .worker
        .completed(&serde_json::to_vec(&f.hint).unwrap())
        .unwrap());
    assert!(f.worker.journal.consume(&f.hint.binding).is_err());
    let e = e.lock().unwrap();
    assert_eq!(e.iter().filter(|v| **v == "activate").count(), 1);
    assert!(e.iter().position(|v| *v == "guard") < e.iter().position(|v| *v == "activate"));
    assert!(e.iter().position(|v| *v == "rollback") < e.iter().position(|v| *v == "release"));
    drop(stop);
}
#[tokio::test]
async fn expired_lease_interrupts_blocked_activation_without_retry() {
    let f = Fixture::new(700);
    let c = f.credentials();
    let ready = Readiness::good();
    let (_stop, rx) = watch::channel(false);
    let (mut n, mut g, e) = backends(&f, "blocked");
    let status = timeout(
        Duration::from_secs(4),
        execute(&f.worker, &mut n, &mut g, &ready, &c, f.hint.clone(), rx),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(status.state, State::Closed);
    let e = e.lock().unwrap();
    assert_eq!(e.iter().filter(|v| **v == "activate").count(), 1);
    assert!(e.contains(&"rollback") && e.contains(&"release"));
}
#[tokio::test]
async fn uncertain_guard_or_rollback_is_latched_across_recovery() {
    for mode in ["guard", "rollback"] {
        let f = Fixture::new(15_000);
        let c = f.credentials();
        let ready = Readiness::good();
        let (_stop, rx) = watch::channel(false);
        let (mut n, mut g, e) = backends(&f, mode);
        let status = timeout(
            Duration::from_secs(5),
            execute(&f.worker, &mut n, &mut g, &ready, &c, f.hint.clone(), rx),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(status.state, State::Unknown);
        assert!(!status.guard_released);
        assert!(f.worker.recover().unwrap());
        assert!(f.worker.journal.consume(&f.hint.binding).is_err());
        assert!(!e.lock().unwrap().contains(&"release"));
        if mode == "guard" {
            assert!(!e.lock().unwrap().contains(&"activate"));
        }
    }
}
#[tokio::test]
async fn absent_readiness_bad_binding_or_stopped_caller_has_no_side_effect() {
    for mode in ["tls", "binding", "stopped"] {
        let f = Fixture::new(15_000);
        let c = f.credentials();
        let ready = Readiness::good();
        let (_stop, rx) = watch::channel(mode == "stopped");
        let (mut n, mut g, e) = backends(&f, "normal");
        let mut hint = f.hint.clone();
        if mode == "binding" {
            hint.binding.material_sha256 = "8".repeat(64);
        }
        if mode == "tls" {
            ready.allowed.store(false, Ordering::SeqCst);
        }
        let result = execute(&f.worker, &mut n, &mut g, &ready, &c, hint, rx).await;
        if mode == "tls" {
            assert_eq!(result.unwrap().state, State::Closed);
            assert!(f.worker.journal.closed(&f.hint.binding).unwrap().is_some());
        } else {
            assert!(result.is_err());
        }
        assert!(f.worker.journal.pending().unwrap().is_none());
        assert!(e.lock().unwrap().is_empty());
    }
}
#[test]
fn pending_restart_is_unknown_and_completed_receipt_survives_lost_publication() {
    let f = Fixture::new(15_000);
    f.worker.journal.consume(&f.hint.binding).unwrap();
    assert!(f.worker.recover().unwrap());
    let done = receipt(&f.hint.binding, State::Closed, 1).unwrap();
    // Simulate crash between receipt sync and active marker removal.
    let path = f
        ._journal
        .path()
        .join(format!("{}.closed", f.hint.binding.journal_key().unwrap()));
    fs::write(&path, serde_json::to_vec(&done).unwrap()).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(!f.worker.recover().unwrap());
    assert!(f.worker.journal.pending().unwrap().is_none());
    assert!(f
        .worker
        .completed(&serde_json::to_vec(&f.hint).unwrap())
        .unwrap());
    let bytes = f.worker.output.read(Slot::Status).unwrap().unwrap();
    assert_eq!(
        Status::parse(&bytes, &f.hint.binding, channel::uptime_ms().unwrap())
            .unwrap()
            .state,
        State::Closed
    );
}

#[tokio::test]
async fn nm_ready_waits_for_ap_https_and_loss_after_ready_rolls_back() {
    let f = Fixture::new(15_000);
    let c = f.credentials();
    let ready = Readiness::good();
    ready.ap.store(false, Ordering::SeqCst);
    let (_stop, rx) = watch::channel(false);
    let (mut n, mut g, e) = backends(&f, "normal");
    n.cancel_on_ready = None;
    let operation = execute(&f.worker, &mut n, &mut g, &ready, &c, f.hint.clone(), rx);
    let observer = async {
        tokio::time::sleep(Duration::from_millis(750)).await;
        let raw = f.worker.output.read(Slot::Status).unwrap().unwrap();
        let status = Status::parse(&raw, &f.hint.binding, channel::uptime_ms().unwrap()).unwrap();
        assert_eq!(status.state, State::Starting);
        ready.ap.store(true, Ordering::SeqCst);
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let raw = f.worker.output.read(Slot::Status).unwrap().unwrap();
            let status =
                Status::parse(&raw, &f.hint.binding, channel::uptime_ms().unwrap()).unwrap();
            if status.state == State::Ready {
                break;
            }
        }
        ready.ap.store(false, Ordering::SeqCst);
    };
    let (done, ()) = timeout(Duration::from_secs(5), async {
        tokio::join!(operation, observer)
    })
    .await
    .unwrap();
    assert_eq!(done.unwrap().state, State::Closed);
    assert!(e.lock().unwrap().contains(&"rollback"));
}

#[test]
fn recovery_unknown_keeps_the_latest_sequence_and_cannot_be_hidden_by_old_ready() {
    let f = Fixture::new(15_000);
    f.worker.journal.consume(&f.hint.binding).unwrap();
    publish(
        &f.worker.output,
        &receipt(&f.hint.binding, State::Ready, 9).unwrap(),
    )
    .unwrap();
    let mut hint = f.hint.clone();
    hint.action = Action::KeepAlive;
    hint.sequence = 10;
    f.worker.input.publish(Slot::Hint, &hint).unwrap();
    assert!(f.worker.recover().unwrap());
    let raw = f.worker.output.read(Slot::Status).unwrap().unwrap();
    let status = Status::parse(&raw, &f.hint.binding, channel::uptime_ms().unwrap()).unwrap();
    assert_eq!(status.state, State::Unknown);
    assert_eq!(status.sequence, 10);
}

#[tokio::test]
async fn caller_shutdown_cancels_pending_dbus_and_does_not_drop_cleanup_future() {
    let f = Fixture::new(15_000);
    let c = f.credentials();
    let ready = Readiness::good();
    let (stop, rx) = watch::channel(false);
    let (mut n, mut g, e) = backends(&f, "blocked");
    let operation = execute(&f.worker, &mut n, &mut g, &ready, &c, f.hint.clone(), rx);
    let shutdown = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        stop.send_replace(true);
    };
    let (done, ()) = timeout(Duration::from_secs(4), async {
        tokio::join!(operation, shutdown)
    })
    .await
    .unwrap();
    assert_eq!(done.unwrap().state, State::Closed);
    let e = e.lock().unwrap();
    assert!(e.contains(&"activate") && e.contains(&"rollback") && e.contains(&"release"));
}

#[tokio::test]
async fn lost_status_publication_still_cleans_and_republishes_only_durable_proof() {
    let f = Fixture::new(15_000);
    let c = f.credentials();
    let ready = Readiness::good();
    let (_stop, rx) = watch::channel(false);
    let (mut n, mut g, e) = backends(&f, "normal");
    n.cancel_on_ready = None;
    let operation = execute(&f.worker, &mut n, &mut g, &ready, &c, f.hint.clone(), rx);
    let damage = async {
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if let Some(raw) = f.worker.output.read(Slot::Status).unwrap() {
                let s =
                    Status::parse(&raw, &f.hint.binding, channel::uptime_ms().unwrap()).unwrap();
                if s.state == State::Ready {
                    break;
                }
            }
        }
        fs::set_permissions(
            f._output.path().join("status.json"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
    };
    let (done, ()) = timeout(Duration::from_secs(5), async {
        tokio::join!(operation, damage)
    })
    .await
    .unwrap();
    assert!(done.is_err());
    assert!(e.lock().unwrap().contains(&"release"));
    assert!(f.worker.journal.pending().unwrap().is_none());
    assert!(f.worker.journal.closed(&f.hint.binding).unwrap().is_some());
    fs::set_permissions(
        f._output.path().join("status.json"),
        fs::Permissions::from_mode(0o640),
    )
    .unwrap();
    assert!(f
        .worker
        .completed(&serde_json::to_vec(&f.hint).unwrap())
        .unwrap());
    let raw = f.worker.output.read(Slot::Status).unwrap().unwrap();
    assert_eq!(
        Status::parse(&raw, &f.hint.binding, channel::uptime_ms().unwrap())
            .unwrap()
            .state,
        State::Closed
    );
    assert_eq!(
        e.lock()
            .unwrap()
            .iter()
            .filter(|v| **v == "activate")
            .count(),
        1
    );
}
