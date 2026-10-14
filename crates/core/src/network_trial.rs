// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit saved-Wi-Fi trial. Finite NM checkpoint, no APN/WWAN/radio or settings writes.
pub(crate) mod provision;
use rustsimhub_auth::{Auth, Error};
use rustsimhub_policy::{Principal, Scope};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tokio::{sync::Semaphore, time::timeout};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
const NM: &str = "org.freedesktop.NetworkManager";
const ROOT: &str = "/org/freedesktop/NetworkManager";
const MM: &str = "org.freedesktop.ModemManager1";
static DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
#[cfg(feature = "dev-fixtures")]
static DEVELOPMENT_FAULT: OnceLock<bool> = OnceLock::new();
#[cfg(feature = "dev-fixtures")]
pub(crate) fn enable_fault(path: &Path) -> Result<()> {
    DEVELOPMENT_FAULT.set(true).map_err(|_| Error::Forbidden)?;
    enable(path)
}
#[cfg(feature = "dev-fixtures")]
fn fault_mode() -> bool {
    DEVELOPMENT_FAULT.get() == Some(&true)
}
static TRIALS: Semaphore = Semaphore::const_new(1);
type Result<T> = rustsimhub_auth::Result<T>;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Summary {
    schema: u16,
    wifi_only: bool,
    activation_requested: bool,
    rollback_verified: bool,
    cellular_activation: bool,
    hardware_mutations: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format: u16,
    principal: String,
    generation: u64,
    summary: Option<Summary>,
}
struct Journal {
    marker: PathBuf,
    result: PathBuf,
    record: Record,
}
fn directory(path: &Path) -> Result<()> {
    let m = std::fs::symlink_metadata(path).map_err(|_| Error::Io)?;
    if !path.is_absolute()
        || !m.is_dir()
        || m.uid() != rustix::process::geteuid().as_raw()
        || m.mode() & 0o077 != 0
    {
        return Err(Error::Forbidden);
    }
    Ok(())
}
pub(crate) fn enabled() -> bool {
    DIRECTORY.get().is_some()
}
pub fn enable(path: &Path) -> Result<()> {
    directory(path)?;
    DIRECTORY.set(path.to_owned()).map_err(|_| Error::Forbidden)
}
impl Journal {
    fn new(dir: &Path, key: &str, p: &Principal) -> Result<Self> {
        directory(dir)?;
        if key.is_empty() || key.len() > 128 || key.chars().any(char::is_control) {
            return Err(Error::InvalidInput);
        }
        let digest = Sha256::digest(key.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        Ok(Self {
            marker: dir.join(format!("{digest}.started")),
            result: dir.join(format!("{digest}.result")),
            record: Record {
                format: 1,
                principal: p.id.clone(),
                generation: p.generation,
                summary: None,
            },
        })
    }
    fn previous(&self) -> Result<Option<Summary>> {
        match std::fs::symlink_metadata(&self.marker) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(Error::Io),
            Ok(_) => {}
        }
        let old: Record =
            serde_json::from_slice(&rustsimhub_auth::read_private(&self.marker, 4096)?)
                .map_err(|_| Error::InvalidInput)?;
        if old.format != 1
            || old.principal != self.record.principal
            || old.generation != self.record.generation
        {
            return Err(Error::Forbidden);
        }
        // An uncertain/truncated request is never automatically activated again.
        let done: Record =
            serde_json::from_slice(&rustsimhub_auth::read_private(&self.result, 4096)?)
                .map_err(|_| Error::Forbidden)?;
        if done.format != 1 || done.principal != old.principal || done.generation != old.generation
        {
            return Err(Error::Forbidden);
        }
        done.summary.ok_or(Error::Forbidden).map(Some)
    }
    fn begin(&self) -> Result<()> {
        if std::fs::read_dir(self.marker.parent().ok_or(Error::Forbidden)?)
            .map_err(|_| Error::Io)?
            .take(257)
            .count()
            >= 256
        {
            return Err(Error::Busy);
        }
        rustsimhub_auth::write_private_new(
            &self.marker,
            &serde_json::to_vec(&self.record).map_err(|_| Error::InvalidInput)?,
        )
    }
    fn finish(&mut self, s: Summary) -> Result<()> {
        self.record.summary = Some(s);
        rustsimhub_auth::write_private_new(
            &self.result,
            &serde_json::to_vec(&self.record).map_err(|_| Error::InvalidInput)?,
        )
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Snapshot {
    owner: String,
    mm_owner: String,
    wifi: OwnedObjectPath,
    usb: OwnedObjectPath,
    profile: OwnedObjectPath,
    wwan: bool,
    profile_identity: [u8; 32],
}
trait Backend {
    fn snapshot(&self) -> impl std::future::Future<Output = Result<Snapshot>> + Send;
    fn fence(&self, b: &Snapshot) -> impl std::future::Future<Output = Result<()>> + Send;
    fn checkpoint(
        &self,
        b: &Snapshot,
    ) -> impl std::future::Future<Output = Result<OwnedObjectPath>> + Send;
    fn activate(&self, b: &Snapshot) -> impl std::future::Future<Output = Result<()>> + Send;
    fn restore(
        &self,
        b: &Snapshot,
        c: &OwnedObjectPath,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    fn restored(&self, b: &Snapshot) -> impl std::future::Future<Output = Result<()>> + Send;
}
trait Gate {
    fn check(&self) -> impl std::future::Future<Output = Result<()>> + Send;
}
struct Authorization {
    auth: Arc<Mutex<Auth>>,
    token: String,
    expected: Principal,
}
async fn principal(auth: Arc<Mutex<Auth>>, token: String) -> Result<Principal> {
    tokio::task::spawn_blocking(move || {
        auth.try_lock()
            .map_err(|_| Error::Busy)?
            .authorize(&token, Scope::NetworkManage)
    })
    .await
    .map_err(|_| Error::Busy)?
}
impl Gate for Authorization {
    async fn check(&self) -> Result<()> {
        if principal(self.auth.clone(), self.token.clone()).await? != self.expected {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
}
async fn execute(b: &impl Backend, g: &impl Gate, j: &mut Journal) -> Result<Summary> {
    g.check().await?;
    let initial = b.snapshot().await?;
    g.check().await?;
    b.fence(&initial).await?;
    j.begin()?; // Durable intent before the first side effect; no replay after process exit.
    g.check().await?;
    b.fence(&initial).await?;
    let checkpoint = b.checkpoint(&initial).await?;
    let result = async {
        g.check().await?;
        b.fence(&initial).await?;
        b.activate(&initial).await?;
        g.check().await?;
        Ok::<_, Error>(())
    }
    .await;
    // Compensation for this exact checkpoint is allowed even after client revocation.
    // A changed NM owner is rejected by restore; its own finite timer remains responsible.
    #[cfg(feature = "dev-fixtures")]
    if result.is_ok() && fault_mode() {
        rustsimhub_auth::write_private_new(
            &j.marker.with_extension("activated"),
            b"wifi-only fault test; uncertain; never reapply",
        )?;
        std::future::pending::<()>().await; // Diagnostic process-kill point; NM still rolls back at 8s.
    }
    let restore = b.restore(&initial, &checkpoint).await;
    result?;
    restore?;
    b.restored(&initial).await?;
    g.check().await?;
    let summary = Summary {
        schema: 1,
        wifi_only: true,
        activation_requested: true,
        rollback_verified: true,
        cellular_activation: false,
        hardware_mutations: true,
    };
    j.finish(summary.clone())?;
    Ok(summary)
}
pub(crate) async fn authorized(
    auth: Arc<Mutex<Auth>>,
    token: String,
    key: String,
) -> Result<serde_json::Value> {
    let dir = DIRECTORY.get().ok_or(Error::Forbidden)?;
    let p = principal(auth.clone(), token.clone()).await?;
    let _slot = TRIALS.try_acquire().map_err(|_| Error::Busy)?;
    let mut journal = Journal::new(dir, &key, &p)?;
    if let Some(s) = journal.previous()? {
        if principal(auth.clone(), token.clone()).await? != p {
            return Err(Error::Forbidden);
        }
        return serde_json::to_value(s).map_err(|_| Error::InvalidInput);
    }
    let gate = Authorization {
        auth,
        token,
        expected: p,
    };
    let result = timeout(Duration::from_secs(13), async {
        let c = zbus::Connection::system().await.map_err(|_| Error::Io)?;
        execute(&NetworkManager { c }, &gate, &mut journal).await
    })
    .await
    .map_err(|_| Error::Busy)??;
    serde_json::to_value(result).map_err(|_| Error::InvalidInput)
}
struct NetworkManager {
    c: zbus::Connection,
}
impl NetworkManager {
    async fn owner(&self, name: &str) -> Result<String> {
        let bus = zbus::fdo::DBusProxy::new(&self.c)
            .await
            .map_err(|_| Error::Io)?;
        Ok(bus
            .get_name_owner(name.try_into().map_err(|_| Error::Io)?)
            .await
            .map_err(|_| Error::Io)?
            .to_string())
    }
    async fn proxy<'a>(
        &'a self,
        owner: &'a str,
        path: &'a str,
        interface: &'a str,
    ) -> Result<zbus::Proxy<'a>> {
        zbus::Proxy::new(&self.c, owner, path, interface)
            .await
            .map_err(|_| Error::Io)
    }
    async fn idle_calls(&self, owner: &str) -> Result<()> {
        // Fixed first board modem only; no shell/AT and no Voice control methods.
        let voice = self
            .proxy(
                owner,
                "/org/freedesktop/ModemManager1/Modem/0",
                "org.freedesktop.ModemManager1.Modem.Voice",
            )
            .await?;
        let paths: Vec<OwnedObjectPath> =
            voice.get_property("Calls").await.map_err(|_| Error::Io)?;
        if paths.len() > 64 {
            return Err(Error::Forbidden);
        }
        for p in paths {
            if !p
                .as_str()
                .starts_with("/org/freedesktop/ModemManager1/Call/")
            {
                return Err(Error::Forbidden);
            }
            let call = self
                .proxy(owner, p.as_str(), "org.freedesktop.ModemManager1.Call")
                .await?;
            let state: i32 = call.get_property("State").await.map_err(|_| Error::Io)?;
            if state != 7 {
                return Err(Error::Busy);
            }
        }
        if self.owner(MM).await? != owner {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
    async fn active_profile(&self, owner: &str, path: &OwnedObjectPath) -> Result<OwnedObjectPath> {
        let d = self
            .proxy(
                owner,
                path.as_str(),
                "org.freedesktop.NetworkManager.Device",
            )
            .await?;
        let state: u32 = d.get_property("State").await.map_err(|_| Error::Io)?;
        if state != 100 {
            return Err(Error::Forbidden);
        }
        let active: OwnedObjectPath = d
            .get_property("ActiveConnection")
            .await
            .map_err(|_| Error::Io)?;
        if !active
            .as_str()
            .starts_with("/org/freedesktop/NetworkManager/ActiveConnection/")
        {
            return Err(Error::Forbidden);
        }
        let a = self
            .proxy(
                owner,
                active.as_str(),
                "org.freedesktop.NetworkManager.Connection.Active",
            )
            .await?;
        let profile: OwnedObjectPath = a.get_property("Connection").await.map_err(|_| Error::Io)?;
        if !profile
            .as_str()
            .starts_with("/org/freedesktop/NetworkManager/Settings/")
        {
            return Err(Error::Forbidden);
        }
        Ok(profile)
    }
    async fn profile_identity(&self, owner: &str, profile: &OwnedObjectPath) -> Result<[u8; 32]> {
        let settings = self
            .proxy(
                owner,
                profile.as_str(),
                "org.freedesktop.NetworkManager.Settings.Connection",
            )
            .await?;
        let values: HashMap<String, HashMap<String, OwnedValue>> = settings
            .call("GetSettings", &())
            .await
            .map_err(|_| Error::Io)?;
        let connection = values.get("connection").ok_or(Error::Forbidden)?;
        let kind = connection
            .get("type")
            .and_then(|v| <&str>::try_from(v).ok());
        let uuid = connection
            .get("uuid")
            .and_then(|v| <&str>::try_from(v).ok())
            .ok_or(Error::Forbidden)?;
        if kind != Some("802-11-wireless") || uuid.len() != 36 {
            return Err(Error::Forbidden);
        }
        let ssid: Vec<u8> = values
            .get("802-11-wireless")
            .and_then(|v| v.get("ssid"))
            .ok_or(Error::Forbidden)?
            .try_clone()
            .map_err(|_| Error::Io)?
            .try_into()
            .map_err(|_| Error::Forbidden)?;
        if ssid.is_empty() || ssid.len() > 32 {
            return Err(Error::Forbidden);
        }
        Ok(
            Sha256::digest(serde_json::to_vec(&(uuid, ssid)).map_err(|_| Error::InvalidInput)?)
                .into(),
        )
    }
    async fn cellular_autoconnect_absent(&self, owner: &str) -> Result<()> {
        let p = self
            .proxy(
                owner,
                "/org/freedesktop/NetworkManager/Settings",
                "org.freedesktop.NetworkManager.Settings",
            )
            .await?;
        let paths: Vec<OwnedObjectPath> = p
            .call("ListConnections", &())
            .await
            .map_err(|_| Error::Io)?;
        if paths.len() > 64 {
            return Err(Error::Forbidden);
        }
        for path in paths {
            if !path
                .as_str()
                .starts_with("/org/freedesktop/NetworkManager/Settings/")
            {
                return Err(Error::Forbidden);
            }
            let p = self
                .proxy(
                    owner,
                    path.as_str(),
                    "org.freedesktop.NetworkManager.Settings.Connection",
                )
                .await?;
            let settings: HashMap<String, HashMap<String, OwnedValue>> =
                p.call("GetSettings", &()).await.map_err(|_| Error::Io)?;
            let c = settings.get("connection").ok_or(Error::Forbidden)?;
            let kind = c
                .get("type")
                .and_then(|v| <&str>::try_from(v).ok())
                .ok_or(Error::Forbidden)?;
            if matches!(kind, "gsm" | "cdma")
                && c.get("autoconnect")
                    .and_then(|v| bool::try_from(v).ok())
                    .unwrap_or(true)
            {
                return Err(Error::Forbidden);
            }
        }
        Ok(())
    }
    #[cfg(feature = "dev-fixtures")]
    async fn fault_activate(&self, b: &Snapshot) -> Result<()> {
        self.cellular_autoconnect_absent(&b.owner).await?;
        let saved = self
            .proxy(
                &b.owner,
                b.profile.as_str(),
                "org.freedesktop.NetworkManager.Settings.Connection",
            )
            .await?;
        let settings: HashMap<String, HashMap<String, OwnedValue>> = saved
            .call("GetSettings", &())
            .await
            .map_err(|_| Error::Io)?;
        let security = settings
            .get("802-11-wireless-security")
            .and_then(|v| v.get("key-mgmt"))
            .and_then(|v| <&str>::try_from(v).ok());
        if security != Some("wpa-psk") {
            return Err(Error::Forbidden);
        }
        let ssid: Vec<u8> = settings
            .get("802-11-wireless")
            .and_then(|v| v.get("ssid"))
            .ok_or(Error::Forbidden)?
            .try_clone()
            .map_err(|_| Error::Io)?
            .try_into()
            .map_err(|_| Error::Forbidden)?;
        let text = b
            .profile_identity
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect::<String>();
        let uuid = format!(
            "{}-{}-4{}-8{}-{}",
            &text[..8],
            &text[8..12],
            &text[12..15],
            &text[15..18],
            &text[18..30]
        );
        // The development-only fault uses the saved SSID, deliberately wrong PSK and no IP.
        // It cannot join an unrelated open AP or create a data/default route.
        let v = |s: &str| OwnedValue::from(zbus::zvariant::Str::from(s));
        let config = HashMap::from([
            (
                "connection",
                HashMap::from([
                    ("id", v("rustsimhub-wifi-fault")),
                    ("uuid", v(&uuid)),
                    ("type", v("802-11-wireless")),
                    ("interface-name", v("wlan0")),
                    ("autoconnect", OwnedValue::from(false)),
                ]),
            ),
            (
                "802-11-wireless",
                HashMap::from([
                    (
                        "ssid",
                        OwnedValue::try_from(zbus::zvariant::Value::from(ssid))
                            .map_err(|_| Error::InvalidInput)?,
                    ),
                    ("mode", v("infrastructure")),
                ]),
            ),
            (
                "802-11-wireless-security",
                HashMap::from([("key-mgmt", v("wpa-psk")), ("psk", v(&"0".repeat(64)))]),
            ),
            (
                "ipv4",
                HashMap::from([
                    ("method", v("disabled")),
                    ("never-default", OwnedValue::from(true)),
                ]),
            ),
            (
                "ipv6",
                HashMap::from([
                    ("method", v("disabled")),
                    ("never-default", OwnedValue::from(true)),
                ]),
            ),
        ]);
        let options = HashMap::from([
            ("persist", v("volatile")),
            ("bind-activation", v("dbus-client")),
        ]);
        let nm = self.proxy(&b.owner, ROOT, NM).await?;
        let reply: (
            OwnedObjectPath,
            OwnedObjectPath,
            HashMap<String, OwnedValue>,
        ) = nm
            .call(
                "AddAndActivateConnection2",
                &(
                    config,
                    &b.wifi,
                    OwnedObjectPath::try_from("/").map_err(|_| Error::InvalidInput)?,
                    options,
                ),
            )
            .await
            .map_err(|_| Error::Io)?;
        if !reply
            .0
            .as_str()
            .starts_with("/org/freedesktop/NetworkManager/Settings/")
            || !reply
                .1
                .as_str()
                .starts_with("/org/freedesktop/NetworkManager/ActiveConnection/")
        {
            return Err(Error::Forbidden);
        }
        self.owner_fence(b).await
    }
    async fn owner_fence(&self, b: &Snapshot) -> Result<()> {
        if self.owner(NM).await? != b.owner {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
}
impl NetworkManager {
    async fn snapshot_inner(&self, first_setup: bool) -> Result<Snapshot> {
        let owner = self.owner(NM).await?;
        let mm_owner = self.owner(MM).await?;
        let nm = self.proxy(&owner, ROOT, NM).await?;
        let points: Vec<OwnedObjectPath> = nm
            .get_property("Checkpoints")
            .await
            .map_err(|_| Error::Io)?;
        if !points.is_empty() {
            return Err(Error::Busy);
        }
        let wwan: bool = nm
            .get_property("WwanEnabled")
            .await
            .map_err(|_| Error::Io)?;
        let paths: Vec<OwnedObjectPath> =
            nm.call("GetDevices", &()).await.map_err(|_| Error::Io)?;
        if paths.len() > 16 {
            return Err(Error::Forbidden);
        }
        let mut wifi = None;
        let mut usb = None;
        let mut seen = std::collections::HashSet::new();
        for p in paths {
            if !p
                .as_str()
                .starts_with("/org/freedesktop/NetworkManager/Devices/")
                || !seen.insert(p.clone())
            {
                return Err(Error::Forbidden);
            }
            let d = self
                .proxy(&owner, p.as_str(), "org.freedesktop.NetworkManager.Device")
                .await?;
            let kind: u32 = d.get_property("DeviceType").await.map_err(|_| Error::Io)?;
            let interface: String = d.get_property("Interface").await.map_err(|_| Error::Io)?;
            let state: u32 = d.get_property("State").await.map_err(|_| Error::Io)?;
            let managed: bool = d.get_property("Managed").await.map_err(|_| Error::Io)?;
            if kind == 2
                && interface == "wlan0"
                && managed
                && (state == 100 || (first_setup && state == 30))
                && wifi.replace(p.clone()).is_some()
            {
                return Err(Error::Forbidden);
            }
            if kind == 1
                && interface == "usb0"
                && managed
                && state == 100
                && usb.replace(p.clone()).is_some()
            {
                return Err(Error::Forbidden);
            }
        }
        let (wifi, usb) = (wifi.ok_or(Error::Forbidden)?, usb.ok_or(Error::Forbidden)?);
        let device = self
            .proxy(
                &owner,
                wifi.as_str(),
                "org.freedesktop.NetworkManager.Device",
            )
            .await?;
        let state: u32 = device.get_property("State").await.map_err(|_| Error::Io)?;
        let profile = if first_setup && state == 30 {
            "/".try_into().map_err(|_| Error::Io)?
        } else {
            self.active_profile(&owner, &wifi).await?
        };
        let profile_identity = if profile.as_str() == "/" {
            [0; 32]
        } else {
            self.profile_identity(&owner, &profile).await?
        };
        drop(nm);
        #[cfg(feature = "dev-fixtures")]
        if fault_mode() {
            self.cellular_autoconnect_absent(&owner).await?;
        }
        let b = Snapshot {
            owner,
            mm_owner,
            wifi,
            usb,
            profile,
            wwan,
            profile_identity,
        };
        self.fence(&b).await?;
        Ok(b)
    }
}
impl Backend for NetworkManager {
    async fn snapshot(&self) -> Result<Snapshot> {
        self.snapshot_inner(false).await
    }
    async fn fence(&self, b: &Snapshot) -> Result<()> {
        self.owner_fence(b).await?;
        if self.owner(MM).await? != b.mm_owner {
            return Err(Error::Forbidden);
        }
        self.idle_calls(&b.mm_owner).await?;
        let wifi = self
            .proxy(
                &b.owner,
                b.wifi.as_str(),
                "org.freedesktop.NetworkManager.Device",
            )
            .await?;
        let kind: u32 = wifi
            .get_property("DeviceType")
            .await
            .map_err(|_| Error::Io)?;
        let interface: String = wifi
            .get_property("Interface")
            .await
            .map_err(|_| Error::Io)?;
        let managed: bool = wifi.get_property("Managed").await.map_err(|_| Error::Io)?;
        if kind != 2
            || interface != "wlan0"
            || !managed
            || (b.profile.as_str() != "/"
                && self.profile_identity(&b.owner, &b.profile).await? != b.profile_identity)
        {
            return Err(Error::Forbidden);
        }
        self.active_profile(&b.owner, &b.usb).await?;
        let nm = self.proxy(&b.owner, ROOT, NM).await?;
        let wwan: bool = nm
            .get_property("WwanEnabled")
            .await
            .map_err(|_| Error::Io)?;
        if wwan != b.wwan {
            return Err(Error::Forbidden);
        }
        self.owner_fence(b).await
    }
    async fn checkpoint(&self, b: &Snapshot) -> Result<OwnedObjectPath> {
        let nm = self.proxy(&b.owner, ROOT, NM).await?;
        let cp: OwnedObjectPath = nm
            .call("CheckpointCreate", &(vec![b.wifi.clone()], 8u32, 0u32))
            .await
            .map_err(|_| Error::Io)?;
        if !cp
            .as_str()
            .starts_with("/org/freedesktop/NetworkManager/Checkpoint/")
        {
            return Err(Error::Forbidden);
        }
        self.owner_fence(b).await?;
        Ok(cp)
    }
    async fn activate(&self, b: &Snapshot) -> Result<()> {
        if self.active_profile(&b.owner, &b.wifi).await? != b.profile {
            return Err(Error::Forbidden);
        }
        #[cfg(feature = "dev-fixtures")]
        if fault_mode() {
            return self.fault_activate(b).await;
        }
        let nm = self.proxy(&b.owner, ROOT, NM).await?;
        let root = OwnedObjectPath::try_from("/").map_err(|_| Error::InvalidInput)?;
        let active: OwnedObjectPath = nm
            .call("ActivateConnection", &(&b.profile, &b.wifi, root))
            .await
            .map_err(|_| Error::Io)?;
        if !active
            .as_str()
            .starts_with("/org/freedesktop/NetworkManager/ActiveConnection/")
        {
            return Err(Error::Forbidden);
        }
        self.owner_fence(b).await?;
        Ok(())
    }
    async fn restore(&self, b: &Snapshot, cp: &OwnedObjectPath) -> Result<()> {
        self.owner_fence(b).await?;
        let nm = self.proxy(&b.owner, ROOT, NM).await?;
        let results: HashMap<String, u32> = nm
            .call("CheckpointRollback", &(cp,))
            .await
            .map_err(|_| Error::Io)?;
        if results.len() != 1 || results.get(b.wifi.as_str()) != Some(&0) {
            return Err(Error::Io);
        }
        // Only our exact checkpoint may be destroyed; never the empty/all checkpoint selector.
        let points: Vec<OwnedObjectPath> = nm
            .get_property("Checkpoints")
            .await
            .map_err(|_| Error::Io)?;
        if points.contains(cp) {
            nm.call::<_, _, ()>("CheckpointDestroy", &(cp,))
                .await
                .map_err(|_| Error::Io)?;
        }
        self.owner_fence(b).await
    }
    async fn restored(&self, b: &Snapshot) -> Result<()> {
        timeout(Duration::from_secs(3), async {
            loop {
                self.fence(b).await?;
                if self.active_profile(&b.owner, &b.wifi).await.ok().as_ref() == Some(&b.profile) {
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| Error::Busy)?
    }
}
#[cfg(test)]
mod tests;
