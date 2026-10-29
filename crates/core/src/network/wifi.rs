// SPDX-License-Identifier: AGPL-3.0-only
//! Owner-only, bounded Wi-Fi cache. Never requests a scan or changes a connection.
use super::*;
use std::collections::BTreeMap;

const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const ACCESS_POINT: &str = "org.freedesktop.NetworkManager.AccessPoint";
const AP_PREFIX: &str = "/org/freedesktop/NetworkManager/AccessPoint/";
static CATALOGUES: Semaphore = Semaphore::const_new(1);

#[derive(Clone, Serialize)]
struct Network {
    ssid: String,
    strength: u8,
    frequency_mhz: u32,
    security: &'static str,
    selectable: bool,
    connected: bool,
}
#[derive(Clone, Serialize)]
struct Catalogue {
    schema: u16,
    source: &'static str,
    scan_requested: bool,
    wifi_enabled: bool,
    scan_cache_known: bool,
    /// Age of the pre-AP display snapshot, not a promise of a live scan.
    snapshot_age_seconds: Option<u64>,
    networks: Vec<Network>,
    omitted: usize,
    truncated: bool,
    hardware_mutations: bool,
}
async fn owner(auth: Arc<Mutex<Auth>>, token: String) -> rustsimhub_auth::Result<Principal> {
    tokio::task::spawn_blocking(move || {
        let mut a = auth.try_lock().map_err(|_| Error::Busy)?;
        let p = a.recent_owner(&token)?;
        if a.authorize(&token, Scope::NetworkManage)? != p {
            return Err(Error::Forbidden);
        }
        Ok(p)
    })
    .await
    .map_err(|_| Error::Busy)?
}
async fn guarded<F, Fut>(
    auth: Arc<Mutex<Auth>>,
    token: String,
    read: F,
) -> rustsimhub_auth::Result<Value>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = rustsimhub_auth::Result<Catalogue>>,
{
    let before = owner(auth.clone(), token.clone()).await?;
    let _slot = CATALOGUES.try_acquire().map_err(|_| Error::Busy)?;
    let catalogue = timeout(Duration::from_secs(5), read())
        .await
        .map_err(|_| Error::Busy)??;
    if owner(auth, token).await? != before {
        return Err(Error::Forbidden);
    }
    serde_json::to_value(catalogue).map_err(|_| Error::InvalidInput)
}
pub(crate) async fn authorized(
    auth: Arc<Mutex<Auth>>,
    token: String,
) -> rustsimhub_auth::Result<Value> {
    guarded(auth, token, || async {
        let c = zbus::Connection::system().await.map_err(|_| Error::Io)?;
        let live = probe(&c).await?;
        Ok(before_setup_ap(
            live,
            crate::setup_ap_control::first_setup_catalogue(),
        ))
    })
    .await
}
/// Saved station (infrastructure) Wi-Fi profiles in NM; AP profiles do not count. Read-only and
/// bounded; errors propagate so callers can fail closed.
pub(crate) async fn saved_stations() -> rustsimhub_auth::Result<usize> {
    timeout(Duration::from_secs(5), async {
        let c = zbus::Connection::system().await.map_err(|_| Error::Io)?;
        let settings = zbus::Proxy::new(
            &c,
            "org.freedesktop.NetworkManager",
            "/org/freedesktop/NetworkManager/Settings",
            "org.freedesktop.NetworkManager.Settings",
        )
        .await
        .map_err(|_| Error::Io)?;
        let paths: Vec<OwnedObjectPath> = settings
            .call("ListConnections", &())
            .await
            .map_err(|_| Error::Io)?;
        if paths.len() > 64 {
            return Err(Error::Forbidden);
        }
        let mut count = 0;
        for path in paths {
            let p = zbus::Proxy::new(
                &c,
                "org.freedesktop.NetworkManager",
                path.as_str(),
                "org.freedesktop.NetworkManager.Settings.Connection",
            )
            .await
            .map_err(|_| Error::Io)?;
            let s: HashMap<String, HashMap<String, OwnedValue>> =
                p.call("GetSettings", &()).await.map_err(|_| Error::Io)?;
            if station(&s) {
                count += 1;
            }
        }
        Ok(count)
    })
    .await
    .map_err(|_| Error::Busy)?
}
/// A Wi-Fi profile that is not an access point (an absent mode means infrastructure).
fn station(s: &HashMap<String, HashMap<String, OwnedValue>>) -> bool {
    let kind = s
        .get("connection")
        .and_then(|c| c.get("type"))
        .and_then(|v| <&str>::try_from(v).ok());
    let mode = s
        .get("802-11-wireless")
        .and_then(|w| w.get("mode"))
        .and_then(|v| <&str>::try_from(v).ok());
    kind == Some("802-11-wireless") && mode.is_none_or(|m| m == "infrastructure")
}
/// Last non-empty catalogue taken just before the first-setup AP started: the single radio
/// usually cannot keep a station scan cache while it serves the AP.
static BEFORE_AP: std::sync::Mutex<Option<(std::time::Instant, Catalogue)>> =
    std::sync::Mutex::new(None);
const BEFORE_AP_MAX_AGE: Duration =
    Duration::from_secs(rustsimhub_setup_ap::MAX_WINDOW_SECONDS as u64);
/// Best-effort, bounded and read-only (no scan request); failures leave the old snapshot.
pub(crate) async fn capture_before_setup_ap() {
    // New round: never inherit a previous round's display choices if this read fails.
    if let Ok(mut slot) = BEFORE_AP.lock() {
        *slot = None;
    }
    let read = timeout(Duration::from_secs(3), async {
        let c = zbus::Connection::system().await.ok()?;
        probe(&c).await.ok()
    })
    .await;
    if let Ok(Some(catalogue)) = read {
        remember(catalogue, std::time::Instant::now());
    }
}
fn remember(catalogue: Catalogue, at: std::time::Instant) {
    if catalogue.networks.iter().any(|n| n.selectable) {
        if let Ok(mut slot) = BEFORE_AP.lock() {
            *slot = Some((at, catalogue));
        }
    }
}
fn before_setup_ap(live: Catalogue, first_setup: bool) -> Catalogue {
    let snapshot = BEFORE_AP.lock().ok().and_then(|s| s.clone());
    with_pre_ap_snapshot(live, first_setup, snapshot)
}
fn with_pre_ap_snapshot(
    live: Catalogue,
    first_setup: bool,
    snapshot: Option<(std::time::Instant, Catalogue)>,
) -> Catalogue {
    // The radio may report only the open setup AP (not a selectable station).
    // Such a nonempty cache must not hide useful pre-AP station choices.
    if !first_setup || live.networks.iter().any(|n| n.selectable) {
        return live;
    }
    match snapshot {
        Some((at, mut saved))
            if at.elapsed() <= BEFORE_AP_MAX_AGE && saved.networks.iter().any(|n| n.selectable) =>
        {
            saved.source = "before_setup_ap";
            saved.snapshot_age_seconds = Some(at.elapsed().as_secs());
            for n in &mut saved.networks {
                n.connected = false;
            }
            saved
        }
        _ => live,
    }
}
fn empty(wifi_enabled: bool) -> Catalogue {
    Catalogue {
        schema: 1,
        source: "networkmanager_cache",
        scan_requested: false,
        wifi_enabled,
        scan_cache_known: false,
        snapshot_age_seconds: None,
        networks: vec![],
        omitted: 0,
        truncated: false,
        hardware_mutations: false,
    }
}
fn ap_paths_valid(paths: &[OwnedObjectPath]) -> bool {
    paths.len() <= 128 && paths.iter().all(|p| p.as_str().starts_with(AP_PREFIX)) && {
        let mut seen = std::collections::HashSet::new();
        paths.iter().all(|p| seen.insert(p.as_str()))
    }
}
fn network(
    p: &HashMap<String, OwnedValue>,
    connected: bool,
) -> rustsimhub_auth::Result<Option<Network>> {
    let bytes: Vec<u8> = p
        .get("Ssid")
        .ok_or(Error::Io)?
        .try_clone()
        .map_err(|_| Error::Io)?
        .try_into()
        .map_err(|_| Error::Io)?;
    let Ok(ssid) = String::from_utf8(bytes) else {
        return Ok(None);
    };
    if ssid.is_empty() || ssid.len() > 32 || ssid.chars().any(char::is_control) {
        return Ok(None);
    }
    let strength = p
        .get("Strength")
        .and_then(|v| u8::try_from(v).ok())
        .filter(|v| *v <= 100)
        .ok_or(Error::InvalidInput)?;
    let frequency_mhz = u32prop(p, "Frequency")?;
    if !(2000..=8000).contains(&frequency_mhz) || u32prop(p, "Mode")? != 2 {
        return Ok(None);
    }
    let rsn = u32prop(p, "RsnFlags")?;
    let flags = u32prop(p, "Flags")?;
    // RSN + PSK is compatible with the existing WPA2-personal provisioning path.
    // Open, WEP, WPA1-only, enterprise and SAE-only are explicitly not selectable.
    let security = if flags & 1 != 0 && rsn & 0x100 != 0 {
        "wpa2_personal"
    } else if flags & 1 == 0 && rsn == 0 && u32prop(p, "WpaFlags")? == 0 {
        "open"
    } else {
        "unsupported"
    };
    Ok(Some(Network {
        ssid,
        strength,
        frequency_mhz,
        security,
        selectable: security == "wpa2_personal",
        connected,
    }))
}
fn catalogue(rows: Vec<Network>, omitted: usize, known: bool) -> Catalogue {
    // Group display choices only. Never select or expose a BSSID, and never merge stored data.
    let mut choices = BTreeMap::<(String, &'static str, u32), Network>::new();
    for row in rows {
        let key = (row.ssid.clone(), row.security, row.frequency_mhz);
        if let Some(previous) = choices.get_mut(&key) {
            previous.strength = previous.strength.max(row.strength);
            previous.connected |= row.connected;
        } else {
            choices.insert(key, row);
        }
    }
    let mut networks: Vec<_> = choices.into_values().collect();
    networks.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then(b.strength.cmp(&a.strength))
            .then(a.ssid.cmp(&b.ssid))
            .then(a.security.cmp(b.security))
            .then(a.frequency_mhz.cmp(&b.frequency_mhz))
    });
    let truncated = networks.len() > 32;
    networks.truncate(32);
    Catalogue {
        networks,
        omitted,
        truncated,
        scan_cache_known: known,
        ..empty(true)
    }
}
async fn probe(c: &zbus::Connection) -> rustsimhub_auth::Result<Catalogue> {
    let bus = zbus::fdo::DBusProxy::new(c).await.map_err(|_| Error::Io)?;
    let name = zbus::names::BusName::try_from(NAME).map_err(|_| Error::Io)?;
    let service = bus
        .get_name_owner(name.clone())
        .await
        .map_err(|_| Error::Io)?;
    let nm = zbus::Proxy::new(c, service.as_str(), ROOT, NAME)
        .await
        .map_err(|_| Error::Io)?;
    let enabled: bool = nm
        .get_property("WirelessEnabled")
        .await
        .map_err(|_| Error::Io)?;
    let mut result = empty(enabled);
    if enabled {
        let paths: Vec<OwnedObjectPath> =
            nm.call("GetDevices", &()).await.map_err(|_| Error::Io)?;
        if !devices_valid(&paths) {
            return Err(Error::InvalidInput);
        }
        let mut selected = None;
        for path in paths {
            let device = properties(
                c,
                service.as_str(),
                path.as_str(),
                "org.freedesktop.NetworkManager.Device",
            )
            .await?;
            if u32prop(&device, "DeviceType")? == 2
                && boolprop(&device, "Managed")?
                && selected.replace(path).is_some()
            {
                return Err(Error::Forbidden);
            }
        }
        if let Some(path) = selected {
            let wifi = zbus::Proxy::new(c, service.as_str(), path.as_str(), WIRELESS)
                .await
                .map_err(|_| Error::Io)?;
            let wireless = properties(c, service.as_str(), path.as_str(), WIRELESS).await?;
            let stamp = wireless
                .get("LastScan")
                .and_then(|v| i64::try_from(v).ok())
                .ok_or(Error::Io)?;
            let active: OwnedObjectPath = wireless
                .get("ActiveAccessPoint")
                .ok_or(Error::Io)?
                .try_clone()
                .map_err(|_| Error::Io)?
                .try_into()
                .map_err(|_| Error::Io)?;
            if active.as_str() != "/" && !active.as_str().starts_with(AP_PREFIX) {
                return Err(Error::InvalidInput);
            }
            let aps: Vec<OwnedObjectPath> = wifi
                .call("GetAllAccessPoints", &())
                .await
                .map_err(|_| Error::Io)?;
            if !ap_paths_valid(&aps) || stamp < -1 {
                return Err(Error::InvalidInput);
            }
            let mut rows = Vec::new();
            let mut omitted = 0;
            for ap in aps {
                let p = properties(c, service.as_str(), ap.as_str(), ACCESS_POINT).await?;
                match network(&p, ap == active)? {
                    Some(row) => rows.push(row),
                    None => omitted += 1,
                }
            }
            let after = properties(c, service.as_str(), path.as_str(), WIRELESS).await?;
            let after = after
                .get("LastScan")
                .and_then(|v| i64::try_from(v).ok())
                .ok_or(Error::Io)?;
            if after != stamp {
                return Err(Error::Busy);
            }
            result = catalogue(rows, omitted, stamp >= 0);
        }
    }
    if bus.get_name_owner(name).await.map_err(|_| Error::Io)? != service {
        return Err(Error::Io);
    }
    Ok(result)
}
#[cfg(test)]
mod tests;
