// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::bluez_control::{
    tests::{Bus, Calls},
    Adapter,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::Notify;

const DEVICE: &str = "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF";
fn epoch() -> Epoch {
    Epoch::new([11; 16], 1).unwrap()
}
async fn bounded<T>(f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(3), f)
        .await
        .expect("private peer deadline")
}
pub(crate) struct DeviceState {
    connected: AtomicBool,
    hold: AtomicBool,
    started: Notify,
    release: Notify,
}
impl DeviceState {
    fn connected() -> Arc<Self> {
        Arc::new(Self {
            connected: AtomicBool::new(true),
            hold: AtomicBool::new(false),
            started: Notify::new(),
            release: Notify::new(),
        })
    }
}
struct Device(Arc<DeviceState>);
#[zbus::interface(name = "org.bluez.Device1")]
impl Device {
    #[zbus(property)]
    async fn connected(&self) -> bool {
        if self.0.hold.load(Ordering::Acquire) {
            self.0.started.notify_one();
            self.0.release.notified().await;
        }
        self.0.connected.load(Ordering::Acquire)
    }
    #[zbus(property)]
    fn adapter(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/org/bluez/hci0").unwrap()
    }
}
struct BrokenDevice;
#[zbus::interface(name = "org.bluez.Device1")]
impl BrokenDevice {
    #[zbus(property)]
    fn connected(&self) -> &str {
        "true"
    }
    #[zbus(property)]
    fn adapter(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/org/bluez/hci0").unwrap()
    }
}
struct ForeignAdapter;
#[zbus::interface(name = "org.bluez.Device1")]
impl ForeignAdapter {
    #[zbus(property)]
    fn connected(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn adapter(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/org/bluez/hci1").unwrap()
    }
}
struct MissingConnected;
#[zbus::interface(name = "org.bluez.Device1")]
impl MissingConnected {
    #[zbus(property)]
    fn adapter(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/org/bluez/hci0").unwrap()
    }
}
async fn setup(bus: &Bus) -> (Connection, BluezControl, Arc<DeviceState>) {
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    assert!(bluez
        .object_server()
        .at("/", zbus::fdo::ObjectManager)
        .await
        .unwrap());
    let state = DeviceState::connected();
    assert!(bluez
        .object_server()
        .at(DEVICE, Device(state.clone()))
        .await
        .unwrap());
    let control = BluezControl::bind(
        bus.connect().await,
        bluez.unique_name().unwrap().as_str(),
        Adapter::Hci0,
    )
    .await
    .unwrap();
    (bluez, control, state)
}
async fn changed(
    sender: &Connection,
    path: &str,
    values: HashMap<String, OwnedValue>,
    invalidated: Vec<String>,
) {
    bounded(sender.emit_signal(
        None::<&str>,
        path,
        "org.freedesktop.DBus.Properties",
        "PropertiesChanged",
        &(DEVICE_INTERFACE, values, invalidated),
    ))
    .await
    .unwrap();
    // Real broker barrier after this sender's signal; no fake message context.
    bounded(zbus::fdo::DBusProxy::new(sender).await.unwrap().get_id())
        .await
        .unwrap();
}

#[tokio::test]
async fn real_snapshot_pins_connected_device_actual_owner_adapter_and_epoch() {
    let bus = Bus::new().await;
    let (bluez, control, _state) = setup(&bus).await;
    let lease = bounded(PeerLease::bind(&control, epoch(), DEVICE))
        .await
        .unwrap();
    assert_eq!(lease.peer().owner(), bluez.unique_name().unwrap().as_str());
    assert_eq!(lease.peer().device_path(), DEVICE);
    assert_eq!(lease.epoch(), epoch());
    bounded(lease.check_current()).await.unwrap();
}

#[tokio::test]
async fn same_path_disconnect_and_reconnect_never_revives_old_lease() {
    let bus = Bus::new().await;
    let (bluez, control, state) = setup(&bus).await;
    let lease = PeerLease::bind(&control, epoch(), DEVICE).await.unwrap();
    state.connected.store(false, Ordering::Release);
    changed(
        &bluez,
        DEVICE,
        HashMap::from([("Connected".into(), OwnedValue::from(false))]),
        vec![],
    )
    .await;
    state.connected.store(true, Ordering::Release);
    changed(
        &bluez,
        DEVICE,
        HashMap::from([("Connected".into(), OwnedValue::from(true))]),
        vec![],
    )
    .await;
    assert_eq!(
        bounded(lease.check_current()).await,
        Err(Error::StaleContext)
    );
    assert_eq!(
        bounded(lease.check_current()).await,
        Err(Error::StaleContext)
    );
}

#[tokio::test]
async fn bind_subscribes_before_snapshot_and_rejects_gap_while_query_is_pending() {
    let bus = Bus::new().await;
    let (bluez, control, state) = setup(&bus).await;
    state.hold.store(true, Ordering::Release);
    let binding = PeerLease::bind(&control, epoch(), DEVICE);
    let change = async {
        bounded(state.started.notified()).await;
        state.connected.store(false, Ordering::Release);
        changed(
            &bluez,
            DEVICE,
            HashMap::from([("Connected".into(), OwnedValue::from(false))]),
            vec![],
        )
        .await;
        state.connected.store(true, Ordering::Release);
        changed(
            &bluez,
            DEVICE,
            HashMap::from([("Connected".into(), OwnedValue::from(true))]),
            vec![],
        )
        .await;
        state.hold.store(false, Ordering::Release);
        state.release.notify_one();
    };
    let (result, ()) = bounded(async { tokio::join!(binding, change) }).await;
    assert!(
        matches!(result, Err(Error::StaleContext)),
        "unexpected result: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn actual_object_removal_and_reexport_cannot_revalidate_same_path() {
    let bus = Bus::new().await;
    let (bluez, control, state) = setup(&bus).await;
    let lease = PeerLease::bind(&control, epoch(), DEVICE).await.unwrap();
    assert!(bounded(bluez.object_server().remove::<Device, _>(DEVICE))
        .await
        .unwrap());
    assert!(bounded(bluez.object_server().at(DEVICE, Device(state)))
        .await
        .unwrap());
    assert_eq!(
        bounded(lease.check_current()).await,
        Err(Error::StaleContext)
    );
}

#[tokio::test]
async fn invalidation_of_connection_or_adapter_fails_closed() {
    for property in ["Connected", "Adapter"] {
        let bus = Bus::new().await;
        let (bluez, control, _state) = setup(&bus).await;
        let lease = PeerLease::bind(&control, epoch(), DEVICE).await.unwrap();
        changed(&bluez, DEVICE, HashMap::new(), vec![property.into()]).await;
        assert_eq!(
            bounded(lease.check_current()).await,
            Err(Error::StaleContext)
        );
    }
}

#[tokio::test]
async fn foreign_sender_cannot_spoof_device_disconnection() {
    let bus = Bus::new().await;
    let (_bluez, control, _state) = setup(&bus).await;
    let lease = PeerLease::bind(&control, epoch(), DEVICE).await.unwrap();
    let stranger = bus.connect().await;
    changed(
        &stranger,
        DEVICE,
        HashMap::from([("Connected".into(), OwnedValue::from(false))]),
        vec![],
    )
    .await;
    bounded(lease.check_current()).await.unwrap();
}

#[tokio::test]
async fn unrelated_trusted_properties_and_other_object_removal_do_not_revoke_peer() {
    let bus = Bus::new().await;
    let (bluez, control, _state) = setup(&bus).await;
    let lease = PeerLease::bind(&control, epoch(), DEVICE).await.unwrap();
    changed(
        &bluez,
        DEVICE,
        HashMap::from([("RSSI".into(), OwnedValue::from(-42i16))]),
        vec![],
    )
    .await;
    bounded(bluez.emit_signal(
        None::<&str>,
        "/",
        "org.freedesktop.DBus.ObjectManager",
        "InterfacesRemoved",
        &(
            OwnedObjectPath::try_from("/org/bluez/hci0/dev_11_22_33_44_55_66").unwrap(),
            vec![DEVICE_INTERFACE],
        ),
    ))
    .await
    .unwrap();
    bounded(lease.check_current()).await.unwrap();
}

#[tokio::test]
async fn disconnected_missing_or_wrong_typed_snapshot_is_never_a_live_lease() {
    let bus = Bus::new().await;
    let (bluez, control, state) = setup(&bus).await;
    state.connected.store(false, Ordering::Release);
    assert!(matches!(
        bounded(PeerLease::bind(&control, epoch(), DEVICE)).await,
        Err(Error::StaleContext)
    ));
    assert!(bluez
        .object_server()
        .remove::<Device, _>(DEVICE)
        .await
        .unwrap());
    assert!(matches!(
        bounded(PeerLease::bind(&control, epoch(), DEVICE)).await,
        Err(Error::Backend)
    ));
    assert!(bluez
        .object_server()
        .at(DEVICE, BrokenDevice)
        .await
        .unwrap());
    assert!(matches!(
        bounded(PeerLease::bind(&control, epoch(), DEVICE)).await,
        Err(Error::Protocol)
    ));
    assert!(bluez
        .object_server()
        .remove::<BrokenDevice, _>(DEVICE)
        .await
        .unwrap());
    assert!(bluez
        .object_server()
        .at(DEVICE, ForeignAdapter)
        .await
        .unwrap());
    assert!(matches!(
        bounded(PeerLease::bind(&control, epoch(), DEVICE)).await,
        Err(Error::StaleContext)
    ));
    assert!(bluez
        .object_server()
        .remove::<ForeignAdapter, _>(DEVICE)
        .await
        .unwrap());
    assert!(bluez
        .object_server()
        .at(DEVICE, MissingConnected)
        .await
        .unwrap());
    assert!(matches!(
        bounded(PeerLease::bind(&control, epoch(), DEVICE)).await,
        Err(Error::Protocol)
    ));
}

#[tokio::test]
async fn invalid_paths_never_get_a_device_binding() {
    let bus = Bus::new().await;
    let (_bluez, control, _state) = setup(&bus).await;
    for path in [
        "/org/bluez/hci1/dev_AA_BB_CC_DD_EE_FF",
        "/org/bluez/hci0/dev_AA",
        "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF/child",
        "/arbitrary",
    ] {
        assert!(matches!(
            bounded(PeerLease::bind(&control, epoch(), path)).await,
            Err(Error::InvalidInput)
        ));
    }
}

#[tokio::test]
async fn owner_retirement_and_same_unique_owner_gap_revoke_device_lease() {
    for gap in [false, true] {
        let bus = Bus::new().await;
        let (bluez, control, _state) = setup(&bus).await;
        let lease = PeerLease::bind(&control, epoch(), DEVICE).await.unwrap();
        if gap {
            assert!(bluez.release_name("org.bluez").await.unwrap());
            bluez.request_name("org.bluez").await.unwrap();
        } else {
            control.retire();
        }
        assert_eq!(
            bounded(lease.check_current()).await,
            Err(Error::StaleContext)
        );
    }
}

#[tokio::test]
async fn cancelling_started_check_retires_but_unpolled_check_has_no_effect() {
    let bus = Bus::new().await;
    let (_bluez, control, _state) = setup(&bus).await;
    let lease = PeerLease::bind(&control, epoch(), DEVICE).await.unwrap();
    drop(lease.check_current());
    bounded(lease.check_current()).await.unwrap();
    let mut checking = Box::pin(lease.check_current());
    assert!(futures_lite::future::poll_once(&mut checking)
        .await
        .is_none());
    drop(checking);
    assert_eq!(
        bounded(lease.check_current()).await,
        Err(Error::StaleContext)
    );
}

#[tokio::test]
async fn stalled_property_query_expires_and_late_snapshot_cannot_revive_lease() {
    let bus = Bus::new().await;
    let (_bluez, control, state) = setup(&bus).await;
    let lease = PeerLease::bind(&control, epoch(), DEVICE).await.unwrap();
    state.hold.store(true, Ordering::Release);
    let result = tokio::time::timeout(Duration::from_secs(7), lease.check_current())
        .await
        .unwrap();
    assert_eq!(result, Err(Error::Deadline));
    state.hold.store(false, Ordering::Release);
    state.release.notify_one();
    assert_eq!(
        bounded(lease.check_current()).await,
        Err(Error::StaleContext)
    );
}

#[cfg(feature = "bluez-gatt")]
pub(crate) async fn install_connected_device(bluez: &Connection) -> Arc<DeviceState> {
    assert!(bluez
        .object_server()
        .at("/", zbus::fdo::ObjectManager)
        .await
        .unwrap());
    let state = DeviceState::connected();
    assert!(bluez
        .object_server()
        .at(DEVICE, Device(state.clone()))
        .await
        .unwrap());
    state
}
#[cfg(feature = "bluez-gatt")]
pub(crate) async fn reconnect_same_path(bluez: &Connection) {
    let interface = bluez
        .object_server()
        .interface::<_, Device>(DEVICE)
        .await
        .unwrap();
    let state = interface.get().await.0.clone();
    state.connected.store(false, Ordering::Release);
    changed(
        bluez,
        DEVICE,
        HashMap::from([("Connected".into(), OwnedValue::from(false))]),
        vec![],
    )
    .await;
    state.connected.store(true, Ordering::Release);
    changed(
        bluez,
        DEVICE,
        HashMap::from([("Connected".into(), OwnedValue::from(true))]),
        vec![],
    )
    .await;
}
#[cfg(feature = "bluez-gatt")]
pub(crate) async fn set_connected(bluez: &Connection, connected: bool) {
    let interface = bluez
        .object_server()
        .interface::<_, Device>(DEVICE)
        .await
        .unwrap();
    let state = interface.get().await.0.clone();
    state.connected.store(connected, Ordering::Release);
    changed(
        bluez,
        DEVICE,
        HashMap::from([("Connected".into(), OwnedValue::from(connected))]),
        vec![],
    )
    .await;
}
#[cfg(feature = "bluez-gatt")]
pub(crate) async fn hold_snapshot(bluez: &Connection, hold: bool) {
    let interface = bluez
        .object_server()
        .interface::<_, Device>(DEVICE)
        .await
        .unwrap();
    let state = interface.get().await.0.clone();
    state.hold.store(hold, Ordering::Release);
    if !hold {
        state.release.notify_one();
    }
}
#[cfg(feature = "bluez-notify-fd")]
pub(crate) async fn wait_held_snapshot(bluez: &Connection) {
    let interface = bluez
        .object_server()
        .interface::<_, Device>(DEVICE)
        .await
        .unwrap();
    let state = interface.get().await.0.clone();
    bounded(state.started.notified()).await;
}
