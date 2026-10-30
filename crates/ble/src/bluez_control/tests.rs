// SPDX-License-Identifier: AGPL-3.0-only
//! Real private dbus-daemon and actual RPCs; only BlueZ manager endpoints are synthetic.
use super::*;
use std::{
    future::Future,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tempfile::TempDir;
use tokio::sync::Notify;
use zbus::zvariant::OwnedObjectPath;

async fn bounded<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(3), future)
        .await
        .expect("private bus deadline")
}
struct Bus {
    child: Child,
    _directory: TempDir,
    address: String,
}
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Bus {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("test-bus.sock");
        let address = format!("unix:path={}", socket.display());
        let child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--nopidfile"])
            .arg(format!("--address={address}"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("private dbus-daemon required");
        let mut bus = Self {
            child,
            _directory: directory,
            address,
        };
        bounded(async {
            while !socket.exists() {
                assert!(
                    bus.child.try_wait().unwrap().is_none(),
                    "private bus exited"
                );
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        bus
    }
    async fn connect(&self) -> Connection {
        bounded(
            zbus::connection::Builder::address(self.address.as_str())
                .unwrap()
                .build(),
        )
        .await
        .unwrap()
    }
    async fn bluez(&self, state: Arc<Calls>, advertisement: bool) -> Connection {
        let mut builder = zbus::connection::Builder::address(self.address.as_str())
            .unwrap()
            .name(BLUEZ)
            .unwrap()
            .serve_at("/org/bluez/hci0", Gatt(state.clone()))
            .unwrap();
        if advertisement {
            builder = builder
                .serve_at("/org/bluez/hci0", Advertising(state))
                .unwrap();
        }
        bounded(builder.build()).await.unwrap()
    }
}
#[derive(Default)]
struct Calls {
    rows: Mutex<Vec<(&'static str, String, usize)>>,
    hold: AtomicBool,
    fail: AtomicBool,
    started: Notify,
    release: Notify,
}
impl Calls {
    async fn record(
        &self,
        name: &'static str,
        path: OwnedObjectPath,
        options: usize,
    ) -> zbus::fdo::Result<()> {
        self.rows
            .lock()
            .unwrap()
            .push((name, path.to_string(), options));
        self.started.notify_one();
        if self.hold.load(Ordering::Acquire) {
            self.release.notified().await;
        }
        if self.fail.load(Ordering::Acquire) {
            Err(zbus::fdo::Error::AccessDenied(
                "synthetic private detail".into(),
            ))
        } else {
            Ok(())
        }
    }
}
struct Gatt(Arc<Calls>);
#[zbus::interface(name = "org.bluez.GattManager1")]
impl Gatt {
    async fn register_application(
        &self,
        path: OwnedObjectPath,
        options: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<()> {
        self.0.record("app+", path, options.len()).await
    }
    async fn unregister_application(&self, path: OwnedObjectPath) -> zbus::fdo::Result<()> {
        self.0.record("app-", path, 0).await
    }
}
struct Advertising(Arc<Calls>);
#[zbus::interface(name = "org.bluez.LEAdvertisingManager1")]
impl Advertising {
    async fn register_advertisement(
        &self,
        path: OwnedObjectPath,
        options: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<()> {
        self.0.record("adv+", path, options.len()).await
    }
    async fn unregister_advertisement(&self, path: OwnedObjectPath) -> zbus::fdo::Result<()> {
        self.0.record("adv-", path, 0).await
    }
}
async fn control(bus: &Bus, server: &Connection) -> BluezControl {
    bounded(BluezControl::bind(
        bus.connect().await,
        server.unique_name().unwrap().as_str(),
        Adapter::Hci0,
    ))
    .await
    .unwrap()
}

#[tokio::test]
async fn four_rpcs_use_captured_owner_fixed_paths_interfaces_and_empty_options() {
    let bus = Bus::new().await;
    let calls = Arc::new(Calls::default());
    let server = bus.bluez(calls.clone(), true).await;
    let mut port = control(&bus, &server).await;
    bounded(port.register_application()).await.unwrap();
    bounded(port.register_advertisement()).await.unwrap();
    bounded(port.unregister_advertisement()).await.unwrap();
    bounded(port.unregister_application()).await.unwrap();
    assert_eq!(
        *calls.rows.lock().unwrap(),
        vec![
            ("app+", manifest::ROOT.into(), 0),
            ("adv+", manifest::ADVERTISEMENT_PATH.into(), 0),
            ("adv-", manifest::ADVERTISEMENT_PATH.into(), 0),
            ("app-", manifest::ROOT.into(), 0)
        ]
    );
}

#[tokio::test]
async fn absent_owner_and_well_known_expected_name_never_start_registration() {
    let bus = Bus::new().await;
    let client = bus.connect().await;
    let owner = client.unique_name().unwrap().to_string();
    assert!(matches!(
        bounded(BluezControl::bind(client.clone(), &owner, Adapter::Hci0)).await,
        Err(Error::Unavailable)
    ));
    assert!(matches!(
        bounded(BluezControl::bind(client, BLUEZ, Adapter::Hci0)).await,
        Err(Error::InvalidInput)
    ));
}

#[tokio::test]
async fn different_actual_unique_owner_is_rejected_before_any_manager_call() {
    let bus = Bus::new().await;
    let calls = Arc::new(Calls::default());
    let _server = bus.bluez(calls.clone(), true).await;
    let wrong = bus.connect().await;
    let owner = wrong.unique_name().unwrap().to_string();
    assert!(matches!(
        bounded(BluezControl::bind(
            bus.connect().await,
            &owner,
            Adapter::Hci0
        ))
        .await,
        Err(Error::StaleContext)
    ));
    assert!(calls.rows.lock().unwrap().is_empty());
}

#[tokio::test]
async fn replacement_owner_never_receives_old_registration_or_cleanup() {
    let bus = Bus::new().await;
    let old_calls = Arc::new(Calls::default());
    let old = bus.bluez(old_calls.clone(), true).await;
    let mut port = control(&bus, &old).await;
    assert!(bounded(old.release_name(BLUEZ)).await.unwrap());
    let new_calls = Arc::new(Calls::default());
    let _new = bus.bluez(new_calls.clone(), true).await;
    for operation in [
        port.register_application(),
        port.unregister_application(),
        port.register_advertisement(),
        port.unregister_advertisement(),
    ] {
        assert_eq!(bounded(operation).await, Err(Error::StaleContext));
    }
    assert!(old_calls.rows.lock().unwrap().is_empty());
    assert!(new_calls.rows.lock().unwrap().is_empty());
}

#[tokio::test]
async fn owner_change_after_real_request_rejects_late_success_without_retry() {
    let bus = Bus::new().await;
    let calls = Arc::new(Calls::default());
    calls.hold.store(true, Ordering::Release);
    let old = bus.bluez(calls.clone(), true).await;
    let mut port = control(&bus, &old).await;
    let mut pending = port.register_application();
    bounded(async {
        tokio::select! {
            ()=calls.started.notified()=>(),
            _=&mut pending=>panic!("request completed before release"),
        }
    })
    .await;
    assert!(bounded(old.release_name(BLUEZ)).await.unwrap());
    let new_calls = Arc::new(Calls::default());
    let _new = bus.bluez(new_calls.clone(), true).await;
    calls.release.notify_one();
    assert_eq!(bounded(pending).await, Err(Error::StaleContext));
    assert_eq!(calls.rows.lock().unwrap().len(), 1);
    assert!(new_calls.rows.lock().unwrap().is_empty());
}

#[tokio::test]
async fn caller_deadline_can_retain_the_same_real_future_until_late_reply() {
    let bus = Bus::new().await;
    let calls = Arc::new(Calls::default());
    calls.hold.store(true, Ordering::Release);
    let server = bus.bluez(calls.clone(), true).await;
    let mut port = control(&bus, &server).await;
    let mut pending = port.register_advertisement();
    bounded(async {
        tokio::select! {
            ()=calls.started.notified()=>(),
            _=&mut pending=>panic!("request completed before release"),
        }
    })
    .await;
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut pending)
            .await
            .is_err()
    );
    assert_eq!(calls.rows.lock().unwrap().len(), 1);
    calls.release.notify_one();
    bounded(pending).await.unwrap();
    assert_eq!(calls.rows.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn remote_error_is_finite_and_not_converted_to_success_or_a_retry() {
    let bus = Bus::new().await;
    let calls = Arc::new(Calls::default());
    calls.fail.store(true, Ordering::Release);
    let server = bus.bluez(calls.clone(), true).await;
    let mut port = control(&bus, &server).await;
    assert_eq!(
        bounded(port.register_application()).await,
        Err(Error::Backend)
    );
    assert_eq!(calls.rows.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn absent_manager_interface_is_not_replaced_by_another_bluez_operation() {
    let bus = Bus::new().await;
    let calls = Arc::new(Calls::default());
    let server = bus.bluez(calls.clone(), false).await;
    let mut port = control(&bus, &server).await;
    assert_eq!(
        bounded(port.register_advertisement()).await,
        Err(Error::Backend)
    );
    assert!(calls.rows.lock().unwrap().is_empty());
}

#[tokio::test]
async fn release_and_reacquire_by_the_same_unique_owner_cannot_revive_old_context() {
    let bus = Bus::new().await;
    let calls = Arc::new(Calls::default());
    let old = bus.bluez(calls.clone(), true).await;
    let mut port = control(&bus, &old).await;
    assert!(bounded(old.release_name(BLUEZ)).await.unwrap());
    bounded(old.request_name(BLUEZ)).await.unwrap();
    // The current unique name is identical; only observing the gap distinguishes it.
    assert_eq!(
        bounded(port.register_application()).await,
        Err(Error::StaleContext)
    );
    assert!(calls.rows.lock().unwrap().is_empty());
}

#[tokio::test]
async fn untrusted_sender_cannot_forge_the_broker_owner_change_signal() {
    let bus = Bus::new().await;
    let calls = Arc::new(Calls::default());
    let server = bus.bluez(calls.clone(), true).await;
    let mut port = control(&bus, &server).await;
    let stranger = bus.connect().await;
    bounded(stranger.emit_signal(
        None::<&str>,
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameOwnerChanged",
        &(
            BLUEZ,
            server.unique_name().unwrap().as_str(),
            stranger.unique_name().unwrap().as_str(),
        ),
    ))
    .await
    .unwrap();
    bounded(port.register_application()).await.unwrap();
    assert_eq!(calls.rows.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn explicit_retirement_fences_real_inflight_reply_and_future_calls() {
    let bus = Bus::new().await;
    let calls = Arc::new(Calls::default());
    calls.hold.store(true, Ordering::Release);
    let server = bus.bluez(calls.clone(), true).await;
    let mut port = control(&bus, &server).await;
    let mut pending = port.register_application();
    bounded(async {
        tokio::select! {
            ()=calls.started.notified()=>(),
            _=&mut pending=>panic!("request completed before release"),
        }
    })
    .await;
    port.retire();
    calls.release.notify_one();
    assert_eq!(bounded(pending).await, Err(Error::StaleContext));
    assert_eq!(
        bounded(port.unregister_application()).await,
        Err(Error::StaleContext)
    );
    assert_eq!(calls.rows.lock().unwrap().len(), 1);
}
