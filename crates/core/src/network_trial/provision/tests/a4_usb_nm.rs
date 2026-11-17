// SPDX-License-Identifier: AGPL-3.0-only
//! Original ordinary USB Wi-Fi API and NM adapter on a private session bus.
//! No AP/recovery/scan methods, system bus, real network or device is used.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_policy::Kind;
use std::os::unix::fs::MetadataExt;

const WIFI: &str = "/org/freedesktop/NetworkManager/Devices/1";
const USB: &str = "/org/freedesktop/NetworkManager/Devices/2";
const OLD: &str = "/org/freedesktop/NetworkManager/Settings/1";
const USB_PROFILE: &str = "/org/freedesktop/NetworkManager/Settings/2";
const NEW: &str = "/org/freedesktop/NetworkManager/Settings/3";
const POINT: &str = "/org/freedesktop/NetworkManager/Checkpoint/1";
const ACCESS: &str = "/org/freedesktop/NetworkManager/AccessPoint/1";
const IP: &str = "/org/freedesktop/NetworkManager/IP4Config/1";
fn path(s: &str) -> OwnedObjectPath {
    s.try_into().unwrap()
}
fn active(n: u8) -> OwnedObjectPath {
    path(&format!(
        "/org/freedesktop/NetworkManager/ActiveConnection/{n}"
    ))
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Save,
    Cancel,
    Renamed,
    LostSaveReply,
    Revoked,
    NoUsb,
    NoIp,
    LostActivationReply,
    HiddenFlagLost,
}
struct Data {
    mode: Mode,
    hidden: bool,
    cp: bool,
    active: bool,
    profile: Option<Settings>,
    unsaved: bool,
    effects: Vec<&'static str>,
    revoke: Option<(Arc<Mutex<Auth>>, String, [u8; 32])>,
}
type Shared = Arc<Mutex<Data>>;
fn cloned(s: &Settings) -> Settings {
    s.iter()
        .map(|(key, values)| {
            (
                key.clone(),
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), value.try_clone().unwrap()))
                    .collect(),
            )
        })
        .collect()
}
struct Manager(Shared);
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        vec![path(WIFI), path(USB)]
    }
    #[zbus(property)]
    fn wwan_enabled(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn checkpoints(&self) -> Vec<OwnedObjectPath> {
        if self.0.lock().unwrap().cp {
            vec![path(POINT)]
        } else {
            vec![]
        }
    }
    fn checkpoint_create(
        &self,
        devices: Vec<OwnedObjectPath>,
        seconds: u32,
        flags: u32,
    ) -> OwnedObjectPath {
        assert_eq!(devices, vec![path(WIFI)]);
        assert_eq!((seconds, flags), (45, 0));
        let mut s = self.0.lock().unwrap();
        assert!(!s.cp);
        s.cp = true;
        s.effects.push("checkpoint");
        path(POINT)
    }
    fn checkpoint_destroy(&self, checkpoint: OwnedObjectPath) {
        assert_eq!(checkpoint, path(POINT));
        let mut s = self.0.lock().unwrap();
        assert!(s.cp);
        s.cp = false;
        s.effects.push("destroy");
    }
    fn checkpoint_rollback(&self, checkpoint: OwnedObjectPath) -> HashMap<String, u32> {
        assert_eq!(checkpoint, path(POINT));
        let mut s = self.0.lock().unwrap();
        s.active = false;
        s.effects.push("rollback");
        HashMap::from([(WIFI.into(), 0)])
    }
    fn activate_connection(
        &self,
        profile: OwnedObjectPath,
        device: OwnedObjectPath,
        specific: OwnedObjectPath,
    ) -> OwnedObjectPath {
        assert_eq!(
            (profile, device, specific),
            (path(OLD), path(WIFI), path("/"))
        );
        let mut s = self.0.lock().unwrap();
        s.active = false;
        s.effects.push("restore-old");
        active(1)
    }
    fn add_and_activate_connection2(
        &self,
        settings: Settings,
        device: OwnedObjectPath,
        specific: OwnedObjectPath,
        options: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<(
        OwnedObjectPath,
        OwnedObjectPath,
        HashMap<String, OwnedValue>,
    )> {
        assert_eq!((device, specific), (path(WIFI), path("/")));
        assert_eq!(<&str>::try_from(&options["persist"]).unwrap(), "volatile");
        assert_eq!(
            <&str>::try_from(&options["bind-activation"]).unwrap(),
            "dbus-client"
        );
        assert_eq!(
            <&str>::try_from(&settings["802-11-wireless"]["mode"]).unwrap(),
            "infrastructure"
        );
        assert_eq!(
            <&str>::try_from(&settings["802-11-wireless-security"]["key-mgmt"]).unwrap(),
            "wpa-psk"
        );
        assert!(!settings.contains_key("gsm") && !settings.contains_key("cdma"));
        let mut s = self.0.lock().unwrap();
        s.effects.push("activate");
        assert_eq!(
            bool::try_from(&settings["802-11-wireless"]["hidden"]).unwrap(),
            s.hidden
        );
        s.profile = Some(settings);
        s.active = true;
        s.unsaved = true;
        if s.mode == Mode::LostActivationReply {
            return Err(zbus::fdo::Error::Failed(
                "synthetic reply loss after activation".into(),
            ));
        }
        Ok((path(NEW), active(3), HashMap::new()))
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
struct Device {
    data: Shared,
    usb: bool,
}
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
impl Device {
    #[zbus(property)]
    fn device_type(&self) -> u32 {
        if self.usb {
            1
        } else {
            2
        }
    }
    #[zbus(property)]
    fn interface(&self) -> &str {
        if self.usb {
            "usb0"
        } else {
            "wlan0"
        }
    }
    #[zbus(property)]
    fn managed(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        if self.usb && self.data.lock().unwrap().mode == Mode::NoUsb {
            30
        } else {
            100
        }
    }
    #[zbus(property)]
    fn active_connection(&self) -> OwnedObjectPath {
        if self.usb {
            active(2)
        } else if self.data.lock().unwrap().active {
            active(3)
        } else {
            active(1)
        }
    }
    #[zbus(property)]
    fn ip4_config(&self) -> OwnedObjectPath {
        if self.data.lock().unwrap().mode == Mode::NoIp {
            path("/")
        } else {
            path(IP)
        }
    }
}
struct Active(&'static str);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl Active {
    #[zbus(property)]
    fn connection(&self) -> OwnedObjectPath {
        path(self.0)
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
struct Access(Shared);
#[zbus::interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
impl Access {
    #[zbus(property)]
    fn ssid(&self) -> Vec<u8> {
        let s = self.0.lock().unwrap();
        if s.mode == Mode::Renamed {
            b"different-network".to_vec()
        } else {
            s.profile
                .as_ref()
                .map(|p| {
                    Vec::<u8>::try_from(p["802-11-wireless"]["ssid"].try_clone().unwrap()).unwrap()
                })
                .unwrap_or_else(|| b"old-network".to_vec())
        }
    }
}
struct Ip;
#[zbus::interface(name = "org.freedesktop.NetworkManager.IP4Config")]
impl Ip {
    #[zbus(property)]
    fn address_data(&self) -> Vec<HashMap<String, OwnedValue>> {
        vec![HashMap::from([
            ("address".into(), string("192.0.2.2")),
            ("prefix".into(), OwnedValue::from(24u32)),
        ])]
    }
}
struct Profiles(Shared);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings")]
impl Profiles {
    fn list_connections(&self) -> Vec<OwnedObjectPath> {
        let mut rows = vec![path(OLD), path(USB_PROFILE)];
        if self.0.lock().unwrap().profile.is_some() {
            rows.push(path(NEW))
        };
        rows
    }
}
struct Profile {
    data: Shared,
    role: u8,
}
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl Profile {
    fn get_settings(&self) -> Settings {
        if self.role == 3 {
            let s = self.data.lock().unwrap();
            let mut profile = cloned(s.profile.as_ref().unwrap());
            if !s.hidden || s.mode == Mode::HiddenFlagLost {
                profile.get_mut("802-11-wireless").unwrap().remove("hidden");
            }
            return profile;
        }
        HashMap::from([
            (
                "connection".into(),
                HashMap::from([
                    (
                        "uuid".into(),
                        string(if self.role == 1 {
                            "11111111-1111-4111-8111-111111111111"
                        } else {
                            "22222222-2222-4222-8222-222222222222"
                        }),
                    ),
                    (
                        "type".into(),
                        string(if self.role == 1 {
                            "802-11-wireless"
                        } else {
                            "802-3-ethernet"
                        }),
                    ),
                    ("autoconnect".into(), OwnedValue::from(false)),
                ]),
            ),
            (
                "802-11-wireless".into(),
                HashMap::from([(
                    "ssid".into(),
                    OwnedValue::try_from(zbus::zvariant::Value::from(b"old-network".to_vec()))
                        .unwrap(),
                )]),
            ),
        ])
    }
    #[zbus(property)]
    fn unsaved(&self) -> bool {
        self.role == 3 && self.data.lock().unwrap().unsaved
    }
    #[zbus(property)]
    fn flags(&self) -> u32 {
        if self.unsaved() {
            4
        } else {
            0
        }
    }
    fn update2(
        &self,
        settings: Settings,
        flags: u32,
        options: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<HashMap<String, OwnedValue>> {
        assert_eq!(self.role, 3);
        assert_eq!(flags, 1);
        assert!(options.is_empty());
        assert!(bool::try_from(&settings["connection"]["autoconnect"]).unwrap());
        let (mode, revoke) = {
            let mut s = self.data.lock().unwrap();
            assert_eq!(
                bool::try_from(&settings["802-11-wireless"]["hidden"]).unwrap(),
                s.hidden
            );
            s.profile = Some(settings);
            s.unsaved = false;
            s.effects.push("save");
            (s.mode, s.revoke.take())
        };
        if let Some((auth, owner, key)) = revoke {
            auth.lock().unwrap().revoke(&owner, key).unwrap();
        }
        if mode == Mode::LostSaveReply {
            return Err(zbus::fdo::Error::Failed(
                "synthetic lost persistence reply".into(),
            ));
        };
        Ok(HashMap::new())
    }
    fn delete(&self) {
        assert_eq!(self.role, 3);
        let mut s = self.data.lock().unwrap();
        s.profile = None;
        s.effects.push("delete-new");
    }
}
async fn server(data: Shared) -> zbus::Connection {
    let address = std::env::var("DBUS_SYSTEM_BUS_ADDRESS").unwrap();
    let mut b = zbus::connection::Builder::address(address.as_str())
        .unwrap()
        .name(NM)
        .unwrap()
        .name(MM)
        .unwrap()
        .serve_at(ROOT, Manager(data.clone()))
        .unwrap()
        .serve_at("/org/freedesktop/ModemManager1/Modem/0", Voice)
        .unwrap()
        .serve_at(
            WIFI,
            Device {
                data: data.clone(),
                usb: false,
            },
        )
        .unwrap()
        .serve_at(
            USB,
            Device {
                data: data.clone(),
                usb: true,
            },
        )
        .unwrap()
        .serve_at(WIFI, Wireless)
        .unwrap()
        .serve_at(ACCESS, Access(data.clone()))
        .unwrap()
        .serve_at(IP, Ip)
        .unwrap()
        .serve_at(
            "/org/freedesktop/NetworkManager/Settings",
            Profiles(data.clone()),
        )
        .unwrap();
    for (id, profile) in [(1, OLD), (2, USB_PROFILE), (3, NEW)] {
        b = b
            .serve_at(active(id), Active(profile))
            .unwrap()
            .serve_at(
                profile,
                Profile {
                    data: data.clone(),
                    role: id,
                },
            )
            .unwrap();
    }
    b.build().await.unwrap()
}
fn login(a: &mut Auth, key: &SigningKey) -> String {
    let c = a.login_challenge(key.verifying_key().to_bytes()).unwrap();
    a.login_finish(&c.id, &key.sign(&c.transcript).to_bytes())
        .unwrap()
        .expose()
        .to_owned()
}
async fn until(auth: Arc<Mutex<Auth>>, token: &str, id: &str, allow_connecting: bool) -> Value {
    let end = Instant::now() + Duration::from_secs(8);
    loop {
        let v = authorized(
            auth.clone(),
            AuthCommand::WifiStatus {
                token: token.into(),
                trial_id: id.into(),
            },
        )
        .await
        .unwrap();
        let state = v["state"].as_str().unwrap();
        if matches!(state, "ready" | "saved" | "rolled_back" | "unknown")
            || (allow_connecting && state == "connecting")
        {
            return v;
        }
        assert!(
            Instant::now() < end,
            "ordinary USB trial did not reach expected phase"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
#[tokio::test]
#[ignore = "explicit private dbus-daemon in loopback-only namespace; ordinary USB only"]
async fn actual_usb_api_and_nm_adapter_preserve_original_principal_and_owned_rollback() {
    assert_eq!(std::env::var("RSH_A4_USB_NM").unwrap(), "1");
    let root = std::path::PathBuf::from(std::env::var_os("RSH_A4_BUS_ROOT").unwrap());
    assert_eq!(
        std::env::var("DBUS_SYSTEM_BUS_ADDRESS").unwrap(),
        format!("unix:path={}", root.join("bus").display())
    );
    assert_eq!(std::fs::metadata(&root).unwrap().mode() & 0o777, 0o700);
    let audit = directory();
    enable(audit.path()).unwrap();
    let cases = [
        ("save", Mode::Save, false),
        ("cancel", Mode::Cancel, false),
        ("renamed", Mode::Renamed, false),
        ("lost-save", Mode::LostSaveReply, false),
        ("revoked", Mode::Revoked, false),
        ("no-usb", Mode::NoUsb, false),
        ("no-ip", Mode::NoIp, false),
        ("lost-activation", Mode::LostActivationReply, false),
        ("hidden-save", Mode::Save, true),
        ("hidden-cancel", Mode::Cancel, true),
        ("hidden-renamed", Mode::Renamed, true),
        ("hidden-lost-save", Mode::LostSaveReply, true),
        ("hidden-revoked", Mode::Revoked, true),
        ("hidden-no-usb", Mode::NoUsb, true),
        ("hidden-no-ip", Mode::NoIp, true),
        ("hidden-lost-activation", Mode::LostActivationReply, true),
        ("hidden-profile-lost-flag", Mode::HiddenFlagLost, true),
    ];
    for (name, mode, hidden) in cases {
        let d = directory();
        let i = directory();
        let store = rustsimhub_storage::Store::open(d.path()).unwrap();
        let mut a = Auth::new(
            rustsimhub_auth::Identity::open(i.path()).unwrap(),
            store.handle(),
        )
        .unwrap();
        let owner = SigningKey::from_bytes(&[191; 32]);
        let c = a
            .bootstrap_challenge(owner.verifying_key().to_bytes())
            .unwrap();
        let principal = a
            .bootstrap_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
            .unwrap();
        let owner_token = login(&mut a, &owner);
        let mut tokens = vec![];
        for (seed, kind, scopes) in [
            (
                192,
                Kind::Client,
                vec![Scope::StatusRead, Scope::NetworkManage],
            ),
            (
                193,
                Kind::Client,
                vec![Scope::StatusRead, Scope::NetworkManage],
            ),
            (194, Kind::Client, vec![Scope::StatusRead]),
            (
                195,
                Kind::Nas,
                vec![Scope::StatusRead, Scope::NetworkManage],
            ),
        ] {
            let key = SigningKey::from_bytes(&[seed; 32]);
            store
                .handle()
                .approve_client(
                    principal.clone(),
                    key.verifying_key().to_bytes(),
                    kind,
                    rustsimhub_policy::mask(&scopes).unwrap(),
                )
                .unwrap();
            tokens.push(login(&mut a, &key));
        }
        let auth = Arc::new(Mutex::new(a));
        let data = Arc::new(Mutex::new(Data {
            mode,
            hidden,
            cp: false,
            active: false,
            profile: None,
            unsaved: false,
            effects: vec![],
            revoke: if mode == Mode::Revoked {
                Some((
                    auth.clone(),
                    owner_token.clone(),
                    SigningKey::from_bytes(&[192; 32])
                        .verifying_key()
                        .to_bytes(),
                ))
            } else {
                None
            },
        }));
        let bus = server(data.clone()).await;
        if mode == Mode::Save {
            let nm = NetworkManager {
                c: zbus::Connection::system().await.unwrap(),
            };
            let baseline = nm
                .snapshot_inner(true)
                .await
                .expect("private NM baseline snapshot");
            nm.cellular_autoconnect_absent(&baseline.owner)
                .await
                .expect("private NM cellular guard");
        }
        let begin = |token: String, key: String, ssid: String| {
            if hidden {
                AuthCommand::WifiBeginHidden {
                    token,
                    idempotency_key: key,
                    ssid,
                    passphrase: "synthetic-only-passphrase".into(),
                }
            } else {
                AuthCommand::WifiBegin {
                    token,
                    idempotency_key: key,
                    ssid,
                    passphrase: "synthetic-only-passphrase".into(),
                }
            }
        };
        for denied in [&tokens[2], &tokens[3]] {
            assert!(authorized(
                auth.clone(),
                begin(
                    denied.clone(),
                    format!("{name}-denied"),
                    "合成USB网络".into()
                )
            )
            .await
            .is_err());
        }
        assert!(data.lock().unwrap().effects.is_empty());
        let first = authorized(
            auth.clone(),
            begin(tokens[0].clone(), name.into(), "合成USB网络".into()),
        )
        .await
        .unwrap();
        let id = first["trial_id"].as_str().unwrap().to_owned();
        let repeat = authorized(
            auth.clone(),
            begin(tokens[0].clone(), name.into(), "合成USB网络".into()),
        )
        .await
        .unwrap();
        assert_eq!(repeat["trial_id"], id);
        let changed_mode = if hidden {
            AuthCommand::WifiBegin {
                token: tokens[0].clone(),
                idempotency_key: name.into(),
                ssid: "合成USB网络".into(),
                passphrase: "synthetic-only-passphrase".into(),
            }
        } else {
            AuthCommand::WifiBeginHidden {
                token: tokens[0].clone(),
                idempotency_key: name.into(),
                ssid: "合成USB网络".into(),
                passphrase: "synthetic-only-passphrase".into(),
            }
        };
        assert!(authorized(auth.clone(), changed_mode).await.is_err());
        assert!(authorized(
            auth.clone(),
            begin(tokens[0].clone(), name.into(), "changed-network".into())
        )
        .await
        .is_err());
        for foreign in [&tokens[1], &owner_token] {
            for command in [
                AuthCommand::WifiStatus {
                    token: foreign.clone(),
                    trial_id: id.clone(),
                },
                AuthCommand::WifiConfirm {
                    token: foreign.clone(),
                    trial_id: id.clone(),
                },
                AuthCommand::WifiCancel {
                    token: foreign.clone(),
                    trial_id: id.clone(),
                },
            ] {
                assert!(authorized(auth.clone(), command).await.is_err());
            }
        }
        let state = until(auth.clone(), &tokens[0], &id, mode == Mode::NoIp).await;
        let terminal = if state["state"] == "ready" {
            if mode == Mode::Cancel {
                authorized(
                    auth.clone(),
                    AuthCommand::WifiCancel {
                        token: tokens[0].clone(),
                        trial_id: id.clone(),
                    },
                )
                .await
                .unwrap()
            } else if mode == Mode::Revoked {
                let _ = authorized(
                    auth.clone(),
                    AuthCommand::WifiConfirm {
                        token: tokens[0].clone(),
                        trial_id: id.clone(),
                    },
                )
                .await;
                until_terminal_private(&id).await
            } else {
                authorized(
                    auth.clone(),
                    AuthCommand::WifiConfirm {
                        token: tokens[0].clone(),
                        trial_id: id.clone(),
                    },
                )
                .await
                .unwrap()
            }
        } else if state["state"] == "connecting" {
            authorized(
                auth.clone(),
                AuthCommand::WifiCancel {
                    token: tokens[0].clone(),
                    trial_id: id.clone(),
                },
            )
            .await
            .unwrap()
        } else {
            state
        };
        let expected = if mode == Mode::Save {
            "saved"
        } else if matches!(mode, Mode::LostActivationReply | Mode::HiddenFlagLost) {
            "unknown"
        } else {
            "rolled_back"
        };
        assert_eq!(
            terminal["state"],
            expected,
            "{name}: {:?}",
            data.lock().unwrap().effects
        );
        for _ in 0..100 {
            if TRIALS.available_permits() == 1 {
                break;
            };
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(TRIALS.available_permits(), 1);
        if mode == Mode::Revoked {
            assert!(manager(auth.clone(), tokens[0].clone()).await.is_err());
        }
        {
            let s = data.lock().unwrap();
            assert!(!s.cp);
            assert_eq!(
                s.effects.iter().filter(|v| **v == "activate").count(),
                usize::from(mode != Mode::NoUsb)
            );
            if mode == Mode::Save {
                assert!(s.active && !s.unsaved && s.profile.is_some());
            } else {
                assert!(!s.active);
                if !matches!(mode, Mode::LostActivationReply | Mode::HiddenFlagLost) {
                    assert!(s.profile.is_none());
                }
            }
            if matches!(
                mode,
                Mode::NoUsb
                    | Mode::Renamed
                    | Mode::Cancel
                    | Mode::NoIp
                    | Mode::LostActivationReply
                    | Mode::HiddenFlagLost
            ) {
                assert!(!s.effects.contains(&"save"));
            }
            println!(
                "A4 ordinary USB NM case={name}, state={expected}, effects={:?}",
                s.effects
            );
        }
        bus.close().await.unwrap();
    }
}
async fn until_terminal_private(id: &str) -> Value {
    let end = Instant::now() + Duration::from_secs(8);
    loop {
        let active = ACTIVE.lock().await;
        let s = active.as_ref().unwrap();
        assert_eq!(s.id, id);
        let value = *s.state.lock().await;
        if value.terminal() {
            return view(id, value, s.deadline);
        };
        drop(active);
        assert!(Instant::now() < end);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
