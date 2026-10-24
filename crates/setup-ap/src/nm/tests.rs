// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::sync::{Arc, Mutex};
const WIFI: &str = "/org/freedesktop/NetworkManager/Devices/1";
const PROFILE: &str = "/org/freedesktop/NetworkManager/Settings/1";
const ACTIVE: &str = "/org/freedesktop/NetworkManager/ActiveConnection/1";
const CP: &str = "/org/freedesktop/NetworkManager/Checkpoint/1";
const IP: &str = "/org/freedesktop/NetworkManager/IP4Config/1";
const STATION_PROFILE: &str = "/org/freedesktop/NetworkManager/Settings/2";
const STATION_ACTIVE: &str = "/org/freedesktop/NetworkManager/ActiveConnection/2";
const STATION_CP: &str = "/org/freedesktop/NetworkManager/Checkpoint/2";
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
    retain_ap: bool,
    station_profile: Option<Settings>,
    station_active: bool,
    station_cp: bool,
    lost_station_reply: bool,
    lost_station_cp_reply: bool,
    fail_station_rollback: bool,
    tamper_station: bool,
    invalid_station_ip: bool,
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
        let m = self.0.lock().unwrap();
        let mut v = Vec::new();
        if m.cp {
            v.push(CP.try_into().unwrap());
        }
        if m.station_cp {
            v.push(STATION_CP.try_into().unwrap());
        }
        v
    }
    fn checkpoint_create(
        &self,
        devices: Vec<OwnedObjectPath>,
        seconds: u32,
        flags: u32,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        assert_eq!(devices, [OwnedObjectPath::try_from(WIFI).unwrap()]);
        let mut m = self.0.lock().unwrap();
        if flags == 8 {
            assert_eq!(seconds, 45);
            assert!(m.cp && m.active && m.retain_ap);
            m.station_cp = true;
            m.events.push("station-checkpoint");
            if m.lost_station_cp_reply {
                return Err(zbus::fdo::Error::Failed(
                    "synthetic lost station checkpoint reply".into(),
                ));
            }
            return Ok(STATION_CP.try_into().unwrap());
        }
        assert_eq!(seconds, 330);
        assert_eq!(flags, 0);
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
        assert_eq!(text(&options, "bind-activation").unwrap(), "dbus-client");
        assert_eq!(options.len(), 2);
        let mut m = self.0.lock().unwrap();
        if text(&settings["802-11-wireless"], "mode").unwrap() == "infrastructure" {
            assert!(m.station_cp && m.cp && m.active && m.retain_ap);
            assert_eq!(text(&options, "persist").unwrap(), "memory");
            m.active = false;
            m.station_active = true;
            m.station_profile = Some(settings);
            m.events.push("station-activate");
            if m.lost_station_reply {
                return Err(zbus::fdo::Error::Failed(
                    "synthetic lost station activation reply".into(),
                ));
            }
            return Ok((
                STATION_PROFILE.try_into().unwrap(),
                STATION_ACTIVE.try_into().unwrap(),
                HashMap::new(),
            ));
        }
        assert_eq!(
            text(&options, "persist").unwrap(),
            if m.retain_ap { "memory" } else { "volatile" }
        );
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
    fn checkpoint_rollback(&self, cp: OwnedObjectPath) -> zbus::fdo::Result<HashMap<String, u32>> {
        let mut m = self.0.lock().unwrap();
        if cp.as_str() == STATION_CP {
            if m.fail_station_rollback {
                return Err(zbus::fdo::Error::Failed(
                    "synthetic rollback failure".into(),
                ));
            }
            m.active = true;
            m.station_active = false;
            m.events.push("station-rollback");
            if m.rollback_consumes {
                m.station_cp = false;
            }
            return Ok(HashMap::from([(WIFI.into(), 0)]));
        }
        assert_eq!(cp.as_str(), CP);
        m.active = false;
        m.events.push("rollback");
        if m.rollback_consumes {
            m.cp = false;
        }
        Ok(HashMap::from([(WIFI.into(), 0)]))
    }
    fn checkpoint_destroy(&self, cp: OwnedObjectPath) {
        let mut m = self.0.lock().unwrap();
        if cp.as_str() == STATION_CP {
            assert!(m.station_cp);
            m.station_cp = false;
            m.events.push("station-destroy");
            return;
        }
        assert_eq!(cp.as_str(), CP);
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
        let m = self.0.lock().unwrap();
        if m.active || m.station_active {
            100
        } else {
            30
        }
    }
    #[zbus(property)]
    fn active_connection(&self) -> OwnedObjectPath {
        let m = self.0.lock().unwrap();
        if m.station_active {
            STATION_ACTIVE
        } else if m.active {
            ACTIVE
        } else {
            "/"
        }
        .try_into()
        .unwrap()
    }
    #[zbus(property)]
    fn ip4_config(&self) -> OwnedObjectPath {
        let m = self.0.lock().unwrap();
        if m.active || m.station_active {
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
        let m = self.0.lock().unwrap();
        let mut v = Vec::new();
        if m.profile.is_some() {
            v.push(PROFILE.try_into().unwrap());
        }
        if m.station_profile.is_some() {
            v.push(STATION_PROFILE.try_into().unwrap());
        }
        v
    }
}
struct Connection(Shared, bool);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl Connection {
    fn get_settings(&self) -> zbus::fdo::Result<Settings> {
        let m = self.0.lock().unwrap();
        let p = (if self.1 {
            &m.station_profile
        } else {
            &m.profile
        })
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
        if self.1 && m.tamper_station {
            copy.get_mut("connection").unwrap().insert(
                "uuid".into(),
                string("00000000-0000-0000-0000-000000000000"),
            );
        }
        Ok(copy)
    }
    fn delete(&self) {
        let mut m = self.0.lock().unwrap();
        if self.1 {
            assert!(!m.station_active);
            m.station_profile = None;
            m.events.push("station-delete");
            return;
        }
        m.profile = None;
        m.events.push("delete");
    }
}
struct Active(&'static str);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl Active {
    #[zbus(property)]
    fn connection(&self) -> OwnedObjectPath {
        self.0.try_into().unwrap()
    }
}
struct Ip(Shared);
#[zbus::interface(name = "org.freedesktop.NetworkManager.IP4Config")]
impl Ip {
    #[zbus(property)]
    fn address_data(&self) -> Vec<Properties> {
        let m = self.0.lock().unwrap();
        vec![HashMap::from([
            (
                "address".into(),
                string(if m.station_active && !m.invalid_station_ip {
                    "192.168.88.20"
                } else {
                    ADDRESS
                }),
            ),
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
        .serve_at(PROFILE, Connection(m.clone(), false))
        .unwrap()
        .serve_at(STATION_PROFILE, Connection(m.clone(), true))
        .unwrap()
        .serve_at(ACTIVE, Active(PROFILE))
        .unwrap()
        .serve_at(STATION_ACTIVE, Active(STATION_PROFILE))
        .unwrap()
        .serve_at(IP, Ip(m))
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
async fn prepared_station_fixture(
    model: Shared,
) -> (zbus::Connection, NetworkManager, Credentials) {
    model.lock().unwrap().retain_ap = true;
    let (server, mut nm) = fixture(model).await;
    nm.retain_ap_for_station_trial().unwrap();
    let ap = Credentials::generate().unwrap();
    nm.preflight().await.unwrap();
    nm.checkpoint(330).await.unwrap();
    nm.activate(&ap).await.unwrap();
    assert!(nm.ready(&ap).await.unwrap());
    (server, nm, ap)
}
fn station_credentials() -> crate::station_trial::StationCredentials {
    crate::station_trial::StationCredentials::new(
        "fixture station".into(),
        zeroize::Zeroizing::new("fixture-password".into()),
    )
    .unwrap()
}
#[tokio::test]
async fn real_dbus_station_test_restores_same_ap_and_deletes_only_its_candidate() {
    use crate::station_trial::StationNetwork;
    for (lost_reply, rollback_consumes) in [(false, false), (true, false), (false, true)] {
        let m = Arc::new(Mutex::new(Model {
            lost_station_reply: lost_reply,
            rollback_consumes,
            ..Default::default()
        }));
        let (_server, mut nm, ap) = prepared_station_fixture(m.clone()).await;
        let c = station_credentials();
        {
            let mut trial = nm.station_trial(&ap).unwrap();
            assert_eq!(trial.begin(&c).await.is_err(), lost_reply);
            assert!(trial.begin(&c).await.is_err());
            if !lost_reply {
                assert!(trial.usable(&c).await.unwrap());
            }
            trial.restore_ap(&c).await.unwrap();
        }
        assert!(nm.ready(&ap).await.unwrap());
        {
            let s = m.lock().unwrap();
            assert!(s.cp && s.active && !s.station_cp && s.station_profile.is_none());
            assert_eq!(
                s.events
                    .iter()
                    .filter(|e| **e == "station-activate")
                    .count(),
                1
            );
            assert!(s.events.contains(&"station-delete"));
            assert!(!s.events.contains(&"destroy"));
        }
        nm.rollback(&ap).await.unwrap();
        assert!(!m.lock().unwrap().cp);
    }
}
#[tokio::test]
async fn lost_station_checkpoint_reply_keeps_unknown_until_actual_timer_restores_ap() {
    use crate::station_trial::StationNetwork;
    let m = Arc::new(Mutex::new(Model {
        lost_station_cp_reply: true,
        ..Default::default()
    }));
    let (_server, mut nm, ap) = prepared_station_fixture(m.clone()).await;
    let c = station_credentials();
    let mut trial = nm.station_trial(&ap).unwrap();
    assert!(trial.begin(&c).await.is_err());
    assert!(trial.restore_ap(&c).await.is_err());
    assert!(trial.begin(&c).await.is_err());
    assert!(!m.lock().unwrap().events.contains(&"station-activate"));
    m.lock().unwrap().station_cp = false; // Independent NM timer, never a guessed checkpoint destroy.
    trial.restore_ap(&c).await.unwrap();
    assert!(!m.lock().unwrap().events.contains(&"station-rollback"));
}
#[tokio::test]
async fn wrong_station_subnet_or_failed_rollback_is_never_reported_as_a_success() {
    use crate::station_trial::StationNetwork;
    let m = Arc::new(Mutex::new(Model {
        invalid_station_ip: true,
        ..Default::default()
    }));
    let (_server, mut nm, ap) = prepared_station_fixture(m.clone()).await;
    let c = station_credentials();
    {
        let mut trial = nm.station_trial(&ap).unwrap();
        trial.begin(&c).await.unwrap();
        assert!(trial.usable(&c).await.is_err());
        m.lock().unwrap().fail_station_rollback = true;
        assert!(trial.restore_ap(&c).await.is_err());
    }
    assert!(nm.rollback(&ap).await.is_err());
    assert!(m.lock().unwrap().station_profile.is_some());
    m.lock().unwrap().fail_station_rollback = false;
    nm.station_trial(&ap).unwrap().restore_ap(&c).await.unwrap();
}
#[tokio::test]
async fn station_owner_or_profile_change_prevents_unsafe_followup_writes() {
    use crate::station_trial::StationNetwork;
    let m = Arc::new(Mutex::new(Model::default()));
    let (_server, mut nm, ap) = prepared_station_fixture(m.clone()).await;
    let c = station_credentials();
    m.lock().unwrap().changed = true;
    assert!(nm.station_trial(&ap).unwrap().begin(&c).await.is_err());
    assert!(!m.lock().unwrap().events.contains(&"station-checkpoint"));
    m.lock().unwrap().changed = false;
    let mut trial = nm.station_trial(&ap).unwrap();
    trial.begin(&c).await.unwrap();
    let wrong = station_credentials();
    assert!(trial.restore_ap(&wrong).await.is_err());
    assert!(!m.lock().unwrap().events.contains(&"station-rollback"));
    m.lock().unwrap().tamper_station = true;
    assert!(trial.usable(&c).await.is_err());
    assert!(trial.restore_ap(&c).await.is_err());
    assert!(!m.lock().unwrap().events.contains(&"station-delete"));
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
#[test]
fn restoring_ap_also_requires_the_original_isolation_ciphers_and_address_policy() {
    let c = Credentials::generate().unwrap();
    for (section, key, value) in [
        ("802-11-wireless", "ap-isolation", 0i32.into()),
        ("802-11-wireless", "band", string("a")),
        (
            "802-11-wireless-security",
            "pairwise",
            strings(vec!["tkip"]).unwrap(),
        ),
        (
            "802-11-wireless-security",
            "group",
            strings(vec!["tkip"]).unwrap(),
        ),
        ("802-11-wireless-security", "psk-flags", 1u32.into()),
        ("ipv4", "may-fail", true.into()),
    ] {
        let mut s = settings(&c).unwrap();
        s.get_mut(section).unwrap().insert(key.into(), value);
        assert!(!profile_matches(&s, &c).unwrap());
    }
}
