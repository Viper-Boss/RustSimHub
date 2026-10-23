// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_auth::Identity;
use rustsimhub_storage::Store;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn ap(ssid: &[u8], strength: u8, rsn: u32, flags: u32) -> HashMap<String, OwnedValue> {
    HashMap::from([
        (
            "Ssid".into(),
            OwnedValue::try_from(zbus::zvariant::Value::from(ssid.to_vec())).unwrap(),
        ),
        ("Strength".into(), strength.into()),
        ("RsnFlags".into(), rsn.into()),
        ("WpaFlags".into(), 0u32.into()),
        ("Flags".into(), flags.into()),
        ("Mode".into(), 2u32.into()),
        ("Frequency".into(), 2412u32.into()),
    ])
}
#[test]
fn display_choices_are_finite_utf8_safe_and_never_silently_allow_other_security() {
    for ssid in [&[][..], &[255][..], b"bad\0name", &[b'x'; 33]] {
        assert!(network(&ap(ssid, 20, 0x100, 1), false).unwrap().is_none());
    }
    assert!(network(&ap(b"synthetic", 101, 0x100, 1), false).is_err());
    for (rsn, flags, selectable) in [
        (0x100, 1, true),
        (0, 0, false),
        (0x200, 1, false),
        (0x400, 1, false),
        (0, 1, false),
    ] {
        assert_eq!(
            network(&ap(b"synthetic", 60, rsn, flags), false)
                .unwrap()
                .unwrap()
                .selectable,
            selectable
        );
    }
    let mut rows = Vec::new();
    for n in 0..40 {
        rows.push(
            network(&ap(format!("synthetic-{n}").as_bytes(), n, 0x100, 1), false)
                .unwrap()
                .unwrap(),
        );
    }
    let mut duplicate = rows[0].clone();
    duplicate.strength = 99;
    duplicate.connected = true;
    rows.push(duplicate);
    let value = catalogue(rows, 2, true);
    assert!(value.truncated);
    assert_eq!(value.networks.len(), 32);
    assert_eq!(value.networks[0].ssid, "synthetic-0");
    assert_eq!(value.networks[0].strength, 99);
    assert!(value.networks[0].connected);
    let path: OwnedObjectPath = format!("{AP_PREFIX}1").try_into().unwrap();
    assert!(ap_paths_valid(std::slice::from_ref(&path)));
    assert!(!ap_paths_valid(&[path.clone(), path.clone()]));
    assert!(!ap_paths_valid(&vec![path; 129]));
    assert!(!ap_paths_valid(&["/foreign/1".try_into().unwrap()]));
}
struct Bus {
    calls: AtomicUsize,
    change: bool,
}
#[zbus::interface(name = "org.freedesktop.DBus")]
impl Bus {
    fn get_name_owner(&self, _name: &str) -> zbus::names::OwnedUniqueName {
        let at = self.calls.fetch_add(1, Ordering::SeqCst);
        if self.change && at > 0 {
            ":1.52"
        } else {
            ":1.51"
        }
        .try_into()
        .unwrap()
    }
}
struct Manager;
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        vec!["/org/freedesktop/NetworkManager/Devices/1"
            .try_into()
            .unwrap()]
    }
    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        true
    }
}
struct Wifi;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
impl Wifi {
    #[zbus(property)]
    fn device_type(&self) -> u32 {
        2
    }
    #[zbus(property)]
    fn managed(&self) -> bool {
        true
    }
}
struct Wireless {
    reads: AtomicUsize,
    change: bool,
    invalid_paths: bool,
}
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl Wireless {
    fn get_all_access_points(&self) -> Vec<OwnedObjectPath> {
        vec![if self.invalid_paths {
            "/foreign/1".to_owned()
        } else {
            format!("{AP_PREFIX}1")
        }
        .try_into()
        .unwrap()]
    }
    #[zbus(property)]
    fn active_access_point(&self) -> OwnedObjectPath {
        format!("{AP_PREFIX}1").try_into().unwrap()
    }
    #[zbus(property)]
    fn last_scan(&self) -> i64 {
        let at = self.reads.fetch_add(1, Ordering::SeqCst);
        if self.change {
            1000 + at as i64
        } else {
            1000
        }
    }
}
struct AccessPoint;
#[zbus::interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
impl AccessPoint {
    #[zbus(property)]
    fn ssid(&self) -> Vec<u8> {
        "合成 <img src=x>".as_bytes().to_vec()
    }
    #[zbus(property)]
    fn strength(&self) -> u8 {
        81
    }
    #[zbus(property)]
    fn flags(&self) -> u32 {
        1
    }
    #[zbus(property)]
    fn rsn_flags(&self) -> u32 {
        0x100
    }
    #[zbus(property)]
    fn wpa_flags(&self) -> u32 {
        0
    }
    #[zbus(property)]
    fn mode(&self) -> u32 {
        2
    }
    #[zbus(property)]
    fn frequency(&self) -> u32 {
        2412
    }
    #[zbus(property)]
    fn hw_address(&self) -> String {
        "private synthetic BSSID".into()
    }
}
#[tokio::test]
async fn cached_dbus_catalogue_is_owner_pinned_and_scan_consistent_without_write_methods() {
    for (owner_change, scan_change, invalid_paths) in [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let (a, b) = tokio::net::UnixStream::pair().unwrap();
        let server = zbus::connection::Builder::unix_stream(a)
            .server(zbus::Guid::generate())
            .unwrap()
            .p2p()
            .serve_at(
                "/org/freedesktop/DBus",
                Bus {
                    calls: AtomicUsize::new(0),
                    change: owner_change,
                },
            )
            .unwrap()
            .serve_at(ROOT, Manager)
            .unwrap()
            .serve_at("/org/freedesktop/NetworkManager/Devices/1", Wifi)
            .unwrap()
            .serve_at(
                "/org/freedesktop/NetworkManager/Devices/1",
                Wireless {
                    reads: AtomicUsize::new(0),
                    change: scan_change,
                    invalid_paths,
                },
            )
            .unwrap()
            .serve_at(format!("{AP_PREFIX}1"), AccessPoint)
            .unwrap();
        let client = zbus::connection::Builder::unix_stream(b).p2p();
        let (server, client) = tokio::join!(server.build(), client.build());
        let _server = server.unwrap();
        let result = probe(&client.unwrap()).await;
        if owner_change || scan_change || invalid_paths {
            assert!(result.is_err());
        } else {
            let value = serde_json::to_value(result.unwrap()).unwrap();
            assert_eq!(value["networks"][0]["ssid"], "合成 <img src=x>");
            assert_eq!(value["networks"][0]["connected"], true);
            assert_eq!(value["scan_requested"], false);
            assert_eq!(value["hardware_mutations"], false);
            let encoded = value.to_string();
            assert!(!encoded.contains("private synthetic"));
            assert!(!encoded.contains("/org/freedesktop"));
        }
    }
}
#[tokio::test]
async fn catalogue_needs_recent_real_owner_and_rechecks_after_read() {
    let d = tempfile::tempdir().unwrap();
    let i = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    for directory in [d.path(), i.path()] {
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let store = Store::open(d.path()).unwrap();
    let handle = store.handle();
    let mut a = Auth::new(Identity::open(i.path()).unwrap(), handle).unwrap();
    let key = SigningKey::from_bytes(&[108; 32]);
    let challenge = a
        .bootstrap_challenge(key.verifying_key().to_bytes())
        .unwrap();
    a.bootstrap_finish(&challenge.id, &key.sign(&challenge.transcript).to_bytes())
        .unwrap();
    let challenge = a.login_challenge(key.verifying_key().to_bytes()).unwrap();
    let token = a
        .login_finish(&challenge.id, &key.sign(&challenge.transcript).to_bytes())
        .unwrap()
        .expose()
        .to_owned();
    let client = SigningKey::from_bytes(&[109; 32]);
    let kind = rustsimhub_policy::Kind::Client;
    let scopes = [Scope::NetworkManage];
    a.open_pairing(&token).unwrap();
    let c = a
        .pairing_challenge(client.verifying_key().to_bytes(), kind, &scopes)
        .unwrap();
    let pairing = a
        .pairing_finish(&c.id, &client.sign(&c.transcript).to_bytes())
        .unwrap();
    a.approve(
        &token,
        &pairing,
        client.verifying_key().to_bytes(),
        kind,
        &scopes,
    )
    .unwrap();
    let c = a
        .login_challenge(client.verifying_key().to_bytes())
        .unwrap();
    let delegated = a
        .login_finish(&c.id, &client.sign(&c.transcript).to_bytes())
        .unwrap()
        .expose()
        .to_owned();
    let auth = Arc::new(Mutex::new(a));
    let entered = AtomicBool::new(false);
    assert!(guarded(auth.clone(), delegated, || async {
        entered.store(true, Ordering::SeqCst);
        Ok(empty(true))
    })
    .await
    .is_err());
    assert!(!entered.load(Ordering::SeqCst));
    assert!(guarded(auth.clone(), "0".repeat(64), || async {
        entered.store(true, Ordering::SeqCst);
        Ok(empty(true))
    })
    .await
    .is_err());
    assert!(!entered.load(Ordering::SeqCst));
    let value = guarded(auth.clone(), token.clone(), || async { Ok(empty(true)) })
        .await
        .unwrap();
    assert_eq!(value["scan_requested"], false);
    let reset = auth.clone();
    assert!(guarded(auth, token, || async {
        // A Core identity restart invalidates all outstanding sessions while the read waits.
        *reset.lock().unwrap() =
            Auth::new(Identity::open(i.path()).unwrap(), store.handle()).unwrap();
        Ok(empty(true))
    })
    .await
    .is_err());
}
