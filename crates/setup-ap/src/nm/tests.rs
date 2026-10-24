// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::sync::{Arc, Mutex};
const WIFI: &str = "/org/freedesktop/NetworkManager/Devices/1";
const PROFILE: &str = "/org/freedesktop/NetworkManager/Settings/1";
const ACTIVE: &str = "/org/freedesktop/NetworkManager/ActiveConnection/1";
const CP: &str = "/org/freedesktop/NetworkManager/Checkpoint/1";
const IP: &str = "/org/freedesktop/NetworkManager/IP4Config/1";
#[derive(Default)]
struct Model {
    active: bool,
    cp: bool,
    profile: Option<Settings>,
    lost_reply: bool,
    lost_checkpoint_reply: bool,
    changed: bool,
    rollback_consumes: bool,
    events: Vec<&'static str>,
}
type Shared = Arc<Mutex<Model>>;
struct Bus(Shared);
#[zbus::interface(name = "org.freedesktop.DBus")]
impl Bus {
    fn get_name_owner(&self, _name: &str) -> zbus::names::OwnedUniqueName {
        if self.0.lock().unwrap().changed {
            ":1.72"
        } else {
            ":1.71"
        }
        .try_into()
        .unwrap()
    }
}
struct Manager(Shared);
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        vec![WIFI.try_into().unwrap()]
    }
    #[zbus(property)]
    fn networking_enabled(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn checkpoints(&self) -> Vec<OwnedObjectPath> {
        if self.0.lock().unwrap().cp {
            vec![CP.try_into().unwrap()]
        } else {
            vec![]
        }
    }
    fn checkpoint_create(
        &self,
        devices: Vec<OwnedObjectPath>,
        seconds: u32,
        flags: u32,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        assert_eq!(devices, [OwnedObjectPath::try_from(WIFI).unwrap()]);
        assert_eq!(seconds, 330);
        assert_eq!(flags, 0);
        let mut m = self.0.lock().unwrap();
        m.cp = true;
        m.events.push("checkpoint");
        if m.lost_checkpoint_reply {
            return Err(zbus::fdo::Error::Failed(
                "synthetic post-checkpoint error".into(),
            ));
        }
        Ok(CP.try_into().unwrap())
    }
    fn add_and_activate_connection2(
        &self,
        settings: Settings,
        device: OwnedObjectPath,
        specific: OwnedObjectPath,
        options: Properties,
    ) -> zbus::fdo::Result<(OwnedObjectPath, OwnedObjectPath, Properties)> {
        assert_eq!(device.as_str(), WIFI);
        assert_eq!(specific.as_str(), "/");
        assert_eq!(text(&options, "persist").unwrap(), "volatile");
        assert_eq!(text(&options, "bind-activation").unwrap(), "dbus-client");
        assert_eq!(options.len(), 2);
        let mut m = self.0.lock().unwrap();
        m.active = true;
        m.profile = Some(settings);
        m.events.push("activate");
        if m.lost_reply {
            return Err(zbus::fdo::Error::Failed(
                "synthetic post-apply error".into(),
            ));
        }
        Ok((
            PROFILE.try_into().unwrap(),
            ACTIVE.try_into().unwrap(),
            HashMap::new(),
        ))
    }
    fn checkpoint_rollback(&self, cp: OwnedObjectPath) -> HashMap<String, u32> {
        assert_eq!(cp.as_str(), CP);
        let mut m = self.0.lock().unwrap();
        m.active = false;
        m.events.push("rollback");
        if m.rollback_consumes {
            m.cp = false;
        }
        HashMap::from([(WIFI.into(), 0)])
    }
    fn checkpoint_destroy(&self, cp: OwnedObjectPath) {
        assert_eq!(cp.as_str(), CP);
        let mut m = self.0.lock().unwrap();
        assert!(m.cp);
        m.cp = false;
        m.events.push("destroy");
    }
}
struct Device(Shared);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
impl Device {
    #[zbus(property)]
    fn device_type(&self) -> u32 {
        2
    }
    #[zbus(property)]
    fn interface(&self) -> &str {
        INTERFACE
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
        if self.0.lock().unwrap().active {
            ACTIVE
        } else {
            "/"
        }
        .try_into()
        .unwrap()
    }
    #[zbus(property)]
    fn ip4_config(&self) -> OwnedObjectPath {
        if self.0.lock().unwrap().active {
            IP
        } else {
            "/"
        }
        .try_into()
        .unwrap()
    }
}
struct Wireless(Shared);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl Wireless {
    #[zbus(property)]
    fn wireless_capabilities(&self) -> u32 {
        0x40
    }
    #[zbus(property)]
    fn mode(&self) -> u32 {
        if self.0.lock().unwrap().active {
            3
        } else {
            2
        }
    }
}
struct SettingsManager(Shared);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings")]
impl SettingsManager {
    fn list_connections(&self) -> Vec<OwnedObjectPath> {
        if self.0.lock().unwrap().profile.is_some() {
            vec![PROFILE.try_into().unwrap()]
        } else {
            vec![]
        }
    }
}
struct Connection(Shared);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl Connection {
    fn get_settings(&self) -> zbus::fdo::Result<Settings> {
        let m = self.0.lock().unwrap();
        let p = m
            .profile
            .as_ref()
            .ok_or_else(|| zbus::fdo::Error::Failed("missing".into()))?;
        // Real NM does not return secrets from GetSettings; reproduce that boundary.
        let mut copy = HashMap::new();
        for (group, values) in p {
            let mut row = HashMap::new();
            for (k, v) in values {
                if k != "psk" {
                    row.insert(k.clone(), v.try_clone().unwrap());
                }
            }
            copy.insert(group.clone(), row);
        }
        Ok(copy)
    }
    fn delete(&self) {
        let mut m = self.0.lock().unwrap();
        m.profile = None;
        m.events.push("delete");
    }
}
struct Active;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl Active {
    #[zbus(property)]
    fn connection(&self) -> OwnedObjectPath {
        PROFILE.try_into().unwrap()
    }
}
struct Ip;
#[zbus::interface(name = "org.freedesktop.NetworkManager.IP4Config")]
impl Ip {
    #[zbus(property)]
    fn address_data(&self) -> Vec<Properties> {
        vec![HashMap::from([
            ("address".into(), string(ADDRESS)),
            ("prefix".into(), PREFIX.into()),
        ])]
    }
}
async fn fixture(m: Shared) -> (zbus::Connection, NetworkManager) {
    let (a, b) = tokio::net::UnixStream::pair().unwrap();
    let server = zbus::connection::Builder::unix_stream(a)
        .server(zbus::Guid::generate())
        .unwrap()
        .p2p()
        .serve_at("/org/freedesktop/DBus", Bus(m.clone()))
        .unwrap()
        .serve_at(ROOT, Manager(m.clone()))
        .unwrap()
        .serve_at(WIFI, Device(m.clone()))
        .unwrap()
        .serve_at(WIFI, Wireless(m.clone()))
        .unwrap()
        .serve_at(
            "/org/freedesktop/NetworkManager/Settings",
            SettingsManager(m.clone()),
        )
        .unwrap()
        .serve_at(PROFILE, Connection(m))
        .unwrap()
        .serve_at(ACTIVE, Active)
        .unwrap()
        .serve_at(IP, Ip)
        .unwrap();
    let client = zbus::connection::Builder::unix_stream(b).p2p();
    let (server, client) = tokio::join!(server.build(), client.build());
    (server.unwrap(), NetworkManager::new(client.unwrap()))
}
#[tokio::test]
async fn real_dbus_backend_activates_only_volatile_owned_ap_and_removes_it_on_rollback() {
    for (lost_reply, rollback_consumes) in [(false, false), (true, false), (false, true)] {
        let model = Arc::new(Mutex::new(Model {
            lost_reply,
            rollback_consumes,
            ..Default::default()
        }));
        let (_server, mut nm) = fixture(model.clone()).await;
        let c = Credentials::generate().unwrap();
        nm.preflight().await.unwrap();
        nm.checkpoint(330).await.unwrap();
        let activated = nm.activate(&c).await;
        assert_eq!(activated.is_err(), lost_reply);
        if !lost_reply {
            assert!(nm.ready(&c).await.unwrap());
        }
        assert!(nm.activate(&c).await.is_err()); // no replay, even after a lost reply
        nm.rollback(&c).await.unwrap();
        let m = model.lock().unwrap();
        assert!(!m.active && !m.cp && m.profile.is_none());
        assert_eq!(m.events.iter().filter(|x| **x == "activate").count(), 1);
        assert_eq!(
            m.events.iter().filter(|x| **x == "destroy").count(),
            usize::from(!rollback_consumes)
        );
        assert!(m.events.contains(&"delete"));
    }
}
#[tokio::test]
async fn lost_checkpoint_reply_is_not_replayed_or_guessed_from_other_paths() {
    let model = Arc::new(Mutex::new(Model {
        lost_checkpoint_reply: true,
        ..Default::default()
    }));
    let (_server, mut nm) = fixture(model.clone()).await;
    let c = Credentials::generate().unwrap();
    nm.preflight().await.unwrap();
    assert!(nm.checkpoint(330).await.is_err());
    assert!(nm.checkpoint(330).await.is_err());
    assert!(nm.activate(&c).await.is_err());
    assert!(nm.rollback(&c).await.is_err());
    assert_eq!(model.lock().unwrap().events, ["checkpoint"]);
    model.lock().unwrap().cp = false; // Simulate NM's independent finite timer, not a guessed destroy.
    nm.rollback(&c).await.unwrap();
    assert_eq!(model.lock().unwrap().events, ["checkpoint"]);
}
#[tokio::test]
async fn existing_connection_or_owner_change_prevents_new_hardware_work() {
    let model = Arc::new(Mutex::new(Model {
        active: true,
        ..Default::default()
    }));
    let (_s, mut nm) = fixture(model.clone()).await;
    assert!(nm.preflight().await.is_err());
    assert!(model.lock().unwrap().events.is_empty());
    let model = Arc::new(Mutex::new(Model::default()));
    let (_s, mut nm) = fixture(model.clone()).await;
    nm.preflight().await.unwrap();
    model.lock().unwrap().changed = true;
    assert!(nm.checkpoint(330).await.is_err());
    assert!(model.lock().unwrap().events.is_empty());
}
#[test]
fn ap_settings_are_fixed_and_subnet_collision_is_not_guessed_away() {
    let c = Credentials::generate().unwrap();
    let mut s = settings(&c).unwrap();
    assert!(profile_matches(&s, &c).unwrap());
    assert_eq!(text(&s["802-11-wireless"], "mode").unwrap(), "ap");
    assert!(!boolean(&s["connection"], "autoconnect").unwrap());
    assert_eq!(text(&s["ipv4"], "method").unwrap(), "shared");
    assert_eq!(text(&s["ipv6"], "method").unwrap(), "disabled");
    s.get_mut("connection")
        .unwrap()
        .insert("autoconnect".into(), true.into());
    assert!(!profile_matches(&s, &c).unwrap());
    assert!(subnet_conflict("172.31.1.1", 16).unwrap());
    assert!(subnet_conflict("172.31.85.240", 32).unwrap());
    assert!(!subnet_conflict("192.168.120.2", 24).unwrap());
    assert!(subnet_conflict("invalid", 24).is_err());
    assert!(subnet_conflict("10.0.0.1", 33).is_err());
}
