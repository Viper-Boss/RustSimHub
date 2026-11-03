// SPDX-License-Identifier: AGPL-3.0-only
//! Real owning runtime/private broker/Unix I/O. BlueZ and Core wire endpoints
//! only are synthetic. Same-UID socket fixture is NOT the separate role proof.
use super::*;
use crate::{
    bluez_control::{tests::Bus, Adapter},
    manifest::{self, Characteristic},
};
use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tokio::{io::AsyncReadExt, net::UnixListener, sync::Notify};
use zbus::{
    zvariant::{OwnedObjectPath, OwnedValue, Value},
    Proxy,
};
const DEVICE: &str = "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF";
#[derive(Default)]
struct Calls {
    rows: Mutex<Vec<u8>>,
    ready: Notify,
    hold: AtomicBool,
    release: Notify,
}
struct Gatt(Arc<Calls>);
#[zbus::interface(name = "org.bluez.GattManager1")]
impl Gatt {
    async fn register_application(
        &self,
        path: OwnedObjectPath,
        options: HashMap<String, OwnedValue>,
    ) {
        assert_eq!(path.as_str(), manifest::ROOT);
        assert!(options.is_empty());
        self.0.rows.lock().unwrap().push(0);
    }
    async fn unregister_application(&self, path: OwnedObjectPath) {
        assert_eq!(path.as_str(), manifest::ROOT);
        self.0.rows.lock().unwrap().push(3);
    }
}
struct Adv(Arc<Calls>);
#[zbus::interface(name = "org.bluez.LEAdvertisingManager1")]
impl Adv {
    async fn register_advertisement(
        &self,
        path: OwnedObjectPath,
        options: HashMap<String, OwnedValue>,
    ) {
        assert_eq!(path.as_str(), manifest::ADVERTISEMENT_PATH);
        assert!(options.is_empty());
        self.0.rows.lock().unwrap().push(1);
        self.0.ready.notify_one();
        if self.0.hold.load(Ordering::Acquire) {
            self.0.release.notified().await;
        }
    }
    async fn unregister_advertisement(&self, path: OwnedObjectPath) {
        assert_eq!(path.as_str(), manifest::ADVERTISEMENT_PATH);
        self.0.rows.lock().unwrap().push(2);
    }
}
struct Device;
#[zbus::interface(name = "org.bluez.Device1")]
impl Device {
    #[zbus(property)]
    fn connected(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn adapter(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/org/bluez/hci0").unwrap()
    }
}
async fn bounded<T>(f: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(3), f)
        .await
        .expect("real runtime fixture deadline")
}
async fn bluez(bus: &Bus, calls: Arc<Calls>) -> zbus::Connection {
    let b = bus.connect().await;
    b.request_name("org.bluez").await.unwrap();
    b.object_server()
        .at("/org/bluez/hci0", Gatt(calls.clone()))
        .await
        .unwrap();
    b.object_server()
        .at("/org/bluez/hci0", Adv(calls))
        .await
        .unwrap();
    b.object_server().at(DEVICE, Device).await.unwrap();
    b
}
async fn control(bus: &Bus, b: &zbus::Connection) -> (BluezControl, String) {
    let c = bus.connect().await;
    let name = c.unique_name().unwrap().to_string();
    (
        BluezControl::bind(c, b.unique_name().unwrap().as_str(), Adapter::Hci0)
            .await
            .unwrap(),
        name,
    )
}
async fn core() -> (tempfile::TempDir, UnixSecurePort, UnixListener) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o2750)).unwrap();
    let path = dir.path().join("recovery.sock");
    let listener = UnixListener::bind(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o660)).unwrap();
    let port = UnixSecurePort::connect(CoreEndpoint::same_uid_fixture(path))
        .await
        .unwrap();
    let (mut probe, _) = bounded(listener.accept()).await.unwrap();
    assert_eq!(
        probe.read_u8().await.unwrap_err().kind(),
        std::io::ErrorKind::UnexpectedEof
    );
    (dir, port, listener)
}
async fn write(b: &zbus::Connection, name: &str) {
    let proxy = Proxy::new(
        b,
        name,
        Characteristic::AuthExchange.path(),
        "org.bluez.GattCharacteristic1",
    )
    .await
    .unwrap();
    let options: HashMap<&str, OwnedValue> = HashMap::from([
        (
            "device",
            Value::from(OwnedObjectPath::try_from(DEVICE).unwrap())
                .try_into()
                .unwrap(),
        ),
        ("type", Value::from("request").try_into().unwrap()),
    ]);
    let frame = vec![b'R', b'B', 1, 1, 1, 0, 0, 0, 1, 0, 3, 0, b'a', b'b', b'c'];
    bounded(proxy.call::<_, _, ()>("WriteValue", &(frame, options)))
        .await
        .unwrap();
}
async fn held_input(listener: UnixListener, received: Arc<Notify>) {
    let (mut socket, _) = bounded(listener.accept()).await.unwrap();
    let n = socket.read_u32().await.unwrap();
    assert_eq!(n, 5);
    let mut payload = [0; 5];
    socket.read_exact(&mut payload).await.unwrap();
    assert_eq!(payload, [1, 1, b'a', b'b', b'c']);
    received.notify_one();
    assert_eq!(
        bounded(socket.read_u8()).await.unwrap_err().kind(),
        std::io::ErrorKind::UnexpectedEof
    );
}

#[tokio::test]
async fn explicit_policy_is_strict_bounded_object_with_fixed_endpoint() {
    let good = br#"{"format":1,"core_uid":1001,"ble_uid":1002,"ble_gid":1002}"#;
    assert_eq!(
        endpoint_from_policy(good).unwrap().path(),
        std::path::Path::new(CORE_SOCKET)
    );
    for bad in ["[1,1001,1002,1002]", "{}", "null", "{\"format\":1,\"core_uid\":1001,\"ble_uid\":1001,\"ble_gid\":1002}",
        "{\"format\":1,\"core_uid\":0,\"ble_uid\":1002,\"ble_gid\":1002}",
        "{\"format\":1,\"core_uid\":1001,\"ble_uid\":1002,\"ble_gid\":1002,\"format\":1}",
        "{\"format\":1,\"core_uid\":1001,\"ble_uid\":1002,\"ble_gid\":1002,\"socket\":\"/tmp/any\"}"] {
        assert!(endpoint_from_policy(bad.as_bytes()).is_err(),"{bad}");
    }
    assert!(endpoint_from_policy(&vec![b' '; 8193]).is_err());
}
#[tokio::test]
async fn runtime_shutdown_unregisters_both_actual_exports_in_order() {
    let bus = Bus::new().await;
    let calls = Arc::new(Calls::default());
    let b = bluez(&bus, calls.clone()).await;
    let (c, _) = control(&bus, &b).await;
    let (_dir, port, _listener) = core().await;
    let (stop, shutdown) = oneshot::channel();
    let running = tokio::spawn(run(c, port, [54; 16], async {
        let _ = shutdown.await;
    }));
    bounded(calls.ready.notified()).await;
    stop.send(()).unwrap();
    bounded(running).await.unwrap().unwrap();
    assert_eq!(*calls.rows.lock().unwrap(), [0, 1, 2, 3]);
}
#[tokio::test]
async fn late_advertisement_registration_is_retained_until_compensated_on_stop() {
    let bus = Bus::new().await;
    let calls = Arc::new(Calls::default());
    calls.hold.store(true, Ordering::Release);
    let b = bluez(&bus, calls.clone()).await;
    let (c, _) = control(&bus, &b).await;
    let (_dir, port, _listener) = core().await;
    let (stop, shutdown) = oneshot::channel();
    let running = tokio::spawn(run(c, port, [55; 16], async {
        let _ = shutdown.await;
    }));
    bounded(calls.ready.notified()).await;
    stop.send(()).unwrap();
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(
        !running.is_finished(),
        "late registration still owns original cleanup"
    );
    calls.release.notify_one();
    bounded(running).await.unwrap().unwrap();
    assert_eq!(*calls.rows.lock().unwrap(), [0, 1, 2, 3]);
}
#[tokio::test]
async fn real_device_loss_closes_held_core_io_then_unregisters_without_reconnect() {
    let bus = Bus::new().await;
    let calls = Arc::new(Calls::default());
    let b = bluez(&bus, calls.clone()).await;
    let (c, name) = control(&bus, &b).await;
    let (_dir, port, listener) = core().await;
    let received = Arc::new(Notify::new());
    let core = tokio::spawn(held_input(listener, received.clone()));
    let running = tokio::spawn(run(c, port, [56; 16], std::future::pending()));
    bounded(calls.ready.notified()).await;
    write(&b, &name).await;
    bounded(received.notified()).await;
    let changed = HashMap::from([("Connected", OwnedValue::from(false))]);
    b.emit_signal(
        None::<&str>,
        DEVICE,
        "org.freedesktop.DBus.Properties",
        "PropertiesChanged",
        &("org.bluez.Device1", changed, Vec::<String>::new()),
    )
    .await
    .unwrap();
    bounded(core).await.unwrap();
    assert_eq!(bounded(running).await.unwrap(), Err(Error::StaleContext));
    assert_eq!(*calls.rows.lock().unwrap(), [0, 1, 2, 3]);
}
#[tokio::test]
async fn shutdown_fences_actual_pending_core_socket_before_remote_cleanup() {
    let bus = Bus::new().await;
    let calls = Arc::new(Calls::default());
    let b = bluez(&bus, calls.clone()).await;
    let (c, name) = control(&bus, &b).await;
    let (_dir, port, listener) = core().await;
    let received = Arc::new(Notify::new());
    let core = tokio::spawn(held_input(listener, received.clone()));
    let (stop, shutdown) = oneshot::channel();
    let running = tokio::spawn(run(c, port, [57; 16], async {
        let _ = shutdown.await;
    }));
    bounded(calls.ready.notified()).await;
    write(&b, &name).await;
    bounded(received.notified()).await;
    stop.send(()).unwrap();
    bounded(core).await.unwrap();
    bounded(running).await.unwrap().unwrap();
    assert_eq!(*calls.rows.lock().unwrap(), [0, 1, 2, 3]);
}
#[tokio::test]
async fn owner_replacement_fences_actual_core_and_never_cleans_replacement() {
    let bus = Bus::new().await;
    let calls = Arc::new(Calls::default());
    let b = bluez(&bus, calls.clone()).await;
    let (c, name) = control(&bus, &b).await;
    let (_dir, port, listener) = core().await;
    let received = Arc::new(Notify::new());
    let core = tokio::spawn(held_input(listener, received.clone()));
    let running = tokio::spawn(run(c, port, [58; 16], std::future::pending()));
    bounded(calls.ready.notified()).await;
    write(&b, &name).await;
    bounded(received.notified()).await;
    b.release_name("org.bluez").await.unwrap();
    let new_calls = Arc::new(Calls::default());
    let _replacement = bluez(&bus, new_calls.clone()).await;
    bounded(core).await.unwrap();
    assert!(bounded(running).await.unwrap().is_err());
    assert!(new_calls.rows.lock().unwrap().is_empty());
}
#[tokio::test]
async fn actual_advertisement_release_stops_and_cleans_current_owner() {
    let bus = Bus::new().await;
    let calls = Arc::new(Calls::default());
    let b = bluez(&bus, calls.clone()).await;
    let (c, name) = control(&bus, &b).await;
    let (_dir, port, _listener) = core().await;
    let running = tokio::spawn(run(c, port, [59; 16], std::future::pending()));
    bounded(calls.ready.notified()).await;
    let proxy = Proxy::new(
        &b,
        name.as_str(),
        manifest::ADVERTISEMENT_PATH,
        "org.bluez.LEAdvertisement1",
    )
    .await
    .unwrap();
    bounded(proxy.call::<_, _, ()>("Release", &()))
        .await
        .unwrap();
    assert!(bounded(running).await.unwrap().is_err());
    // Release is BlueZ evidence that this advertisement is already gone.
    assert_eq!(*calls.rows.lock().unwrap(), [0, 1, 3]);
}
