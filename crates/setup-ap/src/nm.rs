// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed-interface volatile AP, unique NM owner, finite checkpoint; no modem/APN writes.
use crate::{
    transaction::{Network, Refusal},
    Credentials, Error, Result, ADDRESS, INTERFACE, PREFIX,
};
use std::{
    collections::{HashMap, HashSet},
    net::Ipv4Addr,
};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Str, Value};

const NAME: &str = "org.freedesktop.NetworkManager";
const ROOT: &str = "/org/freedesktop/NetworkManager";
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const SETTING: &str = "org.freedesktop.NetworkManager.Settings.Connection";
pub type Settings = HashMap<String, HashMap<String, OwnedValue>>;
type Properties = HashMap<String, OwnedValue>;
fn string(v: &str) -> OwnedValue {
    OwnedValue::from(Str::from(v))
}
fn strings(v: Vec<&str>) -> Result<OwnedValue> {
    OwnedValue::try_from(Value::from(v)).map_err(|_| Error::Invalid)
}
fn text<'a>(p: &'a Properties, k: &str) -> Result<&'a str> {
    p.get(k)
        .and_then(|v| <&str>::try_from(v).ok())
        .ok_or(Error::Io)
}
/// NM `802-11-wireless-security.pmf`: 1 = disable.
const PMF_DISABLE: i32 = 1;
fn number(p: &Properties, k: &str) -> Result<u32> {
    p.get(k)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or(Error::Io)
}
fn boolean(p: &Properties, k: &str) -> Result<bool> {
    p.get(k)
        .and_then(|v| bool::try_from(v).ok())
        .ok_or(Error::Io)
}
fn path(p: &Properties, k: &str) -> Result<OwnedObjectPath> {
    p.get(k)
        .ok_or(Error::Io)?
        .try_clone()
        .map_err(|_| Error::Io)?
        .try_into()
        .map_err(|_| Error::Io)
}
fn paths_valid(paths: &[OwnedObjectPath], prefix: &str, limit: usize) -> bool {
    let mut seen = HashSet::new();
    paths.len() <= limit
        && paths
            .iter()
            .all(|p| p.as_str().starts_with(prefix) && seen.insert(p))
}
fn addresses(p: &Properties) -> Result<Vec<HashMap<String, OwnedValue>>> {
    p.get("AddressData")
        .ok_or(Error::Io)?
        .try_clone()
        .map_err(|_| Error::Io)?
        .try_into()
        .map_err(|_| Error::Io)
}
fn subnet_conflict(address: &str, prefix: u32) -> Result<bool> {
    if prefix > 32 {
        return Err(Error::Invalid);
    }
    let candidate = u32::from(ADDRESS.parse::<Ipv4Addr>().map_err(|_| Error::Invalid)?);
    let ip = u32::from(address.parse::<Ipv4Addr>().map_err(|_| Error::Invalid)?);
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    Ok(candidate & mask == ip & mask || candidate & 0xffff_ff00 == ip & 0xffff_ff00)
}
pub fn settings(c: &Credentials) -> Result<Settings> {
    let mut s = base_settings(c)?;
    // First-setup AP is open (user decision 2026-10-28): no security section at all.
    if c.is_open() {
        s.remove("802-11-wireless-security");
    }
    Ok(s)
}
fn base_settings(c: &Credentials) -> Result<Settings> {
    let addresses = vec![HashMap::from([
        ("address".to_owned(), string(ADDRESS)),
        ("prefix".to_owned(), PREFIX.into()),
    ])];
    Ok(HashMap::from([
        (
            "connection".into(),
            HashMap::from([
                ("id".into(), string("RustSimHub protected setup")),
                ("uuid".into(), string(c.uuid())),
                ("type".into(), string("802-11-wireless")),
                ("interface-name".into(), string(INTERFACE)),
                ("autoconnect".into(), false.into()),
            ]),
        ),
        (
            "802-11-wireless".into(),
            HashMap::from([
                (
                    "ssid".into(),
                    OwnedValue::try_from(Value::from(c.ssid().as_bytes().to_vec()))
                        .map_err(|_| Error::Invalid)?,
                ),
                ("mode".into(), string("ap")),
                ("band".into(), string("bg")),
                ("ap-isolation".into(), 1i32.into()),
            ]),
        ),
        (
            "802-11-wireless-security".into(),
            HashMap::from([
                ("key-mgmt".into(), string("wpa-psk")),
                ("proto".into(), strings(vec!["rsn"])?),
                ("pairwise".into(), strings(vec!["ccmp"])?),
                ("group".into(), strings(vec!["ccmp"])?),
                // wcn36xx has no BIP/AES-CMAC: NM's default optional PMF adds an IGTK the
                // kernel rejects ("key setting validation failed") and the AP never starts.
                ("pmf".into(), PMF_DISABLE.into()),
                (
                    "psk".into(),
                    string(c.expose_passphrase_for_trusted_delivery()),
                ),
                // An open variant never sends this section (removed in settings()).
                ("psk-flags".into(), 0u32.into()),
            ]),
        ),
        (
            "ipv4".into(),
            HashMap::from([
                ("method".into(), string("shared")),
                (
                    "address-data".into(),
                    OwnedValue::try_from(Value::from(addresses)).map_err(|_| Error::Invalid)?,
                ),
                ("never-default".into(), true.into()),
                ("may-fail".into(), false.into()),
            ]),
        ),
        (
            "ipv6".into(),
            HashMap::from([
                ("method".into(), string("disabled")),
                ("never-default".into(), true.into()),
            ]),
        ),
    ]))
}
fn profile_matches(p: &Settings, c: &Credentials) -> Result<bool> {
    let connection = p.get("connection").ok_or(Error::Io)?;
    let wifi = p.get("802-11-wireless").ok_or(Error::Io)?;
    let ip = p.get("ipv4").ok_or(Error::Io)?;
    let ipv6 = p.get("ipv6").ok_or(Error::Io)?;
    let ssid: Vec<u8> = wifi
        .get("ssid")
        .ok_or(Error::Io)?
        .try_clone()
        .map_err(|_| Error::Io)?
        .try_into()
        .map_err(|_| Error::Io)?;
    let address_data: Vec<Properties> = ip
        .get("address-data")
        .ok_or(Error::Io)?
        .try_clone()
        .map_err(|_| Error::Io)?
        .try_into()
        .map_err(|_| Error::Io)?;
    let secured = match (c.is_open(), p.get("802-11-wireless-security")) {
        (true, None) => true,
        (false, Some(security)) => security_matches(security)?,
        _ => false,
    };
    Ok(secured
        && text(connection, "uuid")? == c.uuid()
        && text(connection, "type")? == "802-11-wireless"
        && text(connection, "interface-name")? == INTERFACE
        && !boolean(connection, "autoconnect")?
        && text(wifi, "mode")? == "ap"
        && text(wifi, "band")? == "bg"
        && wifi.get("ap-isolation").and_then(|v| i32::try_from(v).ok()) == Some(1)
        && ssid == c.ssid().as_bytes()
        && text(ip, "method")? == "shared"
        && boolean(ip, "never-default")?
        && !boolean(ip, "may-fail")?
        && address_data.len() == 1
        && text(&address_data[0], "address")? == ADDRESS
        && number(&address_data[0], "prefix")? == PREFIX
        && text(ipv6, "method")? == "disabled"
        && boolean(ipv6, "never-default")?)
}
fn security_matches(security: &Properties) -> Result<bool> {
    let protocol: Vec<String> = security
        .get("proto")
        .ok_or(Error::Io)?
        .try_clone()
        .map_err(|_| Error::Io)?
        .try_into()
        .map_err(|_| Error::Io)?;
    let cipher = |key: &str| -> Result<Vec<String>> {
        security
            .get(key)
            .ok_or(Error::Io)?
            .try_clone()
            .map_err(|_| Error::Io)?
            .try_into()
            .map_err(|_| Error::Io)
    };
    Ok(text(security, "key-mgmt")? == "wpa-psk"
        && protocol == ["rsn"]
        && cipher("pairwise")? == ["ccmp"]
        && cipher("group")? == ["ccmp"]
        && security.get("pmf").and_then(|v| i32::try_from(v).ok()) == Some(PMF_DISABLE)
        // NM omits settings at their default (psk-flags 0) from GetSettings.
        && security.get("psk-flags").map_or(Ok(0), |v| u32::try_from(v).map_err(|_| Error::Io))? == 0)
}
pub struct NetworkManager {
    connection: zbus::Connection,
    owner: Option<zbus::names::OwnedUniqueName>,
    wifi: Option<OwnedObjectPath>,
    checkpoint: Option<OwnedObjectPath>,
    checkpoint_requested: bool,
    profile: Option<OwnedObjectPath>,
    activation_requested: bool,
    retain_ap: bool,
    station: Option<station::Trial>,
    refusal: Option<Refusal>,
    /// Reconfigure mode: the one saved station this session must hand back on rollback.
    saved: Option<OwnedObjectPath>,
}
impl NetworkManager {
    pub async fn system() -> Result<Self> {
        Ok(Self::new(
            zbus::Connection::system().await.map_err(|_| Error::Io)?,
        ))
    }
    fn new(connection: zbus::Connection) -> Self {
        Self {
            connection,
            owner: None,
            wifi: None,
            checkpoint: None,
            checkpoint_requested: false,
            profile: None,
            activation_requested: false,
            retain_ap: false,
            station: None,
            refusal: None,
            saved: None,
        }
    }
    /// Opt in before preflight only. Keeps this private AP profile in RAM while a station is tested.
    /// The normal AP worker remains volatile; this does not install or enable the handoff flow.
    pub fn retain_ap_for_station_trial(&mut self) -> Result<()> {
        if self.owner.is_some() {
            return Err(Error::Denied);
        }
        self.retain_ap = true;
        Ok(())
    }
    async fn current_owner(&self) -> Result<zbus::names::OwnedUniqueName> {
        zbus::fdo::DBusProxy::new(&self.connection)
            .await
            .map_err(|_| Error::Io)?
            .get_name_owner(NAME.try_into().map_err(|_| Error::Invalid)?)
            .await
            .map_err(|_| Error::Io)
    }
    /// Records a known precondition refusal. Only reached before checkpoint, guard or activation.
    fn refuse(&mut self, reason: Refusal, error: Error) -> Result<()> {
        self.refusal = Some(reason);
        Err(error)
    }
    async fn fence(&self) -> Result<()> {
        if self.owner.as_ref() != Some(&self.current_owner().await?) {
            return Err(Error::Changed);
        }
        Ok(())
    }
    async fn proxy<'a>(&'a self, path: &'a str, interface: &'a str) -> Result<zbus::Proxy<'a>> {
        zbus::Proxy::new(
            &self.connection,
            self.owner.as_ref().ok_or(Error::Denied)?.as_str(),
            path,
            interface,
        )
        .await
        .map_err(|_| Error::Io)
    }
    async fn properties(&self, path: &str, interface: &str) -> Result<Properties> {
        // Explicit GetAll avoids reusing a cached device/checkpoint state across fences.
        zbus::fdo::PropertiesProxy::builder(&self.connection)
            .destination(self.owner.as_ref().ok_or(Error::Denied)?.as_str())
            .map_err(|_| Error::Invalid)?
            .path(path)
            .map_err(|_| Error::Invalid)?
            .build()
            .await
            .map_err(|_| Error::Io)?
            .get_all(interface.try_into().map_err(|_| Error::Invalid)?)
            .await
            .map_err(|_| Error::Io)
    }
    async fn connections(&self) -> Result<Vec<OwnedObjectPath>> {
        let paths: Vec<OwnedObjectPath> = self
            .proxy(
                "/org/freedesktop/NetworkManager/Settings",
                "org.freedesktop.NetworkManager.Settings",
            )
            .await?
            .call("ListConnections", &())
            .await
            .map_err(|_| Error::Io)?;
        if !paths_valid(&paths, "/org/freedesktop/NetworkManager/Settings/", 64) {
            return Err(Error::Invalid);
        }
        Ok(paths)
    }
    async fn get_settings(&self, p: &OwnedObjectPath) -> Result<Settings> {
        self.proxy(p.as_str(), SETTING)
            .await?
            .call("GetSettings", &())
            .await
            .map_err(|_| Error::Io)
    }
    /// Wi-Fi is idle, or (reconfiguration only) activating/active on the recorded saved
    /// station. Anything else, including our AP or an unknown connection, is a change.
    async fn idle_or_saved(&self) -> Result<()> {
        if self.disconnected().await.is_ok() {
            return Ok(());
        }
        self.fence().await?;
        let saved = self.saved.as_ref().ok_or(Error::Changed)?;
        let p = self
            .properties(self.wifi.as_ref().ok_or(Error::Denied)?.as_str(), DEVICE)
            .await?;
        let active = path(&p, "ActiveConnection")?;
        if !(40..=100).contains(&number(&p, "State")?)
            || !active
                .as_str()
                .starts_with("/org/freedesktop/NetworkManager/ActiveConnection/")
        {
            return Err(Error::Changed);
        }
        let a = self
            .properties(
                active.as_str(),
                "org.freedesktop.NetworkManager.Connection.Active",
            )
            .await?;
        if path(&a, "Connection")? != *saved {
            return Err(Error::Changed);
        }
        Ok(())
    }
    async fn disconnected(&self) -> Result<()> {
        self.fence().await?;
        let p = self
            .properties(self.wifi.as_ref().ok_or(Error::Denied)?.as_str(), DEVICE)
            .await?;
        if number(&p, "State")? != 30 || path(&p, "ActiveConnection")?.as_str() != "/" {
            return Err(Error::Changed);
        }
        Ok(())
    }
}
impl Network for NetworkManager {
    async fn preflight(&mut self) -> Result<()> {
        if self.owner.is_some() {
            return Err(Error::Denied);
        }
        self.owner = Some(self.current_owner().await?);
        let root = self.properties(ROOT, NAME).await?;
        if !boolean(&root, "NetworkingEnabled")? || !boolean(&root, "WirelessEnabled")? {
            return self.refuse(Refusal::WifiDisabled, Error::Denied);
        }
        let points: Vec<OwnedObjectPath> = root
            .get("Checkpoints")
            .ok_or(Error::Io)?
            .try_clone()
            .map_err(|_| Error::Io)?
            .try_into()
            .map_err(|_| Error::Io)?;
        if !points.is_empty() {
            return self.refuse(Refusal::CheckpointExists, Error::Busy);
        }
        let paths: Vec<OwnedObjectPath> = self
            .proxy(ROOT, NAME)
            .await?
            .call("GetDevices", &())
            .await
            .map_err(|_| Error::Io)?;
        if !paths_valid(&paths, "/org/freedesktop/NetworkManager/Devices/", 16) {
            return Err(Error::Invalid);
        }
        for device in paths {
            let p = self.properties(device.as_str(), DEVICE).await?;
            if number(&p, "DeviceType")? == 2
                && (text(&p, "Interface")? != INTERFACE
                    || !boolean(&p, "Managed")?
                    || self.wifi.replace(device.clone()).is_some())
            {
                return self.refuse(Refusal::ApUnsupported, Error::Denied);
            }
            let ip = path(&p, "Ip4Config")?;
            if ip.as_str() != "/" {
                if !ip
                    .as_str()
                    .starts_with("/org/freedesktop/NetworkManager/IP4Config/")
                {
                    return Err(Error::Invalid);
                }
                let p = self
                    .properties(ip.as_str(), "org.freedesktop.NetworkManager.IP4Config")
                    .await?;
                let rows = addresses(&p)?;
                if rows.len() > 16 {
                    return Err(Error::Invalid);
                }
                for row in rows {
                    if subnet_conflict(text(&row, "address")?, number(&row, "prefix")?)? {
                        return self.refuse(Refusal::SubnetConflict, Error::Denied);
                    }
                }
            }
        }
        if self.wifi.is_none() {
            return self.refuse(Refusal::ApUnsupported, Error::Denied);
        }
        let wifi = self
            .properties(self.wifi.as_ref().ok_or(Error::Denied)?.as_str(), WIRELESS)
            .await?;
        if number(&wifi, "WirelessCapabilities")? & 0x40 == 0 {
            return self.refuse(Refusal::ApUnsupported, Error::Denied);
        }
        // First setup has no saved station. Reconfiguration keeps exactly one, which is
        // never modified or deleted and is what the outer checkpoint hands back on rollback.
        let mut saved = None;
        for p in self.connections().await? {
            let s = self.get_settings(&p).await?;
            let connection = s.get("connection").ok_or(Error::Io)?;
            let kind = text(connection, "type")?;
            if kind == "802-11-wireless" && saved.replace(p.clone()).is_some() {
                return self.refuse(Refusal::StationSaved, Error::Denied);
            }
            if kind == "gsm" && boolean(connection, "autoconnect")? {
                return self.refuse(Refusal::GsmAutoconnect, Error::Denied);
            }
        }
        self.saved = saved;
        if let Err(e) = self.idle_or_saved().await {
            return self.refuse(Refusal::StationConnected, e);
        }
        self.fence().await
    }
    fn refusal(&self) -> Option<Refusal> {
        self.refusal
    }
    async fn checkpoint(&mut self, seconds: u32) -> Result<()> {
        // Window plus the 30 s rollback margin; must follow MAX_WINDOW_SECONDS (field test
        // 2026-10-29: a stale 330 s limit refused every one-hour first-setup round).
        if !(30..=crate::MAX_WINDOW_SECONDS + 30).contains(&seconds) || self.checkpoint_requested {
            return Err(Error::Invalid);
        }
        self.idle_or_saved().await?;
        self.checkpoint_requested = true;
        let path: OwnedObjectPath = self
            .proxy(ROOT, NAME)
            .await?
            .call(
                "CheckpointCreate",
                &(vec![self.wifi.clone().ok_or(Error::Denied)?], seconds, 0u32),
            )
            .await
            .map_err(|_| Error::Io)?;
        if !path
            .as_str()
            .starts_with("/org/freedesktop/NetworkManager/Checkpoint/")
        {
            return Err(Error::Invalid);
        }
        self.checkpoint = Some(path);
        self.fence().await
    }
    async fn activate(&mut self, c: &Credentials) -> Result<()> {
        if self.activation_requested || self.checkpoint.is_none() {
            return Err(Error::Denied);
        }
        self.idle_or_saved().await?;
        self.activation_requested = true; // Retain uncertainty even if the D-Bus reply is lost.
        let (p, a, _): (OwnedObjectPath, OwnedObjectPath, Properties) = self
            .proxy(ROOT, NAME)
            .await?
            .call(
                "AddAndActivateConnection2",
                &(
                    settings(c)?,
                    self.wifi.as_ref().ok_or(Error::Denied)?,
                    OwnedObjectPath::try_from("/").map_err(|_| Error::Invalid)?,
                    HashMap::from([
                        (
                            "persist",
                            string(if self.retain_ap { "memory" } else { "volatile" }),
                        ),
                        ("bind-activation", string("dbus-client")),
                    ]),
                ),
            )
            .await
            .map_err(|_| Error::Io)?;
        if !p
            .as_str()
            .starts_with("/org/freedesktop/NetworkManager/Settings/")
            || !a
                .as_str()
                .starts_with("/org/freedesktop/NetworkManager/ActiveConnection/")
        {
            return Err(Error::Invalid);
        }
        self.profile = Some(p);
        self.fence().await
    }
    async fn ready(&self, c: &Credentials) -> Result<bool> {
        self.fence().await?;
        let root = self.properties(ROOT, NAME).await?;
        let points: Vec<OwnedObjectPath> = root
            .get("Checkpoints")
            .ok_or(Error::Io)?
            .try_clone()
            .map_err(|_| Error::Io)?
            .try_into()
            .map_err(|_| Error::Io)?;
        if !points.contains(self.checkpoint.as_ref().ok_or(Error::Denied)?) {
            return Err(Error::Changed);
        }
        let profile = self.profile.as_ref().ok_or(Error::Denied)?;
        if !profile_matches(&self.get_settings(profile).await?, c)? {
            return Err(Error::Changed);
        }
        let d = self
            .properties(self.wifi.as_ref().ok_or(Error::Denied)?.as_str(), DEVICE)
            .await?;
        if number(&d, "State")? != 100 {
            return Ok(false);
        }
        let active = path(&d, "ActiveConnection")?;
        if !active
            .as_str()
            .starts_with("/org/freedesktop/NetworkManager/ActiveConnection/")
        {
            return Err(Error::Invalid);
        }
        let a = self
            .properties(
                active.as_str(),
                "org.freedesktop.NetworkManager.Connection.Active",
            )
            .await?;
        if path(&a, "Connection")? != *profile {
            return Err(Error::Changed);
        }
        let w = self
            .properties(self.wifi.as_ref().ok_or(Error::Denied)?.as_str(), WIRELESS)
            .await?;
        if number(&w, "Mode")? != 3 {
            return Err(Error::Changed);
        }
        let ip = path(&d, "Ip4Config")?;
        if !ip
            .as_str()
            .starts_with("/org/freedesktop/NetworkManager/IP4Config/")
        {
            return Ok(false);
        }
        let p = self
            .properties(ip.as_str(), "org.freedesktop.NetworkManager.IP4Config")
            .await?;
        let rows = addresses(&p)?;
        if rows.len() != 1
            || text(&rows[0], "address")? != ADDRESS
            || number(&rows[0], "prefix")? != PREFIX
        {
            return Err(Error::Changed);
        }
        self.fence().await?;
        Ok(true)
    }
    async fn rollback(&mut self, c: &Credentials) -> Result<()> {
        if self.station.is_some() {
            return Err(Error::Denied);
        }
        self.fence().await?;
        if self.checkpoint_requested && self.checkpoint.is_none() {
            let root = self.properties(ROOT, NAME).await?;
            let points: Vec<OwnedObjectPath> = root
                .get("Checkpoints")
                .ok_or(Error::Io)?
                .try_clone()
                .map_err(|_| Error::Io)?
                .try_into()
                .map_err(|_| Error::Io)?;
            // A lost creation reply does not authorize guessing/destroying a checkpoint.
            if !points.is_empty() {
                return Err(Error::Changed);
            }
        }
        if let Some(cp) = &self.checkpoint {
            let root = self.properties(ROOT, NAME).await?;
            let points: Vec<OwnedObjectPath> = root
                .get("Checkpoints")
                .ok_or(Error::Io)?
                .try_clone()
                .map_err(|_| Error::Io)?
                .try_into()
                .map_err(|_| Error::Io)?;
            if points.contains(cp) {
                let results: HashMap<String, u32> = self
                    .proxy(ROOT, NAME)
                    .await?
                    .call("CheckpointRollback", &(cp,))
                    .await
                    .map_err(|_| Error::Io)?;
                if results.len() != 1
                    || results.get(self.wifi.as_ref().ok_or(Error::Denied)?.as_str()) != Some(&0)
                {
                    return Err(Error::Io);
                }
                // Destroy only the exact checkpoint created by this session, never all checkpoints.
                let after = self.properties(ROOT, NAME).await?;
                let remaining: Vec<OwnedObjectPath> = after
                    .get("Checkpoints")
                    .ok_or(Error::Io)?
                    .try_clone()
                    .map_err(|_| Error::Io)?
                    .try_into()
                    .map_err(|_| Error::Io)?;
                if remaining.contains(cp) {
                    let _: () = self
                        .proxy(ROOT, NAME)
                        .await?
                        .call("CheckpointDestroy", &(cp,))
                        .await
                        .map_err(|_| Error::Io)?;
                }
            }
        }
        let mut candidates = Vec::new();
        for p in self.connections().await? {
            let s = self.get_settings(&p).await?;
            if text(s.get("connection").ok_or(Error::Io)?, "uuid")? == c.uuid() {
                if !self.activation_requested || !profile_matches(&s, c)? {
                    return Err(Error::Changed);
                }
                candidates.push(p);
            }
        }
        if candidates.len() > 1 {
            return Err(Error::Changed);
        }
        for p in candidates {
            let _: () = self
                .proxy(p.as_str(), SETTING)
                .await?
                .call("Delete", &())
                .await
                .map_err(|_| Error::Io)?;
        }
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                self.fence().await?;
                if self.idle_or_saved().await.is_ok() {
                    return Ok::<(), Error>(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| Error::Busy)??;
        for p in self.connections().await? {
            let s = self.get_settings(&p).await?;
            if text(s.get("connection").ok_or(Error::Io)?, "uuid")? == c.uuid() {
                return Err(Error::Changed);
            }
        }
        self.fence().await
    }
}

mod station;

#[cfg(test)]
mod tests;
