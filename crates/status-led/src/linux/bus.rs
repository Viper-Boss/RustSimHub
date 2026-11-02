// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed, read-only cached properties. Never Load/StartUnit, activate a network,
//! fetch settings/secrets, or request a connectivity probe.
use super::{Inputs, Result, BASE_UNITS, BUSINESS_UNITS};
use std::{collections::HashSet, time::Duration};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const SD: &str = "org.freedesktop.systemd1";
const SD_ROOT: &str = "/org/freedesktop/systemd1";
const SD_MANAGER: &str = "org.freedesktop.systemd1.Manager";
const NM: &str = "org.freedesktop.NetworkManager";
const NM_ROOT: &str = "/org/freedesktop/NetworkManager";
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";

pub(super) fn runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}

pub(super) async fn snapshot(complete: bool) -> Result<Inputs> {
    bounded(
        async {
            let connection =
                zbus::connection::Builder::address("unix:path=/run/dbus/system_bus_socket")?
                    .build()
                    .await?;
            probe(&connection, complete).await
        },
        Duration::from_secs(3),
    )
    .await
}

async fn bounded(
    future: impl std::future::Future<Output = Result<Inputs>>,
    budget: Duration,
) -> Result<Inputs> {
    tokio::time::timeout(budget, future)
        .await
        .map_err(|_| "indicator snapshot timed out")?
}

async fn property(
    c: &zbus::Connection,
    owner: &str,
    path: &str,
    interface: &str,
    name: &str,
) -> Result<OwnedValue> {
    // Explicit Get: no proxy property cache can outlive this observation.
    Ok(zbus::fdo::PropertiesProxy::builder(c)
        .destination(owner)?
        .path(path)?
        .build()
        .await?
        .get(interface.try_into()?, name)
        .await?)
}

pub(super) fn unit_flags(states: &[String], complete: bool) -> Result<(bool, bool)> {
    if states.len() != BASE_UNITS.len() + BUSINESS_UNITS.len()
        || states.iter().any(|s| {
            !matches!(
                s.as_str(),
                "active"
                    | "reloading"
                    | "inactive"
                    | "failed"
                    | "activating"
                    | "deactivating"
                    | "maintenance"
                    | "refreshing"
            )
        })
    {
        return Err("invalid unit state catalogue".into());
    }
    let (base, business) = states.split_at(BASE_UNITS.len());
    Ok((
        base.iter().any(|s| s == "failed") || complete && business.iter().any(|s| s == "failed"),
        base.iter().any(|s| s == "activating"),
    ))
}

async fn unit_state(c: &zbus::Connection, owner: &str, unit: &str) -> Result<String> {
    let manager = zbus::Proxy::new(c, owner, SD_ROOT, SD_MANAGER).await?;
    let path: OwnedObjectPath = match manager.call("GetUnit", &(unit,)).await {
        Ok(path) => path,
        Err(zbus::Error::MethodError(name, _, _))
            if name.as_str() == "org.freedesktop.systemd1.NoSuchUnit" =>
        {
            // An existing but unloaded unit has no live failed/activating state.
            // Verify the file exists without loading, starting, or resetting it.
            let state: String = manager.call("GetUnitFileState", &(unit,)).await?;
            if !matches!(
                state.as_str(),
                "enabled"
                    | "enabled-runtime"
                    | "linked"
                    | "linked-runtime"
                    | "alias"
                    | "masked"
                    | "masked-runtime"
                    | "static"
                    | "disabled"
                    | "indirect"
                    | "generated"
                    | "transient"
            ) {
                return Err("invalid unit file state".into());
            }
            return Ok("inactive".into());
        }
        Err(error) => return Err(error.into()),
    };
    if !path.as_str().starts_with("/org/freedesktop/systemd1/unit/") || path.as_str().len() > 512 {
        return Err("foreign unit object".into());
    }
    Ok(String::try_from(
        property(
            c,
            owner,
            path.as_str(),
            "org.freedesktop.systemd1.Unit",
            "ActiveState",
        )
        .await?,
    )?)
}

fn devices_valid(paths: &[OwnedObjectPath]) -> bool {
    let mut unique = HashSet::new();
    paths.len() <= 16
        && paths.iter().all(|p| {
            let Some(number) = p
                .as_str()
                .strip_prefix("/org/freedesktop/NetworkManager/Devices/")
            else {
                return false;
            };
            !number.is_empty()
                && number.len() <= 10
                && number.bytes().all(|b| b.is_ascii_digit())
                && unique.insert(p.as_str())
        })
}

async fn number(c: &zbus::Connection, owner: &str, path: &str, name: &str) -> Result<u32> {
    Ok(u32::try_from(
        property(c, owner, path, DEVICE, name).await?,
    )?)
}

async fn probe(c: &zbus::Connection, complete: bool) -> Result<Inputs> {
    let bus = zbus::fdo::DBusProxy::new(c).await?;
    let systemd = bus.get_name_owner(SD.try_into()?).await?;
    let network = bus.get_name_owner(NM.try_into()?).await?;
    let mut states = Vec::new();
    for unit in BASE_UNITS.iter().chain(&BUSINESS_UNITS) {
        states.push(unit_state(c, systemd.as_str(), unit).await?);
    }
    let (failed, activating) = unit_flags(&states, complete)?;
    let mut input = Inputs {
        error: failed,
        configuring: !complete || activating,
        ..Inputs::default()
    };
    let manager = zbus::Proxy::new(c, network.as_str(), NM_ROOT, NM).await?;
    let paths: Vec<OwnedObjectPath> = manager.call("GetDevices", &()).await?;
    if !devices_valid(&paths) {
        return Err("invalid network device catalogue".into());
    }
    for path in paths {
        let path = path.as_str();
        let kind = number(c, network.as_str(), path, "DeviceType").await?;
        if !matches!(kind, 2 | 8) {
            continue;
        }
        let state = number(c, network.as_str(), path, "State").await?;
        if state > 120 || state % 10 != 0 {
            return Err("invalid network state".into());
        }
        if state != 100 {
            continue;
        }
        if kind == 2 {
            let interface =
                String::try_from(property(c, network.as_str(), path, DEVICE, "Interface").await?)?;
            if interface.len() > 15 || interface.chars().any(char::is_control) {
                return Err("invalid interface".into());
            }
            if interface != "wlan0" {
                continue;
            }
            input.wifi_connected = true;
        } else {
            input.cellular_connected = true;
        }
        let ip4 = number(c, network.as_str(), path, "Ip4Connectivity").await?;
        let ip6 = number(c, network.as_str(), path, "Ip6Connectivity").await?;
        if ip4 > 4 || ip6 > 4 {
            return Err("invalid cached connectivity".into());
        }
        input.internet |= ip4 == 4 || ip6 == 4;
    }
    if bus.get_name_owner(SD.try_into()?).await? != systemd
        || bus.get_name_owner(NM.try_into()?).await? != network
    {
        return Err("service owner changed during observation".into());
    }
    Ok(input)
}

#[cfg(test)]
mod tests;
