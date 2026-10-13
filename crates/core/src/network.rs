// SPDX-License-Identifier: AGPL-3.0-only
//! Read-only NetworkManager snapshot. No shell, settings secrets, scan or activation.
use rustsimhub_auth::{Auth, Error};
use rustsimhub_policy::{Principal, Scope};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    net::Ipv4Addr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{sync::Semaphore, time::timeout};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
const NAME: &str = "org.freedesktop.NetworkManager";
const ROOT: &str = "/org/freedesktop/NetworkManager";
static READERS: Semaphore = Semaphore::const_new(2);
#[derive(Serialize)]
struct Device {
    kind: &'static str,
    state: u32,
    managed: bool,
    local_ipv4: Vec<String>,
}
#[derive(Serialize)]
struct Snapshot {
    schema: u16,
    networking: bool,
    wifi_enabled: bool,
    cellular_enabled: bool,
    connectivity: u32,
    metered: u32,
    devices: Vec<Device>,
    control_api: bool,
    hardware_mutations: bool,
}
async fn principal(auth: Arc<Mutex<Auth>>, token: String) -> rustsimhub_auth::Result<Principal> {
    tokio::task::spawn_blocking(move || {
        auth.try_lock()
            .map_err(|_| Error::Busy)?
            .authorize(&token, Scope::StatusRead)
    })
    .await
    .map_err(|_| Error::Busy)?
}
pub(crate) async fn authorized(
    auth: Arc<Mutex<Auth>>,
    token: String,
) -> rustsimhub_auth::Result<Value> {
    let before = principal(auth.clone(), token.clone()).await?;
    let _slot = READERS.try_acquire().map_err(|_| Error::Busy)?;
    let snapshot = timeout(Duration::from_secs(5), async {
        let connection = zbus::Connection::system().await.map_err(|_| Error::Io)?;
        probe(&connection).await
    })
    .await
    .map_err(|_| Error::Busy)??;
    if principal(auth, token).await? != before {
        return Err(Error::Forbidden);
    }
    serde_json::to_value(snapshot).map_err(|_| Error::InvalidInput)
}
fn devices_valid(paths: &[OwnedObjectPath]) -> bool {
    paths.len() <= 16
        && paths.iter().all(|p| {
            p.as_str()
                .starts_with("/org/freedesktop/NetworkManager/Devices/")
        })
        && {
            let mut unique = std::collections::HashSet::new();
            paths.iter().all(|p| unique.insert(p.as_str()))
        }
}
fn kind(value: u32) -> &'static str {
    match value {
        1 => "ethernet",
        2 => "wifi",
        8 => "cellular",
        14 => "generic",
        32 => "loopback",
        _ => "other",
    }
}
fn local_addresses(rows: Vec<HashMap<String, OwnedValue>>) -> rustsimhub_auth::Result<Vec<String>> {
    if rows.len() > 16 {
        return Err(Error::InvalidInput);
    }
    let mut result = Vec::new();
    for row in rows {
        let Some(address) = row.get("address").and_then(|v| <&str>::try_from(v).ok()) else {
            return Err(Error::InvalidInput);
        };
        let ip: Ipv4Addr = address.parse().map_err(|_| Error::InvalidInput)?;
        if ip.is_private() || ip.is_loopback() || ip.is_link_local() {
            result.push(ip.to_string());
        }
    }
    result.sort();
    result.dedup();
    Ok(result)
}
async fn properties(
    c: &zbus::Connection,
    owner: &str,
    path: &str,
    interface: &str,
) -> rustsimhub_auth::Result<HashMap<String, OwnedValue>> {
    let proxy = zbus::fdo::PropertiesProxy::builder(c)
        .destination(owner)
        .map_err(|_| Error::Io)?
        .path(path)
        .map_err(|_| Error::Io)?
        .build()
        .await
        .map_err(|_| Error::Io)?;
    proxy
        .get_all(interface.try_into().map_err(|_| Error::Io)?)
        .await
        .map_err(|_| Error::Io)
}
fn u32prop(p: &HashMap<String, OwnedValue>, key: &str) -> rustsimhub_auth::Result<u32> {
    p.get(key)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or(Error::Io)
}
fn boolprop(p: &HashMap<String, OwnedValue>, key: &str) -> rustsimhub_auth::Result<bool> {
    p.get(key)
        .and_then(|v| bool::try_from(v).ok())
        .ok_or(Error::Io)
}
async fn probe(c: &zbus::Connection) -> rustsimhub_auth::Result<Snapshot> {
    let bus = zbus::fdo::DBusProxy::new(c).await.map_err(|_| Error::Io)?;
    let name = zbus::names::BusName::try_from(NAME).map_err(|_| Error::Io)?;
    let owner = bus
        .get_name_owner(name.clone())
        .await
        .map_err(|_| Error::Io)?;
    let proxy = zbus::Proxy::new(c, owner.as_str(), ROOT, NAME)
        .await
        .map_err(|_| Error::Io)?;
    let paths: Vec<OwnedObjectPath> = proxy.call("GetDevices", &()).await.map_err(|_| Error::Io)?;
    if !devices_valid(&paths) {
        return Err(Error::InvalidInput);
    }
    let root = properties(c, owner.as_str(), ROOT, NAME).await?;
    let mut devices = Vec::new();
    for path in paths {
        let p = properties(
            c,
            owner.as_str(),
            path.as_str(),
            "org.freedesktop.NetworkManager.Device",
        )
        .await?;
        let address_path = OwnedObjectPath::try_from(
            p.get("Ip4Config")
                .ok_or(Error::Io)?
                .try_clone()
                .map_err(|_| Error::Io)?,
        )
        .map_err(|_| Error::Io)?;
        let addresses = if address_path.as_str() == "/" {
            vec![]
        } else {
            if !address_path
                .as_str()
                .starts_with("/org/freedesktop/NetworkManager/IP4Config/")
            {
                return Err(Error::Io);
            }
            let ip = properties(
                c,
                owner.as_str(),
                address_path.as_str(),
                "org.freedesktop.NetworkManager.IP4Config",
            )
            .await?;
            let rows: Vec<HashMap<String, OwnedValue>> = ip
                .get("AddressData")
                .ok_or(Error::Io)?
                .try_clone()
                .map_err(|_| Error::Io)?
                .try_into()
                .map_err(|_| Error::Io)?;
            local_addresses(rows)?
        };
        devices.push(Device {
            kind: kind(u32prop(&p, "DeviceType")?),
            state: u32prop(&p, "State")?,
            managed: boolprop(&p, "Managed")?,
            local_ipv4: addresses,
        });
    }
    if bus.get_name_owner(name).await.map_err(|_| Error::Io)? != owner {
        return Err(Error::Io);
    }
    devices
        .sort_by(|a, b| (&a.kind, &a.local_ipv4, a.state).cmp(&(&b.kind, &b.local_ipv4, b.state)));
    Ok(Snapshot {
        schema: 1,
        networking: boolprop(&root, "NetworkingEnabled")?,
        wifi_enabled: boolprop(&root, "WirelessEnabled")?,
        cellular_enabled: boolprop(&root, "WwanEnabled")?,
        connectivity: u32prop(&root, "Connectivity")?,
        metered: u32prop(&root, "Metered")?,
        devices,
        control_api: false,
        hardware_mutations: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn row(address: &str) -> HashMap<String, OwnedValue> {
        HashMap::from([(
            "address".into(),
            OwnedValue::from(zbus::zvariant::Str::from(address)),
        )])
    }
    #[test]
    fn address_output_excludes_public_and_identifying_properties() {
        let rows = vec![
            row("127.0.0.1"),
            row("10.0.0.1"),
            row("8.8.8.8"),
            row("10.0.0.1"),
        ];
        assert_eq!(
            local_addresses(rows).unwrap(),
            vec!["10.0.0.1", "127.0.0.1"]
        );
        assert!(local_addresses(vec![row("invalid")]).is_err());
        assert!(local_addresses((0..17).map(|_| row("127.0.0.1")).collect()).is_err());
    }
    #[test]
    fn oversized_duplicate_or_foreign_catalogue_is_rejected() {
        let path: OwnedObjectPath = "/org/freedesktop/NetworkManager/Devices/1"
            .try_into()
            .unwrap();
        assert!(devices_valid(std::slice::from_ref(&path)));
        assert!(!devices_valid(&vec![path.clone(); 17]));
        assert!(!devices_valid(&[path.clone(), path]));
        assert!(!devices_valid(&["/foreign/1".try_into().unwrap()]));
    }
    struct Bus {
        calls: std::sync::atomic::AtomicUsize,
        change: bool,
    }
    #[zbus::interface(name = "org.freedesktop.DBus")]
    impl Bus {
        fn get_name_owner(&self, _name: &str) -> zbus::names::OwnedUniqueName {
            let at = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self.change && at > 0 {
                ":1.32"
            } else {
                ":1.31"
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
        fn networking_enabled(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn wireless_enabled(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn wwan_enabled(&self) -> bool {
            false
        }
        #[zbus(property)]
        fn connectivity(&self) -> u32 {
            4
        }
        #[zbus(property)]
        fn metered(&self) -> u32 {
            2
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
        fn state(&self) -> u32 {
            100
        }
        #[zbus(property)]
        fn managed(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn ip4_config(&self) -> OwnedObjectPath {
            "/org/freedesktop/NetworkManager/IP4Config/1"
                .try_into()
                .unwrap()
        }
        #[zbus(property)]
        fn hw_address(&self) -> String {
            "synthetic private MAC".into()
        }
    }
    struct Ip4;
    #[zbus::interface(name = "org.freedesktop.NetworkManager.IP4Config")]
    impl Ip4 {
        #[zbus(property)]
        fn address_data(&self) -> Vec<HashMap<String, OwnedValue>> {
            vec![row("10.0.0.2"), row("8.8.8.8")]
        }
    }
    #[tokio::test]
    async fn snapshot_uses_one_unique_owner_and_returns_only_finite_properties() {
        for change in [false, true] {
            let (a, b) = tokio::net::UnixStream::pair().unwrap();
            let guid = zbus::Guid::generate();
            let server = zbus::connection::Builder::unix_stream(a)
                .server(guid)
                .unwrap()
                .p2p()
                .serve_at(
                    "/org/freedesktop/DBus",
                    Bus {
                        calls: std::sync::atomic::AtomicUsize::new(0),
                        change,
                    },
                )
                .unwrap()
                .serve_at(ROOT, Manager)
                .unwrap()
                .serve_at("/org/freedesktop/NetworkManager/Devices/1", Wifi)
                .unwrap()
                .serve_at("/org/freedesktop/NetworkManager/IP4Config/1", Ip4)
                .unwrap();
            let client = zbus::connection::Builder::unix_stream(b).p2p();
            let (server, client) = tokio::join!(server.build(), client.build());
            let _server = server.unwrap();
            let client = client.unwrap();
            let result = probe(&client).await;
            if change {
                assert!(result.is_err());
            } else {
                let value = serde_json::to_value(result.unwrap()).unwrap();
                assert_eq!(value["devices"][0]["kind"], "wifi");
                assert_eq!(
                    value["devices"][0]["local_ipv4"],
                    serde_json::json!(["10.0.0.2"])
                );
                assert_eq!(value["cellular_enabled"], false);
                let encoded = value.to_string();
                assert!(!encoded.contains("synthetic private"));
                assert!(!encoded.contains("8.8.8.8"));
                assert!(!encoded.contains("NetworkManager/"));
            }
        }
    }
}
