// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::station_trial::{StationCredentials, StationNetwork};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    sync::{atomic::AtomicBool, Mutex as StdMutex},
};
fn directory(mode: u32) -> (tempfile::TempDir, Directory) {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(mode)).unwrap();
    let m = fs::metadata(d.path()).unwrap();
    let dir = Directory::open(d.path(), m.uid(), m.gid(), mode).unwrap();
    (d, dir)
}
struct Fixture {
    _dirs: Vec<tempfile::TempDir>,
    worker: Worker,
    hint: Hint,
}
impl Fixture {
    fn new(ttl: u64) -> Self {
        let (i, input) = directory(0o2750);
        let (o, output) = directory(0o2750);
        let (j, journal) = directory(0o700);
        let t = channel::uptime_ms().unwrap();
        let binding = Binding {
            request_id: "10000000-0000-4000-8000-000000000001".into(),
            boot_id: channel::boot_id().unwrap(),
            realm: "10000000-0000-4000-8000-000000000002".into(),
            device_key_sha256: "1".repeat(64),
            material_sha256: "2".repeat(64),
            core_instance: "3".repeat(64),
            nonce: "4".repeat(64),
            created_uptime_ms: t,
            deadline_uptime_ms: t + 120_000,
        };
        let hint = Hint {
            format: 1,
            binding: binding.clone(),
            action: Action::Start,
            sequence: 1,
            valid_until_uptime_ms: t + ttl,
        };
        input.publish(Slot::Hint, &hint).unwrap();
        Self {
            _dirs: vec![i, o, j],
            worker: Worker {
                input,
                output,
                journal: Journal::open(journal).unwrap(),
                boot: binding.boot_id,
                realm: binding.realm,
                key: binding.device_key_sha256,
                material: binding.material_sha256,
            },
            hint,
        }
    }
    fn request(&self) -> Request {
        let t = channel::uptime_ms().unwrap();
        Request {
            format: 1,
            binding: self.hint.binding.clone(),
            trial_id: "10000000-0000-4000-8000-000000000003".into(),
            owner_key_sha256: "5".repeat(64),
            owner_generation: 1,
            requested_uptime_ms: t,
            deadline_uptime_ms: t + 40_000,
            operation: Operation::Test {
                ssid: "synthetic station".into(),
                passphrase: station::Secret::new(Zeroizing::new("synthetic-passphrase".into())),
            },
        }
    }
}
#[derive(Default)]
struct Model {
    events: Vec<&'static str>,
    ap: bool,
    blocked: bool,
    restore_failed: bool,
    unknown: bool,
    https_lost_after_restore: bool,
    revoke_during_return_https: bool,
}
type Shared = Arc<StdMutex<Model>>;
struct Wifi(Shared);
impl Network for Wifi {
    async fn preflight(&mut self) -> Result<()> {
        self.0.lock().unwrap().events.push("preflight");
        Ok(())
    }
    async fn checkpoint(&mut self, _: u32) -> Result<()> {
        self.0.lock().unwrap().events.push("checkpoint");
        Ok(())
    }
    async fn activate(&mut self, _: &Credentials) -> Result<()> {
        let mut m = self.0.lock().unwrap();
        m.events.push("ap-activate");
        m.ap = true;
        Ok(())
    }
    async fn ready(&self, _: &Credentials) -> Result<bool> {
        Ok(self.0.lock().unwrap().ap)
    }
    async fn rollback(&mut self, _: &Credentials) -> Result<()> {
        let mut m = self.0.lock().unwrap();
        m.events.push("ap-rollback");
        if m.unknown {
            return Err(Error::Changed);
        }
        m.ap = false;
        Ok(())
    }
}
struct Session(Shared);
impl StationNetwork for Session {
    async fn begin(&mut self, _: &StationCredentials) -> Result<()> {
        let blocked = {
            let mut m = self.0.lock().unwrap();
            m.events.push("station-begin");
            m.ap = false;
            m.blocked
        };
        if blocked {
            std::future::pending::<()>().await;
        }
        Ok(())
    }
    async fn usable(&self, _: &StationCredentials) -> Result<bool> {
        Ok(true)
    }
    async fn restore_ap(&mut self, _: &StationCredentials) -> Result<()> {
        let mut m = self.0.lock().unwrap();
        m.events.push("station-restore");
        if m.restore_failed {
            m.unknown = true;
            return Err(Error::Changed);
        }
        m.ap = true;
        Ok(())
    }
}
impl StationProvider for Wifi {
    fn station_session<'a>(
        &'a mut self,
        _: &'a Credentials,
    ) -> Result<impl StationNetwork + Send + 'a> {
        Ok(Session(self.0.clone()))
    }
}
struct Firewall(Shared);
impl Guard for Firewall {
    async fn install(&mut self) -> Result<()> {
        self.0.lock().unwrap().events.push("guard");
        Ok(())
    }
    async fn verify(&self) -> Result<()> {
        Ok(())
    }
    async fn release(&mut self) -> Result<()> {
        self.0.lock().unwrap().events.push("release");
        Ok(())
    }
}
impl TrialGuard for Firewall {
    async fn verify_trial(&self) -> Result<()> {
        Ok(())
    }
}
struct Readiness(Shared, AtomicBool);
impl Ready for Readiness {
    async fn check(&self) -> Result<()> {
        if self.1.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(Error::Denied)
        }
    }
    async fn ap_check(&self) -> Result<()> {
        let m = self.0.lock().unwrap();
        if m.revoke_during_return_https && m.events.contains(&"station-restore") {
            self.1.store(false, Ordering::SeqCst);
        }
        if m.ap && !(m.https_lost_after_restore && m.events.contains(&"station-restore")) {
            Ok(())
        } else {
            Err(Error::Denied)
        }
    }
}
fn setup(m: Shared) -> (Wifi, Firewall, Readiness) {
    (
        Wifi(m.clone()),
        Firewall(m.clone()),
        Readiness(m, AtomicBool::new(true)),
    )
}
async fn wait(m: &Shared, event: &str) {
    timeout(Duration::from_secs(4), async {
        loop {
            if m.lock().unwrap().events.contains(&event) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn protected_request_runs_once_returns_https_verified_result_and_keeps_outer_lease() {
    let f = Fixture::new(15000);
    let req = f.request();
    let intent = req.intent().unwrap();
    f.worker.input.publish(Slot::StationRequest, &req).unwrap();
    let m = Arc::new(StdMutex::new(Model::default()));
    let (mut n, mut g, r) = setup(m.clone());
    let (tx, rx) = watch::channel(false);
    let ap = Credentials::generate().unwrap();
    let monitor = async {
        wait(&m, "station-restore").await;
        tokio::time::sleep(Duration::from_millis(1100)).await;
        let raw = f.worker.output.read(Slot::StationStatus).unwrap().unwrap();
        let s: station::Status = serde_json::from_slice(&raw).unwrap();
        assert!(s.phase == station::Phase::Ready && s.ap_https_verified);
        assert!(s.intent == intent);
        assert!(f.worker.journal.pending().unwrap().is_some());
        assert_eq!(
            m.lock()
                .unwrap()
                .events
                .iter()
                .filter(|e| **e == "station-begin")
                .count(),
            1
        );
        tx.send_replace(true);
    };
    let (result, _) = tokio::join!(
        execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx),
        monitor
    );
    assert!(result.unwrap().state == State::Closed);
    assert!(f.worker.journal.trial_result(&intent).unwrap().is_some());
    assert!(f.worker.journal.pending().unwrap().is_none());
    let events = &m.lock().unwrap().events;
    assert!(
        events.iter().position(|e| *e == "station-restore")
            < events.iter().position(|e| *e == "ap-rollback")
    );
}
#[tokio::test]
async fn shutdown_or_expired_lease_cancels_blocked_station_and_awaits_cleanup() {
    for expired in [false, true] {
        let f = Fixture::new(if expired { 100 } else { 15000 });
        let req = f.request();
        let intent = req.intent().unwrap();
        f.worker.input.publish(Slot::StationRequest, &req).unwrap();
        let m = Arc::new(StdMutex::new(Model {
            blocked: true,
            ..Default::default()
        }));
        let (mut n, mut g, r) = setup(m.clone());
        let (tx, rx) = watch::channel(false);
        let ap = Credentials::generate().unwrap();
        let monitor = async {
            wait(&m, "station-begin").await;
            if !expired {
                tx.send_replace(true);
            } else {
                tokio::time::sleep(Duration::from_millis(1500)).await;
            }
        };
        let (result, _) = tokio::join!(
            timeout(
                Duration::from_secs(5),
                execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx)
            ),
            monitor
        );
        assert!(result.unwrap().unwrap().state == State::Closed);
        assert!(m.lock().unwrap().events.contains(&"station-restore"));
        assert!(
            f.worker
                .journal
                .trial_result(&intent)
                .unwrap()
                .unwrap()
                .phase
                == station::Phase::Unknown
        );
    }
}
#[tokio::test]
async fn unknown_station_restore_retains_full_ap_marker_and_isolation() {
    let f = Fixture::new(15000);
    f.worker
        .input
        .publish(Slot::StationRequest, &f.request())
        .unwrap();
    let m = Arc::new(StdMutex::new(Model {
        restore_failed: true,
        ..Default::default()
    }));
    let (mut n, mut g, r) = setup(m.clone());
    let (_tx, rx) = watch::channel(false);
    let ap = Credentials::generate().unwrap();
    let s = timeout(
        Duration::from_secs(5),
        execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(s.state == State::Unknown);
    assert!(f.worker.journal.pending().unwrap().is_some());
    assert!(!m.lock().unwrap().events.contains(&"release"));
    assert!(f.worker.recover().unwrap());
    assert_eq!(
        m.lock()
            .unwrap()
            .events
            .iter()
            .filter(|e| **e == "station-begin")
            .count(),
        1
    );
}
#[tokio::test]
async fn ap_https_loss_after_network_restore_is_not_ready() {
    let f = Fixture::new(15000);
    let req = f.request();
    let intent = req.intent().unwrap();
    f.worker.input.publish(Slot::StationRequest, &req).unwrap();
    let m = Arc::new(StdMutex::new(Model {
        https_lost_after_restore: true,
        ..Default::default()
    }));
    let (mut n, mut g, r) = setup(m);
    let (_tx, rx) = watch::channel(false);
    let ap = Credentials::generate().unwrap();
    let s = timeout(
        Duration::from_secs(5),
        execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(s.state == State::Closed);
    let trial = f.worker.journal.trial_result(&intent).unwrap().unwrap();
    assert!(trial.phase == station::Phase::Unknown && !trial.ap_https_verified);
}
#[tokio::test]
async fn changed_payload_with_same_trial_id_never_replays_station_activation() {
    let f = Fixture::new(15000);
    let mut req = f.request();
    f.worker.input.publish(Slot::StationRequest, &req).unwrap();
    let m = Arc::new(StdMutex::new(Model::default()));
    let (mut n, mut g, r) = setup(m.clone());
    let (_tx, rx) = watch::channel(false);
    let ap = Credentials::generate().unwrap();
    let monitor = async {
        wait(&m, "station-restore").await;
        req.operation = Operation::Test {
            ssid: "changed".into(),
            passphrase: station::Secret::new(Zeroizing::new("other-passphrase".into())),
        };
        f.worker.input.publish(Slot::StationRequest, &req).unwrap();
    };
    let (result, _) = tokio::join!(
        timeout(
            Duration::from_secs(5),
            execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx)
        ),
        monitor
    );
    assert!(result.unwrap().unwrap().state == State::Closed);
    assert_eq!(
        m.lock()
            .unwrap()
            .events
            .iter()
            .filter(|e| **e == "station-begin")
            .count(),
        1
    );
}
#[tokio::test]
async fn exact_local_cancel_waits_for_restore_but_wrong_owner_cancel_is_ignored() {
    let f = Fixture::new(15000);
    let mut req = f.request();
    let owner = req.owner_key_sha256.clone();
    f.worker.input.publish(Slot::StationRequest, &req).unwrap();
    let m = Arc::new(StdMutex::new(Model {
        blocked: true,
        ..Default::default()
    }));
    let (mut n, mut g, r) = setup(m.clone());
    let (tx, rx) = watch::channel(false);
    let ap = Credentials::generate().unwrap();
    let monitor = async {
        wait(&m, "station-begin").await;
        req.operation = Operation::Cancel;
        req.owner_key_sha256 = "6".repeat(64);
        f.worker.input.publish(Slot::StationRequest, &req).unwrap();
        tokio::time::sleep(Duration::from_millis(700)).await;
        assert!(!m.lock().unwrap().events.contains(&"station-restore"));
        req.owner_key_sha256 = owner;
        f.worker.input.publish(Slot::StationRequest, &req).unwrap();
        wait(&m, "station-restore").await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        let s: station::Status =
            serde_json::from_slice(&f.worker.output.read(Slot::StationStatus).unwrap().unwrap())
                .unwrap();
        assert!(s.phase == station::Phase::Failed && s.ap_https_verified);
        tx.send_replace(true);
    };
    let (result, _) = tokio::join!(
        timeout(
            Duration::from_secs(5),
            execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx)
        ),
        monitor
    );
    assert!(result.unwrap().unwrap().state == State::Closed);
}
#[tokio::test]
async fn authority_change_during_return_https_probe_cannot_publish_ready() {
    let f = Fixture::new(15000);
    let req = f.request();
    let intent = req.intent().unwrap();
    f.worker.input.publish(Slot::StationRequest, &req).unwrap();
    let m = Arc::new(StdMutex::new(Model {
        revoke_during_return_https: true,
        ..Default::default()
    }));
    let (mut n, mut g, r) = setup(m);
    let (_tx, rx) = watch::channel(false);
    let ap = Credentials::generate().unwrap();
    let s = timeout(
        Duration::from_secs(5),
        execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(s.state == State::Closed);
    let result = f.worker.journal.trial_result(&intent).unwrap().unwrap();
    assert!(result.phase == station::Phase::Unknown && !result.ap_https_verified);
}
