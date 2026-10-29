// SPDX-License-Identifier: AGPL-3.0-only
//! One bounded scan before switching the fixed Wi-Fi radio into AP mode.
//! Trusted callers still supply the current Root lease/coordination gate. This is not an API
//! endpoint, does not change the connection, and never retries an uncertain RequestScan.
use super::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Duration};
use tokio::time::{sleep, timeout};

const ACCESS_POINT: &str = "org.freedesktop.NetworkManager.AccessPoint";
const AP_PREFIX: &str = "/org/freedesktop/NetworkManager/AccessPoint/";
pub const MAX_SCAN_SECONDS: u64 = 8;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Security {
    Wpa2Personal,
    Open,
    Unsupported,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub ssid: String,
    pub strength: u8,
    pub frequency_mhz: u32,
    pub security: Security,
}
impl Choice {
    fn valid(&self) -> bool {
        !self.ssid.is_empty()
            && self.ssid.len() <= 32
            && !self.ssid.chars().any(char::is_control)
            && self.strength <= 100
            && (2000..=8000).contains(&self.frequency_mhz)
    }
}
/// Display-only: no BSSID, device path, settings, credentials or connect instruction.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub requested_uptime_ms: u64,
    pub completed_uptime_ms: u64,
    pub last_scan_uptime_ms: u64,
    pub networks: Vec<Choice>,
    pub omitted: usize,
    pub truncated: bool,
}
impl Snapshot {
    pub fn valid(&self) -> bool {
        self.requested_uptime_ms <= self.last_scan_uptime_ms
            && self.last_scan_uptime_ms <= self.completed_uptime_ms
            && self
                .completed_uptime_ms
                .saturating_sub(self.requested_uptime_ms)
                <= MAX_SCAN_SECONDS * 1000
            && self.networks.len() <= 32
            && self.omitted <= 128
            && (!self.truncated || self.networks.len() == 32)
            && self.networks.iter().all(Choice::valid)
            && self.networks.iter().enumerate().all(|(i, n)| {
                !self.networks[..i].iter().any(|old| {
                    old.ssid == n.ssid
                        && old.security == n.security
                        && old.frequency_mhz == n.frequency_mhz
                })
            })
    }
}
fn choice(p: &Properties, started: u64, completed: u64) -> Result<Option<Choice>> {
    let seen = p
        .get("LastSeen")
        .and_then(|v| i32::try_from(v).ok())
        .ok_or(Error::Invalid)?;
    // NM retains older AP objects. LastSeen has second precision: compare within that
    // precision, not to the millisecond LastScan property. Unknown/old/future rows are omitted.
    if seen < 0 || (seen as u64) < started / 1000 || (seen as u64) > completed / 1000 {
        return Ok(None);
    }
    let bytes: Vec<u8> = p
        .get("Ssid")
        .ok_or(Error::Invalid)?
        .try_clone()
        .map_err(|_| Error::Invalid)?
        .try_into()
        .map_err(|_| Error::Invalid)?;
    if bytes.is_empty() || bytes.len() > 32 {
        return Ok(None);
    }
    let Ok(ssid) = String::from_utf8(bytes) else {
        return Ok(None);
    };
    if ssid.chars().any(char::is_control) || number(p, "Mode")? != 2 {
        return Ok(None);
    }
    let strength = p
        .get("Strength")
        .and_then(|v| u8::try_from(v).ok())
        .filter(|v| *v <= 100)
        .ok_or(Error::Invalid)?;
    let frequency_mhz = number(p, "Frequency")?;
    if !(2000..=8000).contains(&frequency_mhz) {
        return Ok(None);
    }
    let (flags, wpa, rsn) = (
        number(p, "Flags")?,
        number(p, "WpaFlags")?,
        number(p, "RsnFlags")?,
    );
    let security = if flags & 1 != 0 && rsn & 0x100 != 0 {
        Security::Wpa2Personal
    } else if flags & 1 == 0 && rsn == 0 && wpa == 0 {
        Security::Open
    } else {
        Security::Unsupported
    };
    Ok(Some(Choice {
        ssid,
        strength,
        frequency_mhz,
        security,
    }))
}
fn snapshot(rows: Vec<Choice>, omitted: usize, started: u64, done: u64, last: u64) -> Snapshot {
    let mut choices = BTreeMap::<(String, Security, u32), Choice>::new();
    for row in rows {
        let key = (row.ssid.clone(), row.security, row.frequency_mhz);
        if let Some(old) = choices.get_mut(&key) {
            old.strength = old.strength.max(row.strength);
        } else {
            choices.insert(key, row);
        }
    }
    let mut networks: Vec<_> = choices.into_values().collect();
    networks.sort_by(|a, b| {
        b.strength
            .cmp(&a.strength)
            .then(a.ssid.cmp(&b.ssid))
            .then(a.security.cmp(&b.security))
            .then(a.frequency_mhz.cmp(&b.frequency_mhz))
    });
    let truncated = networks.len() > 32;
    networks.truncate(32);
    Snapshot {
        requested_uptime_ms: started,
        completed_uptime_ms: done,
        last_scan_uptime_ms: last,
        networks,
        omitted,
        truncated,
    }
}
impl NetworkManager {
    /// Scanning can outlive the first preflight. Recheck radio/device/checkpoint context before
    /// it and again afterwards, including an unsupported/lost scan reply.
    pub(super) async fn scan_context(&self) -> Result<()> {
        self.fence().await?;
        self.idle_or_saved().await?;
        let root = self.properties(ROOT, NAME).await?;
        let points: Vec<OwnedObjectPath> = root
            .get("Checkpoints")
            .ok_or(Error::Invalid)?
            .try_clone()
            .map_err(|_| Error::Invalid)?
            .try_into()
            .map_err(|_| Error::Invalid)?;
        if !boolean(&root, "NetworkingEnabled")?
            || !boolean(&root, "WirelessEnabled")?
            || !points.is_empty()
        {
            return Err(Error::Changed);
        }
        let device = self
            .properties(self.wifi.as_ref().ok_or(Error::Denied)?.as_str(), DEVICE)
            .await?;
        if number(&device, "DeviceType")? != 2
            || text(&device, "Interface")? != INTERFACE
            || !boolean(&device, "Managed")?
        {
            return Err(Error::Changed);
        }
        self.fence().await
    }
    /// Only valid after a successful preflight and before any checkpoint/AP/trial action.
    /// One request per NetworkManager/session, including errors, lost replies and timeouts.
    /// Does not restart NM, switch a profile, enable a radio, or activate cellular data.
    pub async fn scan_before_ap(&mut self) -> Result<Snapshot> {
        self.scan_with_budget(Duration::from_secs(MAX_SCAN_SECONDS))
            .await
    }
    pub(super) async fn scan_with_budget(&mut self, budget: Duration) -> Result<Snapshot> {
        if !self.preflight_complete
            || self.scan_requested
            || self.checkpoint_requested
            || self.activation_requested
            || self.profile.is_some()
            || self.station.is_some()
        {
            return Err(Error::Denied);
        }
        // Consume before polling the future. Cancellation cannot allow a second RequestScan.
        self.scan_requested = true;
        timeout(budget, self.scan_inner())
            .await
            .map_err(|_| Error::Busy)?
    }
    async fn scan_inner(&self) -> Result<Snapshot> {
        self.scan_context().await?;
        let path = self.wifi.as_ref().ok_or(Error::Denied)?.as_str();
        let wireless = self.properties(path, WIRELESS).await?;
        if !matches!(number(&wireless, "Mode")?, 0 | 2) {
            return Err(Error::Denied);
        }
        let before = last_scan(&wireless)?;
        let started = crate::channel::uptime_ms()?;
        if before >= 0 && before as u64 > started {
            return Err(Error::Changed);
        }
        self.fence().await?;
        self.proxy(path, WIRELESS)
            .await?
            .call::<_, _, ()>("RequestScan", &(HashMap::<String, OwnedValue>::new(),))
            .await
            .map_err(|_| Error::Io)?;
        let last = loop {
            self.fence().await?;
            let value = last_scan(&self.properties(path, WIRELESS).await?)?;
            let now = crate::channel::uptime_ms()?;
            // A successful method reply is not a completed scan. Fresh NM objects alone
            // cannot substitute for this monotonic completion marker either.
            if value > before && value >= 0 && value as u64 >= started && value as u64 <= now {
                break value as u64;
            }
            if value < before || value >= 0 && value as u64 > now {
                return Err(Error::Changed);
            }
            sleep(Duration::from_millis(100)).await;
        };
        self.scan_context().await?;
        let paths: Vec<OwnedObjectPath> = self
            .proxy(path, WIRELESS)
            .await?
            .call("GetAllAccessPoints", &())
            .await
            .map_err(|_| Error::Io)?;
        if !paths_valid(&paths, AP_PREFIX, 128) {
            return Err(Error::Invalid);
        }
        let mut rows = Vec::new();
        let mut omitted = 0;
        for ap in paths {
            self.fence().await?;
            let p = self.properties(ap.as_str(), ACCESS_POINT).await?;
            match choice(&p, started, crate::channel::uptime_ms()?)? {
                Some(row) => rows.push(row),
                None => omitted += 1,
            }
        }
        self.scan_context().await?;
        let result = snapshot(rows, omitted, started, crate::channel::uptime_ms()?, last);
        if !result.valid() {
            return Err(Error::Invalid);
        }
        Ok(result)
    }
}
fn last_scan(p: &Properties) -> Result<i64> {
    p.get("LastScan")
        .and_then(|v| i64::try_from(v).ok())
        .filter(|v| *v >= -1)
        .ok_or(Error::Invalid)
}
#[cfg(test)]
mod tests;
