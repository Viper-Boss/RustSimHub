// SPDX-License-Identifier: AGPL-3.0-only
//! Authenticated, bounded, read-only status; never a network transaction permit.
use super::*;
use serde::Deserialize;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(in super::super) enum Wifi {
    Connected,
    Disconnected,
    Unknown,
}
#[derive(Serialize)]
pub struct StatusV1 {
    contract: u8,
    source: &'static str,
    wifi: Wifi,
    busy: bool,
    recovery_available: bool,
}
pub(in super::super) struct Facts {
    pub wifi: Wifi,
    pub busy: bool,
    pub available: bool,
}
impl Facts {
    fn unknown() -> Self {
        Self {
            wifi: Wifi::Unknown,
            busy: true,
            available: false,
        }
    }
}
pub(in super::super) trait StatusBackend {
    fn observe(&self) -> impl std::future::Future<Output = Result<Facts>> + Send;
}
#[derive(Default)]
pub(in super::super) struct NetworkStatus {
    // Never supplied by an inbound packet. None selects the actual system bus.
    pub connection: Option<zbus::Connection>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
async fn proxy<'a>(
    nm: &'a NetworkManager,
    owner: &'a str,
    path: &'a str,
    interface: &'a str,
) -> Result<zbus::Proxy<'a>> {
    zbus::proxy::Builder::<zbus::Proxy<'a>>::new(&nm.c)
        .destination(owner)
        .map_err(|_| Error::Io)?
        .path(path)
        .map_err(|_| Error::Io)?
        .interface(interface)
        .map_err(|_| Error::Io)?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .map_err(|_| Error::Io)
}
pub(super) async fn read<S: StatusBackend>(
    auth: Arc<Mutex<Auth>>,
    request: AuthorizedRecoveryRequest,
    backend: S,
) -> Result<StatusV1> {
    let (request, admitted) = with_auth(auth.clone(), move |a| {
        let admitted = a.recovery_execution_available(&request)?;
        crate::recovery_body::parse::<Empty>(&request.body)?;
        Ok((request, admitted))
    })
    .await?;
    let facts = backend.observe().await.unwrap_or_else(|_| Facts::unknown());
    // No properties, locks or grants acquired here are execution authority.
    // A disconnect/revoke during the actual backend await discards all facts.
    let current = with_auth(auth, move |a| a.recovery_execution_available(&request)).await?;
    let actor_busy = TRIALS.available_permits() == 0
        || ACTIVE
            .try_lock()
            .map(|a| {
                a.as_ref()
                    .is_some_and(|s| s.state.try_lock().map(|s| !s.terminal()).unwrap_or(true))
            })
            .unwrap_or(true)
        || crate::coordination::global().network().is_err();
    let busy = facts.busy || actor_busy;
    Ok(StatusV1 {
        contract: 1,
        source: "core",
        wifi: facts.wifi,
        busy,
        recovery_available: admitted && current && facts.available && !busy,
    })
}
impl StatusBackend for NetworkStatus {
    async fn observe(&self) -> Result<Facts> {
        let c = match &self.connection {
            Some(c) => c.clone(),
            None => zbus::Connection::system().await.map_err(|_| Error::Io)?,
        };
        let nm = NetworkManager { c };
        let owner = nm.owner(NM).await?;
        let root = proxy(&nm, &owner, ROOT, NM).await?;
        let enabled: bool = root
            .get_property("WirelessEnabled")
            .await
            .map_err(|_| Error::Io)?;
        let checkpoints: Vec<OwnedObjectPath> = root
            .get_property("Checkpoints")
            .await
            .map_err(|_| Error::Io)?;
        if checkpoints.len() > 16 {
            return Err(Error::Forbidden);
        }
        let paths: Vec<OwnedObjectPath> =
            root.call("GetDevices", &()).await.map_err(|_| Error::Io)?;
        if paths.len() > 16 {
            return Err(Error::Forbidden);
        }
        let mut seen = std::collections::HashSet::new();
        let mut device = None;
        for path in paths {
            if !path
                .as_str()
                .starts_with("/org/freedesktop/NetworkManager/Devices/")
                || !seen.insert(path.clone())
            {
                return Err(Error::Forbidden);
            }
            let d = proxy(
                &nm,
                &owner,
                path.as_str(),
                "org.freedesktop.NetworkManager.Device",
            )
            .await?;
            let kind: u32 = d.get_property("DeviceType").await.map_err(|_| Error::Io)?;
            let interface: String = d.get_property("Interface").await.map_err(|_| Error::Io)?;
            if interface.len() > 32 {
                return Err(Error::Forbidden);
            }
            if kind == 2 && interface == "wlan0" && device.replace(path.clone()).is_some() {
                return Err(Error::Forbidden);
            }
        }
        let Some(path) = device else {
            return Ok(Facts::unknown());
        };
        let d = proxy(
            &nm,
            &owner,
            path.as_str(),
            "org.freedesktop.NetworkManager.Device",
        )
        .await?;
        let state: u32 = d.get_property("State").await.map_err(|_| Error::Io)?;
        let managed: bool = d.get_property("Managed").await.map_err(|_| Error::Io)?;
        let wifi = if !managed || !enabled {
            Wifi::Unknown
        } else {
            match state {
                30 => Wifi::Disconnected,
                100 => {
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
                    let active = proxy(
                        &nm,
                        &owner,
                        active.as_str(),
                        "org.freedesktop.NetworkManager.Connection.Active",
                    )
                    .await?;
                    let active: u32 = active.get_property("State").await.map_err(|_| Error::Io)?;
                    if active == 2 {
                        Wifi::Connected
                    } else {
                        Wifi::Unknown
                    }
                }
                _ => Wifi::Unknown,
            }
        };
        let permissions: HashMap<String, String> = root
            .call("GetPermissions", &())
            .await
            .map_err(|_| Error::Io)?;
        if permissions.len() > 64
            || permissions
                .iter()
                .any(|(k, v)| k.len() > 128 || v.len() > 16)
        {
            return Err(Error::Forbidden);
        }
        let permitted = [
            "network-control",
            "checkpoint-rollback",
            "settings.modify.system",
        ]
        .iter()
        .all(|name| {
            permissions
                .get(&format!("org.freedesktop.NetworkManager.{name}"))
                .is_some_and(|v| v == "yes")
        });
        let mm = nm.owner(MM).await?;
        let idle = match nm.idle_calls(&mm).await {
            Ok(()) => true,
            Err(Error::Busy) => false,
            Err(e) => return Err(e),
        };
        let final_state: u32 = d.get_property("State").await.map_err(|_| Error::Io)?;
        let final_kind: u32 = d.get_property("DeviceType").await.map_err(|_| Error::Io)?;
        let final_interface: String = d.get_property("Interface").await.map_err(|_| Error::Io)?;
        let final_managed: bool = d.get_property("Managed").await.map_err(|_| Error::Io)?;
        let final_enabled: bool = root
            .get_property("WirelessEnabled")
            .await
            .map_err(|_| Error::Io)?;
        if nm.owner(NM).await? != owner
            || nm.owner(MM).await? != mm
            || final_state != state
            || final_kind != 2
            || final_interface != "wlan0"
            || final_managed != managed
            || final_enabled != enabled
        {
            return Err(Error::Forbidden);
        }
        Ok(Facts {
            wifi,
            busy: !idle || !checkpoints.is_empty(),
            available: enabled
                && managed
                && permitted
                && matches!(wifi, Wifi::Connected | Wifi::Disconnected),
        })
    }
}
