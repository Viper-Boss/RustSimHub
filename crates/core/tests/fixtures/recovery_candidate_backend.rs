// SPDX-License-Identifier: AGPL-3.0-only
//! Private external BlueZ/NM/MM endpoints only. Never replaces Core business,
//! its locks, durable journal, cryptographic sessions, verifier or sealer.
use super::*;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
pub const DEVICE: &str = "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF";
const NM: &str = "/org/freedesktop/NetworkManager";
const WIFI: &str = "/org/freedesktop/NetworkManager/Devices/0";
const PROFILE: &str = "/org/freedesktop/NetworkManager/Settings/1";
const ACTIVE: &str = "/org/freedesktop/NetworkManager/ActiveConnection/0";
const CHECKPOINT: &str = "/org/freedesktop/NetworkManager/Checkpoint/0";
const ACCESS: &str = "/org/freedesktop/NetworkManager/AccessPoint/0";
const IPV4: &str = "/org/freedesktop/NetworkManager/IP4Config/0";
type Settings = HashMap<String, HashMap<String, OwnedValue>>;
fn path(value: &str) -> OwnedObjectPath {
    value.try_into().unwrap()
}
fn copy_settings(value: &Settings) -> Settings {
    value
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                v.iter()
                    .map(|(k, v)| (k.clone(), v.try_clone().unwrap()))
                    .collect(),
            )
        })
        .collect()
}
#[derive(Default)]
struct State {
    checkpoint: bool,
    active: bool,
    exists: bool,
    saved: bool,
    settings: Settings,
    creates: usize,
    saves: usize,
    rollbacks: usize,
    deletes: usize,
    unregister_application: usize,
    unregister_advertisement: usize,
}
type Shared = Arc<Mutex<State>>;
struct Gatt(Shared);
#[zbus::interface(name = "org.bluez.GattManager1")]
impl Gatt {
    fn register_application(
        &self,
        value: OwnedObjectPath,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) {
        assert_eq!(value.as_str(), rustsimhub_ble::manifest::ROOT);
        assert!(options.is_empty());
        write(
            "backend/service.json",
            &serde_json::json!({"name":header.sender().unwrap().as_str()}),
        );
    }
    fn unregister_application(&self, value: OwnedObjectPath) {
        assert_eq!(value.as_str(), rustsimhub_ble::manifest::ROOT);
        self.0.lock().unwrap().unregister_application += 1;
    }
}
struct Advertising(Shared);
#[zbus::interface(name = "org.bluez.LEAdvertisingManager1")]
impl Advertising {
    fn register_advertisement(&self, value: OwnedObjectPath, options: HashMap<String, OwnedValue>) {
        assert_eq!(value.as_str(), rustsimhub_ble::manifest::ADVERTISEMENT_PATH);
        assert!(options.is_empty());
        write("backend/advertising", &serde_json::json!(true));
    }
    fn unregister_advertisement(&self, value: OwnedObjectPath) {
        assert_eq!(value.as_str(), rustsimhub_ble::manifest::ADVERTISEMENT_PATH);
        self.0.lock().unwrap().unregister_advertisement += 1;
    }
}
struct Device(Arc<AtomicBool>);
#[zbus::interface(name = "org.bluez.Device1")]
impl Device {
    #[zbus(property)]
    fn connected(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    #[zbus(property)]
    fn adapter(&self) -> OwnedObjectPath {
        path("/org/bluez/hci0")
    }
}
struct Manager(Shared);
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    #[zbus(property)]
    fn wwan_enabled(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn checkpoints(&self) -> Vec<OwnedObjectPath> {
        if self.0.lock().unwrap().checkpoint {
            vec![path(CHECKPOINT)]
        } else {
            vec![]
        }
    }
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        vec![path(WIFI)]
    }
    fn checkpoint_create(
        &self,
        devices: Vec<OwnedObjectPath>,
        timeout: u32,
        flags: u32,
    ) -> OwnedObjectPath {
        assert_eq!(devices, vec![path(WIFI)]);
        assert_eq!(timeout, 45);
        assert_eq!(flags, 0);
        let mut s = self.0.lock().unwrap();
        assert!(!s.checkpoint);
        s.checkpoint = true;
        path(CHECKPOINT)
    }
    fn add_and_activate_connection2(
        &self,
        settings: Settings,
        device: OwnedObjectPath,
        specific: OwnedObjectPath,
        options: HashMap<String, OwnedValue>,
    ) -> (
        OwnedObjectPath,
        OwnedObjectPath,
        HashMap<String, OwnedValue>,
    ) {
        assert_eq!(device.as_str(), WIFI);
        assert_eq!(specific.as_str(), "/");
        assert_eq!(<&str>::try_from(&options["persist"]).unwrap(), "volatile");
        assert_eq!(
            <&str>::try_from(&options["bind-activation"]).unwrap(),
            "dbus-client"
        );
        assert_eq!(
            <&str>::try_from(&settings["connection"]["type"]).unwrap(),
            "802-11-wireless"
        );
        assert_eq!(
            <&str>::try_from(&settings["connection"]["interface-name"]).unwrap(),
            "wlan0"
        );
        let mut s = self.0.lock().unwrap();
        assert!(!s.exists);
        s.settings = settings;
        s.exists = true;
        s.active = true;
        s.creates += 1;
        (path(PROFILE), path(ACTIVE), HashMap::new())
    }
    fn checkpoint_destroy(&self, checkpoint: OwnedObjectPath) {
        assert_eq!(checkpoint.as_str(), CHECKPOINT);
        let mut s = self.0.lock().unwrap();
        assert!(s.checkpoint);
        s.checkpoint = false;
    }
    fn checkpoint_rollback(&self, checkpoint: OwnedObjectPath) -> HashMap<String, u32> {
        assert_eq!(checkpoint.as_str(), CHECKPOINT);
        let mut s = self.0.lock().unwrap();
        assert!(s.checkpoint);
        s.active = false;
        s.rollbacks += 1;
        HashMap::from([(WIFI.to_owned(), 0)])
    }
}
struct Wifi(Shared);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
impl Wifi {
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
        if self.0.lock().unwrap().active {
            100
        } else {
            30
        }
    }
    #[zbus(property)]
    fn active_connection(&self) -> OwnedObjectPath {
        path(if self.0.lock().unwrap().active {
            ACTIVE
        } else {
            "/"
        })
    }
    #[zbus(property)]
    fn ip4_config(&self) -> OwnedObjectPath {
        path(IPV4)
    }
}
struct Wireless;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl Wireless {
    #[zbus(property)]
    fn active_access_point(&self) -> OwnedObjectPath {
        path(ACCESS)
    }
}
struct Active;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl Active {
    #[zbus(property)]
    fn connection(&self) -> OwnedObjectPath {
        path(PROFILE)
    }
}
struct Access;
#[zbus::interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
impl Access {
    #[zbus(property)]
    fn ssid(&self) -> Vec<u8> {
        b"SYNTHETIC_NATIVE".to_vec()
    }
}
struct Ip;
#[zbus::interface(name = "org.freedesktop.NetworkManager.IP4Config")]
impl Ip {
    #[zbus(property)]
    fn address_data(&self) -> Vec<HashMap<String, OwnedValue>> {
        vec![HashMap::from([(
            "address".into(),
            zbus::zvariant::Value::from("192.0.2.2").try_into().unwrap(),
        )])]
    }
}
struct SettingsRoot(Shared);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings")]
impl SettingsRoot {
    fn list_connections(&self) -> Vec<OwnedObjectPath> {
        if self.0.lock().unwrap().exists {
            vec![path(PROFILE)]
        } else {
            vec![]
        }
    }
}
struct Profile(Shared);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl Profile {
    fn get_settings(&self) -> Settings {
        copy_settings(&self.0.lock().unwrap().settings)
    }
    #[zbus(property)]
    fn unsaved(&self) -> bool {
        !self.0.lock().unwrap().saved
    }
    #[zbus(property)]
    fn flags(&self) -> u32 {
        if self.0.lock().unwrap().saved {
            0
        } else {
            4
        }
    }
    async fn update2(
        &self,
        settings: Settings,
        flags: u32,
        options: HashMap<String, OwnedValue>,
    ) -> HashMap<String, OwnedValue> {
        assert_eq!(flags, 1);
        assert!(options.is_empty());
        {
            let mut s = self.0.lock().unwrap();
            assert!(s.exists);
            s.settings = settings;
            s.saved = true;
            s.saves += 1;
        }
        if case() == "revoke_after_save" {
            write("control/revoke", &serde_json::json!(true));
            wait("core/revoked").await;
        }
        HashMap::new()
    }
    fn delete(&self) {
        let mut s = self.0.lock().unwrap();
        assert!(s.exists);
        s.exists = false;
        s.saved = false;
        s.deletes += 1;
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

pub async fn run() {
    let address = std::env::var("DBUS_SYSTEM_BUS_ADDRESS").unwrap();
    let bus = zbus::connection::Builder::address(address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let state = Arc::new(Mutex::new(State::default()));
    let connected = Arc::new(AtomicBool::new(true));
    bus.object_server()
        .at("/org/bluez/hci0", Gatt(state.clone()))
        .await
        .unwrap();
    bus.object_server()
        .at("/org/bluez/hci0", Advertising(state.clone()))
        .await
        .unwrap();
    bus.object_server()
        .at(DEVICE, Device(connected.clone()))
        .await
        .unwrap();
    bus.object_server()
        .at(NM, Manager(state.clone()))
        .await
        .unwrap();
    bus.object_server()
        .at(WIFI, Wifi(state.clone()))
        .await
        .unwrap();
    bus.object_server().at(WIFI, Wireless).await.unwrap();
    bus.object_server().at(ACTIVE, Active).await.unwrap();
    bus.object_server().at(ACCESS, Access).await.unwrap();
    bus.object_server().at(IPV4, Ip).await.unwrap();
    bus.object_server()
        .at(format!("{NM}/Settings"), SettingsRoot(state.clone()))
        .await
        .unwrap();
    bus.object_server()
        .at(PROFILE, Profile(state.clone()))
        .await
        .unwrap();
    bus.object_server()
        .at("/org/freedesktop/ModemManager1/Modem/0", Voice)
        .await
        .unwrap();
    for name in [
        "org.bluez",
        "org.freedesktop.NetworkManager",
        "org.freedesktop.ModemManager1",
    ] {
        bus.request_name(name).await.unwrap();
    }
    write("backend/ready.json", &serde_json::json!(true));
    wait("backend/advertising").await;
    let peer = peer::run(&bus).await;
    connected.store(false, Ordering::Release);
    let iface = bus
        .object_server()
        .interface::<_, Device>(DEVICE)
        .await
        .unwrap();
    iface
        .get()
        .await
        .connected_changed(iface.signal_emitter())
        .await
        .unwrap();
    wait("ble/done.json").await;
    peer.closed().await;
    wait("core/done.json").await;
    let s = state.lock().unwrap();
    if matches!(case().as_str(), "wrong_pin" | "tamper") {
        assert_eq!(s.creates, 0);
    } else {
        assert_eq!(s.creates, 1);
    }
    if case() == "confirm" {
        assert_eq!(s.saves, 1);
        assert_eq!(s.rollbacks, 0);
        assert!(s.saved);
    } else if case() == "revoke_after_save" {
        assert_eq!(s.saves, 1);
        assert_eq!(s.rollbacks, 1);
        assert!(!s.exists);
    } else {
        assert_eq!(s.saves, 0);
        if s.creates == 1 {
            assert_eq!(s.rollbacks, 1);
            assert!(!s.exists);
        }
    }
    assert!(!s.checkpoint);
    assert_eq!(s.unregister_application, 1);
    assert_eq!(s.unregister_advertisement, 1);
    write(
        "backend/done.json",
        &serde_json::json!({"uid":1003,"case":case(),"external_backends_only":true,"creates":s.creates,"saves":s.saves,"rollbacks":s.rollbacks,"deletes":s.deletes,"gatt_and_advertisement_unregistered":true,"wwan_or_gsm_mutation":false}),
    );
}
