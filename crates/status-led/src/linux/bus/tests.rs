// SPDX-License-Identifier: AGPL-3.0-only
//! The client is production code; only PID1/NM are replaced by private P2P peers.
use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

#[derive(Clone)]
struct Device {
    kind: u32,
    state: u32,
    interface: String,
    ip4: u32,
    ip6: u32,
    delay: bool,
}
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
impl Device {
    #[zbus(property)]
    fn device_type(&self) -> u32 {
        self.kind
    }
    #[zbus(property)]
    async fn state(&self) -> u32 {
        if self.delay {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        self.state
    }
    #[zbus(property)]
    fn interface(&self) -> String {
        self.interface.clone()
    }
    #[zbus(property)]
    fn ip4_connectivity(&self) -> u32 {
        self.ip4
    }
    #[zbus(property)]
    fn ip6_connectivity(&self) -> u32 {
        self.ip6
    }
}
struct Bus {
    calls: AtomicUsize,
    change: usize,
}
#[zbus::interface(name = "org.freedesktop.DBus")]
impl Bus {
    fn get_name_owner(&self, name: &str) -> zbus::names::OwnedUniqueName {
        assert!(matches!(name, SD | NM));
        let at = self.calls.fetch_add(1, Ordering::SeqCst);
        (if at == self.change { ":1.32" } else { ":1.31" })
            .try_into()
            .unwrap()
    }
}
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.systemd1")]
enum UnitError {
    NoSuchUnit(String),
}
struct Systemd {
    unloaded: bool,
    file_state: String,
    foreign_path: bool,
    calls: Arc<Mutex<Vec<String>>>,
}
#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Systemd {
    fn get_unit(&self, name: &str) -> std::result::Result<OwnedObjectPath, UnitError> {
        self.calls.lock().unwrap().push(format!("GetUnit:{name}"));
        let index = BASE_UNITS
            .iter()
            .chain(&BUSINESS_UNITS)
            .position(|s| *s == name)
            .expect("only fixed units may be queried");
        if self.unloaded {
            return Err(UnitError::NoSuchUnit("unloaded fixture unit".into()));
        }
        Ok(format!(
            "{}/{}",
            if self.foreign_path {
                "/foreign"
            } else {
                "/org/freedesktop/systemd1/unit"
            },
            index
        )
        .try_into()
        .unwrap())
    }
    fn get_unit_file_state(&self, name: &str) -> String {
        assert!(BASE_UNITS.iter().chain(&BUSINESS_UNITS).any(|s| *s == name));
        self.calls
            .lock()
            .unwrap()
            .push(format!("GetUnitFileState:{name}"));
        self.file_state.clone()
    }
    fn start_unit(&self, _name: &str, _mode: &str) -> OwnedObjectPath {
        panic!("indicator must never start a unit")
    }
    fn load_unit(&self, _name: &str) -> OwnedObjectPath {
        panic!("indicator must never load a unit")
    }
}
struct Unit {
    state: String,
}
#[zbus::interface(name = "org.freedesktop.systemd1.Unit")]
impl Unit {
    #[zbus(property)]
    fn active_state(&self) -> String {
        self.state.clone()
    }
}
struct Manager {
    paths: Vec<OwnedObjectPath>,
    calls: Arc<Mutex<Vec<String>>>,
}
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        self.calls.lock().unwrap().push("GetDevices".into());
        self.paths.clone()
    }
    fn check_connectivity(&self) -> u32 {
        panic!("indicator must never request a connectivity probe")
    }
    fn activate_connection(
        &self,
        _profile: OwnedObjectPath,
        _device: OwnedObjectPath,
        _specific: OwnedObjectPath,
    ) -> OwnedObjectPath {
        panic!("indicator must never activate a connection")
    }
}
struct WrongDevice;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
impl WrongDevice {
    #[zbus(property)]
    fn device_type(&self) -> String {
        "2".into()
    }
}
struct Options {
    devices: Vec<Device>,
    paths: Option<Vec<OwnedObjectPath>>,
    states: Vec<String>,
    unloaded: bool,
    file_state: String,
    foreign_unit: bool,
    change: usize,
    wrong_type: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            devices: vec![wifi()],
            paths: None,
            states: vec!["active".into(); 10],
            unloaded: false,
            file_state: "disabled".into(),
            foreign_unit: false,
            change: usize::MAX,
            wrong_type: false,
        }
    }
}
fn wifi() -> Device {
    Device {
        kind: 2,
        state: 100,
        interface: "wlan0".into(),
        ip4: 4,
        ip6: 0,
        delay: false,
    }
}
async fn fixture(
    options: Options,
) -> (zbus::Connection, zbus::Connection, Arc<Mutex<Vec<String>>>) {
    let calls = Arc::new(Mutex::new(vec![]));
    let (a, b) = tokio::net::UnixStream::pair().unwrap();
    let paths = options.paths.unwrap_or_else(|| {
        (0..options.devices.len())
            .map(|i| {
                format!("/org/freedesktop/NetworkManager/Devices/{i}")
                    .try_into()
                    .unwrap()
            })
            .collect()
    });
    let mut server = zbus::connection::Builder::unix_stream(a)
        .server(zbus::Guid::generate())
        .unwrap()
        .p2p()
        .serve_at(
            "/org/freedesktop/DBus",
            Bus {
                calls: AtomicUsize::new(0),
                change: options.change,
            },
        )
        .unwrap()
        .serve_at(
            SD_ROOT,
            Systemd {
                unloaded: options.unloaded,
                file_state: options.file_state,
                foreign_path: options.foreign_unit,
                calls: calls.clone(),
            },
        )
        .unwrap()
        .serve_at(
            NM_ROOT,
            Manager {
                paths,
                calls: calls.clone(),
            },
        )
        .unwrap();
    for (i, state) in options.states.into_iter().enumerate() {
        server = server
            .serve_at(
                format!("/org/freedesktop/systemd1/unit/{i}"),
                Unit { state },
            )
            .unwrap();
    }
    for (i, device) in options.devices.into_iter().enumerate() {
        let path = format!("/org/freedesktop/NetworkManager/Devices/{i}");
        server = if options.wrong_type {
            server.serve_at(path, WrongDevice).unwrap()
        } else {
            server.serve_at(path, device).unwrap()
        };
    }
    let (server, client) = tokio::join!(
        server.build(),
        zbus::connection::Builder::unix_stream(b).p2p().build()
    );
    (server.unwrap(), client.unwrap(), calls)
}

#[tokio::test]
async fn real_client_reads_only_fixed_services_and_per_device_cache() {
    let (_server, client, calls) = fixture(Options::default()).await;
    let input = probe(&client, true).await.unwrap();
    assert!(input.wifi_connected && input.internet);
    assert!(!input.cellular_connected && !input.error && !input.configuring);
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 11);
    assert_eq!(calls[10], "GetDevices");
    for (name, call) in BASE_UNITS.iter().chain(&BUSINESS_UNITS).zip(calls.iter()) {
        assert_eq!(call, &format!("GetUnit:{name}"));
    }
}
#[tokio::test]
async fn unit_failure_activation_and_unloaded_files_keep_existing_policy() {
    for (index, state, complete, error, configuring) in [
        (0, "failed", true, true, false),
        (8, "failed", true, true, false),
        (8, "failed", false, false, true),
        (0, "activating", true, false, true),
        (8, "activating", true, false, false),
    ] {
        let mut o = Options::default();
        o.states[index] = state.into();
        let (_server, c, _) = fixture(o).await;
        let input = probe(&c, complete).await.unwrap();
        assert_eq!((input.error, input.configuring), (error, configuring));
    }
    let (_server, c, calls) = fixture(Options {
        unloaded: true,
        ..Options::default()
    })
    .await;
    let input = probe(&c, false).await.unwrap();
    assert!(input.configuring && !input.error);
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.starts_with("GetUnitFileState:"))
            .count(),
        10
    );
}
#[tokio::test]
async fn unknown_wrong_type_foreign_duplicate_and_changed_owners_fail_closed() {
    let mut cases = vec![
        Options {
            change: 2,
            ..Options::default()
        },
        Options {
            change: 3,
            ..Options::default()
        },
        Options {
            wrong_type: true,
            ..Options::default()
        },
        Options {
            foreign_unit: true,
            ..Options::default()
        },
        Options {
            unloaded: true,
            file_state: "bad".into(),
            ..Options::default()
        },
    ];
    let mut invalid = Options::default();
    invalid.states[0] = "made-up".into();
    cases.push(invalid);
    for paths in [
        vec!["/foreign/1"],
        vec!["/org/freedesktop/NetworkManager/Devices/0"; 17],
        vec!["/org/freedesktop/NetworkManager/Devices/0"; 2],
        vec!["/org/freedesktop/NetworkManager/Devices"],
        vec!["/org/freedesktop/NetworkManager/Devices/abc"],
    ] {
        cases.push(Options {
            paths: Some(paths.into_iter().map(|s| s.try_into().unwrap()).collect()),
            ..Options::default()
        });
    }
    for device in [
        Device {
            state: 121,
            ..wifi()
        },
        Device { ip4: 5, ..wifi() },
        Device {
            interface: "x".repeat(16),
            ..wifi()
        },
    ] {
        cases.push(Options {
            devices: vec![device],
            ..Options::default()
        });
    }
    for o in cases {
        let (_server, c, _) = fixture(o).await;
        assert!(probe(&c, true).await.is_err());
    }
}
#[tokio::test]
async fn offline_portal_cellular_ipv6_and_other_interfaces_never_use_global_connectivity() {
    for (devices, wifi_connected, cellular_connected, internet) in [
        (vec![], false, false, false),
        (
            vec![Device {
                state: 30,
                ..wifi()
            }],
            false,
            false,
            false,
        ),
        (vec![Device { ip4: 2, ..wifi() }], true, false, false),
        (
            vec![Device {
                ip4: 0,
                ip6: 4,
                ..wifi()
            }],
            true,
            false,
            true,
        ),
        (
            vec![Device {
                interface: "wlan1".into(),
                ..wifi()
            }],
            false,
            false,
            false,
        ),
        (vec![Device { kind: 1, ..wifi() }], false, false, false),
        (vec![Device { kind: 8, ..wifi() }], false, true, true),
        (
            vec![Device {
                kind: 8,
                ip4: 0,
                ..wifi()
            }],
            false,
            true,
            false,
        ),
        (vec![wifi(), Device { kind: 8, ..wifi() }], true, true, true),
    ] {
        let (_server, c, _) = fixture(Options {
            devices,
            ..Options::default()
        })
        .await;
        let input = probe(&c, true).await.unwrap();
        assert_eq!(
            (
                input.wifi_connected,
                input.cellular_connected,
                input.internet
            ),
            (wifi_connected, cellular_connected, internet)
        );
    }
}
#[tokio::test]
async fn a_stalled_property_is_bounded_without_any_fallback_command() {
    let (_server, c, _) = fixture(Options {
        devices: vec![Device {
            delay: true,
            ..wifi()
        }],
        ..Options::default()
    })
    .await;
    assert!(bounded(probe(&c, true), Duration::from_millis(50))
        .await
        .is_err());
}
