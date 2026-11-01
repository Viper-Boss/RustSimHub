// SPDX-License-Identifier: AGPL-3.0-only
//! Actual Auth/Store and real private D-Bus; only NM/MM endpoints are synthetic.
use super::super::t5::{self, status::NetworkStatus};
use super::*;
use std::{
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU32, AtomicUsize},
};
const DEVICE: &str = "/org/freedesktop/NetworkManager/Devices/0";
const ACTIVE_PATH: &str = "/org/freedesktop/NetworkManager/ActiveConnection/0";
struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Bus {
    _daemon: Daemon,
    directory: TempDir,
    config: Arc<Config>,
    server: zbus::Connection,
    client: zbus::Connection,
}
#[derive(Default)]
struct Config {
    state: AtomicU32,
    active: AtomicU32,
    enabled: AtomicBool,
    permitted: AtomicBool,
    checkpoint: AtomicBool,
    hold: AtomicBool,
    calls: AtomicUsize,
    entered: Notify,
    release: Notify,
}
struct Root(Arc<Config>);
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Root {
    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        self.0.enabled.load(Ordering::Acquire)
    }
    #[zbus(property)]
    fn checkpoints(&self) -> Vec<OwnedObjectPath> {
        if self.0.checkpoint.load(Ordering::Acquire) {
            vec!["/org/freedesktop/NetworkManager/Checkpoint/0"
                .try_into()
                .unwrap()]
        } else {
            vec![]
        }
    }
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        vec![DEVICE.try_into().unwrap()]
    }
    async fn get_permissions(&self) -> HashMap<String, String> {
        self.0.calls.fetch_add(1, Ordering::AcqRel);
        self.0.entered.notify_one();
        if self.0.hold.load(Ordering::Acquire) {
            self.0.release.notified().await;
        }
        [
            "network-control",
            "checkpoint-rollback",
            "settings.modify.system",
        ]
        .into_iter()
        .map(|n| {
            (
                format!("org.freedesktop.NetworkManager.{n}"),
                if self.0.permitted.load(Ordering::Acquire) {
                    "yes"
                } else {
                    "auth"
                }
                .into(),
            )
        })
        .collect()
    }
}
struct Device(Arc<Config>);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
impl Device {
    #[zbus(property)]
    fn device_type(&self) -> u32 {
        2
    }
    #[zbus(property)]
    fn interface(&self) -> &str {
        "wlan0"
    }
    #[zbus(property)]
    fn managed(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        self.0.state.load(Ordering::Acquire)
    }
    #[zbus(property)]
    fn active_connection(&self) -> OwnedObjectPath {
        ACTIVE_PATH.try_into().unwrap()
    }
}
struct Active(Arc<Config>);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl Active {
    #[zbus(property)]
    fn state(&self) -> u32 {
        self.0.active.load(Ordering::Acquire)
    }
}
struct Voice;
#[zbus::interface(name = "org.freedesktop.ModemManager1.Modem.Voice")]
impl Voice {
    #[zbus(property)]
    fn calls(&self) -> Vec<OwnedObjectPath> {
        vec![]
    }
}
impl Bus {
    async fn new() -> Self {
        let directory = dir();
        let socket = directory.path().join("bus");
        let address = format!("unix:path={}", socket.display());
        let child = Daemon(
            Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--nopidfile"])
                .arg(format!("--address={address}"))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        timeout(Duration::from_secs(2), async {
            while !socket.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let config = Arc::new(Config {
            state: AtomicU32::new(100),
            active: AtomicU32::new(2),
            enabled: AtomicBool::new(true),
            permitted: AtomicBool::new(true),
            ..Config::default()
        });
        let server = zbus::connection::Builder::address(address.as_str())
            .unwrap()
            .name(NM)
            .unwrap()
            .name(MM)
            .unwrap()
            .serve_at(ROOT, Root(config.clone()))
            .unwrap()
            .serve_at(DEVICE, Device(config.clone()))
            .unwrap()
            .serve_at(ACTIVE_PATH, Active(config.clone()))
            .unwrap()
            .serve_at("/org/freedesktop/ModemManager1/Modem/0", Voice)
            .unwrap()
            .build()
            .await
            .unwrap();
        let client = zbus::connection::Builder::address(address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        Self {
            _daemon: child,
            directory,
            config,
            server,
            client,
        }
    }
    fn backend(&self) -> NetworkStatus {
        NetworkStatus {
            connection: Some(self.client.clone()),
        }
    }
    async fn call(
        &self,
        f: &Fixture,
        c: &RecoveryConnection,
        n: u8,
        body: &[u8],
    ) -> std::result::Result<Value, t5::RecoveryErrorV1> {
        let r = request(f, c, n, RecoveryAction::ReadStatus, body);
        t5::execute_with_status(
            f.auth.clone(),
            r,
            |_| -> Mock { panic!("status cannot start Wi-Fi") },
            self.backend(),
        )
        .await
        .map(|r| {
            let v = serde_json::to_value(r).unwrap();
            println!("T5_CORE_STATUS {}", serde_json::to_string(&v).unwrap());
            v
        })
    }
}
#[tokio::test]
async fn status_actual_bus_wifi_and_permissions_are_not_payload_claims() {
    let _s = SERIAL.lock().await;
    *ACTIVE.lock().await = None;
    let f = fixture(40000);
    let c = connect(&f);
    let b = Bus::new().await;
    let before = fs::read(f.d.path().join("store.sqlite3")).unwrap();
    let v = b.call(&f, &c, 1, b"{}").await.unwrap();
    assert_eq!(
        v,
        serde_json::json!({"contract":1,"source":"core","wifi":"connected","busy":false,"recovery_available":true})
    );
    b.config.state.store(30, Ordering::Release);
    assert_eq!(
        b.call(&f, &c, 2, b"{}").await.unwrap()["wifi"],
        "disconnected"
    );
    b.config.state.store(100, Ordering::Release);
    b.config.active.store(1, Ordering::Release);
    let v = b.call(&f, &c, 3, b"{}").await.unwrap();
    assert_eq!(v["wifi"], "unknown");
    assert_eq!(v["recovery_available"], false);
    b.config.active.store(2, Ordering::Release);
    b.config.permitted.store(false, Ordering::Release);
    assert_eq!(
        b.call(&f, &c, 4, b"{}").await.unwrap()["recovery_available"],
        false
    );
    b.config.permitted.store(true, Ordering::Release);
    b.config.checkpoint.store(true, Ordering::Release);
    let v = b.call(&f, &c, 5, b"{}").await.unwrap();
    assert_eq!(v["busy"], true);
    assert_eq!(v["recovery_available"], false);
    b.config.checkpoint.store(false, Ordering::Release);
    b.config.enabled.store(false, Ordering::Release);
    assert_eq!(
        b.call(&f, &c, 6, b"{}").await.unwrap()["recovery_available"],
        false
    );
    assert_eq!(before, fs::read(f.d.path().join("store.sqlite3")).unwrap());
    assert!(row(&f, 1).is_none());
}
#[tokio::test]
async fn status_authenticated_body_is_strict_before_any_backend_call() {
    let _s = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let b = Bus::new().await;
    for (n, body) in [
        b"[]".as_slice(),
        b"null",
        br#"{"wifi":"connected"}"#,
        br#"{"verified":true}"#,
        b"{}{}",
        b"",
    ]
    .into_iter()
    .enumerate()
    {
        let e = b.call(&f, &c, n as u8 + 1, body).await.err().unwrap();
        assert_eq!(serde_json::to_value(e).unwrap()["code"], "invalid_input");
    }
    assert_eq!(b.config.calls.load(Ordering::Acquire), 0);
}
#[tokio::test]
async fn status_auth_revoke_during_real_backend_call_discards_status() {
    let _s = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let b = Bus::new().await;
    b.config.hold.store(true, Ordering::Release);
    let r = request(&f, &c, 1, RecoveryAction::ReadStatus, b"{}");
    let a = f.auth.clone();
    let backend = b.backend();
    let task = tokio::spawn(async move {
        t5::execute_with_status(a, r, |_| -> Mock { panic!("status") }, backend).await
    });
    b.config.entered.notified().await;
    f.auth
        .lock()
        .unwrap()
        .revoke_recovery_key(
            f.owner.expose(),
            f.grant.independent_public_key,
            f.grant.generation,
            id(91),
        )
        .unwrap();
    b.config.release.notify_one();
    assert_eq!(
        serde_json::to_value(task.await.unwrap().err().unwrap()).unwrap()["code"],
        // Existing Auth converts the persisted grant refusal to Storage.
        // T5 preserves its finite unknown error and returns NO status facts.
        "unknown"
    );
}
#[tokio::test]
async fn status_backend_state_changes_and_owner_replacement_are_unknown() {
    let _s = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let b = Bus::new().await;
    for replace_owner in [false, true] {
        b.config.hold.store(true, Ordering::Release);
        let r = request(
            &f,
            &c,
            if replace_owner { 2 } else { 1 },
            RecoveryAction::ReadStatus,
            b"{}",
        );
        let a = f.auth.clone();
        let backend = b.backend();
        let task = tokio::spawn(async move {
            t5::execute_with_status(a, r, |_| -> Mock { panic!("status") }, backend).await
        });
        b.config.entered.notified().await;
        let other = if replace_owner {
            b.server.release_name(NM).await.unwrap();
            let address = format!("unix:path={}", b.directory.path().join("bus").display());
            Some(
                zbus::connection::Builder::address(address.as_str())
                    .unwrap()
                    .name(NM)
                    .unwrap()
                    .build()
                    .await
                    .unwrap(),
            )
        } else {
            b.config.state.store(30, Ordering::Release);
            None
        };
        b.config.release.notify_one();
        let v = serde_json::to_value(task.await.unwrap().unwrap()).unwrap();
        assert_eq!(v["wifi"], "unknown");
        assert_eq!(v["busy"], true);
        assert_eq!(v["recovery_available"], false);
        b.config.state.store(100, Ordering::Release);
        drop(other);
    }
}
#[tokio::test]
async fn status_query_deadline_and_actual_call_lock_do_not_start_a_trial() {
    let _s = SERIAL.lock().await;
    *ACTIVE.lock().await = None;
    let f = fixture(40000);
    let c = connect(&f);
    let b = Bus::new().await;
    let voice = crate::coordination::global()
        .telephony(CallAction::Dial)
        .unwrap();
    let v = b.call(&f, &c, 1, b"{}").await.unwrap();
    assert_eq!(v["busy"], true);
    assert_eq!(v["recovery_available"], false);
    drop(voice);
    b.config.hold.store(true, Ordering::Release);
    let now = Instant::now();
    let e = b.call(&f, &c, 2, b"{}").await.err().unwrap();
    assert_eq!(serde_json::to_value(e).unwrap()["code"], "deadline");
    assert!(now.elapsed() >= Duration::from_secs(3));
    b.config.release.notify_one();
    assert!(row(&f, 2).is_none());
    assert_eq!(TRIALS.available_permits(), 1);
}
