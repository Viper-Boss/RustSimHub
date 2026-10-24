// SPDX-License-Identifier: AGPL-3.0-only
//! A nested checkpoint tests STA then restores the same RAM-only AP; no save operation here.
use super::*;
use crate::station_trial::{StationCredentials, StationNetwork};
use sha2::{Digest, Sha256};

pub(super) struct Trial {
    checkpoint: Option<OwnedObjectPath>,
    profile: Option<OwnedObjectPath>,
    activation_requested: bool,
    fingerprint: [u8; 32],
    profiles_before: Vec<OwnedObjectPath>,
}
pub struct Session<'a> {
    nm: &'a mut NetworkManager,
    ap: &'a Credentials,
}
impl NetworkManager {
    pub fn station_trial<'a>(&'a mut self, ap: &'a Credentials) -> Result<Session<'a>> {
        if !self.retain_ap {
            return Err(Error::Denied);
        }
        Ok(Session { nm: self, ap })
    }
    async fn checkpoints(&self) -> Result<Vec<OwnedObjectPath>> {
        self.fence().await?;
        let p = self.properties(ROOT, NAME).await?;
        let paths: Vec<OwnedObjectPath> = p
            .get("Checkpoints")
            .ok_or(Error::Io)?
            .try_clone()
            .map_err(|_| Error::Io)?
            .try_into()
            .map_err(|_| Error::Io)?;
        if !paths_valid(&paths, "/org/freedesktop/NetworkManager/Checkpoint/", 2) {
            return Err(Error::Changed);
        }
        Ok(paths)
    }
    async fn trial_checkpoints(&self, require_inner: bool) -> Result<bool> {
        let paths = self.checkpoints().await?;
        let outer = self.checkpoint.as_ref().ok_or(Error::Denied)?;
        if !paths.contains(outer) {
            return Err(Error::Changed);
        }
        let inner = self.station.as_ref().and_then(|s| s.checkpoint.as_ref());
        if paths.iter().any(|p| p != outer && Some(p) != inner) {
            return Err(Error::Changed);
        }
        let present = inner.is_some_and(|p| paths.contains(p));
        if require_inner && !present {
            return Err(Error::Changed);
        }
        Ok(present)
    }
    async fn ap_profile_intact(&self, ap: &Credentials) -> Result<()> {
        self.fence().await?;
        if !profile_matches(
            &self
                .get_settings(self.profile.as_ref().ok_or(Error::Denied)?)
                .await?,
            ap,
        )? {
            return Err(Error::Changed);
        }
        Ok(())
    }
}
fn fingerprint(c: &StationCredentials) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(c.uuid.as_bytes());
    h.update((c.ssid.len() as u64).to_be_bytes());
    h.update(c.ssid.as_bytes());
    h.update(c.passphrase.as_bytes());
    h.finalize().into()
}
fn settings(c: &StationCredentials) -> Result<Settings> {
    Ok(HashMap::from([
        (
            "connection".into(),
            HashMap::from([
                ("id".into(), string("RustSimHub station test")),
                ("uuid".into(), string(&c.uuid)),
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
                    OwnedValue::try_from(Value::from(c.ssid.as_bytes().to_vec()))
                        .map_err(|_| Error::Invalid)?,
                ),
                ("mode".into(), string("infrastructure")),
                ("hidden".into(), true.into()),
            ]),
        ),
        (
            "802-11-wireless-security".into(),
            HashMap::from([
                ("key-mgmt".into(), string("wpa-psk")),
                ("proto".into(), strings(vec!["rsn"])?),
                ("pairwise".into(), strings(vec!["ccmp"])?),
                ("group".into(), strings(vec!["ccmp"])?),
                ("psk".into(), string(&c.passphrase)),
                ("psk-flags".into(), 0u32.into()),
            ]),
        ),
        (
            "ipv4".into(),
            HashMap::from([
                ("method".into(), string("auto")),
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
fn matches(s: &Settings, c: &StationCredentials) -> Result<bool> {
    let conn = s.get("connection").ok_or(Error::Io)?;
    let wifi = s.get("802-11-wireless").ok_or(Error::Io)?;
    let security = s.get("802-11-wireless-security").ok_or(Error::Io)?;
    let ip = s.get("ipv4").ok_or(Error::Io)?;
    let ipv6 = s.get("ipv6").ok_or(Error::Io)?;
    let ssid: Vec<u8> = wifi
        .get("ssid")
        .ok_or(Error::Io)?
        .try_clone()
        .map_err(|_| Error::Io)?
        .try_into()
        .map_err(|_| Error::Io)?;
    let vector = |key: &str| -> Result<Vec<String>> {
        security
            .get(key)
            .ok_or(Error::Io)?
            .try_clone()
            .map_err(|_| Error::Io)?
            .try_into()
            .map_err(|_| Error::Io)
    };
    Ok(text(conn, "uuid")? == c.uuid
        && text(conn, "type")? == "802-11-wireless"
        && text(conn, "interface-name")? == INTERFACE
        && !boolean(conn, "autoconnect")?
        && text(wifi, "mode")? == "infrastructure"
        && ssid == c.ssid.as_bytes()
        && text(security, "key-mgmt")? == "wpa-psk"
        && vector("proto")? == ["rsn"]
        && vector("pairwise")? == ["ccmp"]
        && vector("group")? == ["ccmp"]
        && text(ip, "method")? == "auto"
        && !boolean(ip, "may-fail")?
        && text(ipv6, "method")? == "disabled"
        && boolean(ipv6, "never-default")?)
}
impl StationNetwork for Session<'_> {
    async fn begin(&mut self, c: &StationCredentials) -> Result<()> {
        let nm = &mut self.nm;
        if nm.station.is_some() || !nm.ready(self.ap).await? {
            return Err(Error::Denied);
        }
        nm.trial_checkpoints(false).await?;
        let profiles_before = nm.connections().await?;
        for p in &profiles_before {
            let s = nm.get_settings(p).await?;
            let conn = s.get("connection").ok_or(Error::Io)?;
            let kind = text(conn, "type")?;
            if text(conn, "uuid")? == c.uuid
                || kind == "802-11-wireless" && Some(p) != nm.profile.as_ref()
                || kind == "gsm" && boolean(conn, "autoconnect")?
            {
                return Err(Error::Denied);
            }
        }
        nm.station = Some(Trial {
            checkpoint: None,
            profile: None,
            activation_requested: false,
            fingerprint: fingerprint(c),
            profiles_before,
        });
        // Only this already-owned AP checkpoint may overlap. Never destroy another checkpoint.
        let cp: OwnedObjectPath = nm
            .proxy(ROOT, NAME)
            .await?
            .call(
                "CheckpointCreate",
                &(vec![nm.wifi.as_ref().ok_or(Error::Denied)?], 45u32, 8u32),
            )
            .await
            .map_err(|_| Error::Io)?;
        if !cp
            .as_str()
            .starts_with("/org/freedesktop/NetworkManager/Checkpoint/")
            || Some(&cp) == nm.checkpoint.as_ref()
        {
            return Err(Error::Changed);
        }
        nm.station.as_mut().ok_or(Error::Io)?.checkpoint = Some(cp);
        nm.trial_checkpoints(true).await?;
        if !nm.ready(self.ap).await? {
            return Err(Error::Changed);
        }
        nm.station.as_mut().ok_or(Error::Io)?.activation_requested = true;
        let (profile, active, _): (OwnedObjectPath, OwnedObjectPath, Properties) = nm
            .proxy(ROOT, NAME)
            .await?
            .call(
                "AddAndActivateConnection2",
                &(
                    settings(c)?,
                    nm.wifi.as_ref().ok_or(Error::Denied)?,
                    OwnedObjectPath::try_from("/").map_err(|_| Error::Invalid)?,
                    HashMap::from([
                        ("persist", string("memory")),
                        ("bind-activation", string("dbus-client")),
                    ]),
                ),
            )
            .await
            .map_err(|_| Error::Io)?;
        if !profile
            .as_str()
            .starts_with("/org/freedesktop/NetworkManager/Settings/")
            || Some(&profile) == nm.profile.as_ref()
            || !active
                .as_str()
                .starts_with("/org/freedesktop/NetworkManager/ActiveConnection/")
        {
            return Err(Error::Changed);
        }
        nm.station.as_mut().ok_or(Error::Io)?.profile = Some(profile);
        nm.fence().await
    }
    async fn usable(&self, c: &StationCredentials) -> Result<bool> {
        let nm = &self.nm;
        nm.trial_checkpoints(true).await?;
        nm.ap_profile_intact(self.ap).await?;
        let s = nm.station.as_ref().ok_or(Error::Denied)?;
        if s.fingerprint != fingerprint(c) {
            return Err(Error::Changed);
        }
        let profile = s.profile.as_ref().ok_or(Error::Denied)?;
        if !matches(&nm.get_settings(profile).await?, c)? {
            return Err(Error::Changed);
        }
        let device = nm
            .properties(nm.wifi.as_ref().ok_or(Error::Denied)?.as_str(), DEVICE)
            .await?;
        if number(&device, "State")? != 100 {
            return Ok(false);
        }
        let active = path(&device, "ActiveConnection")?;
        if !active
            .as_str()
            .starts_with("/org/freedesktop/NetworkManager/ActiveConnection/")
        {
            return Err(Error::Changed);
        }
        if path(
            &nm.properties(
                active.as_str(),
                "org.freedesktop.NetworkManager.Connection.Active",
            )
            .await?,
            "Connection",
        )? != *profile
            || number(
                &nm.properties(nm.wifi.as_ref().ok_or(Error::Denied)?.as_str(), WIRELESS)
                    .await?,
                "Mode",
            )? != 2
        {
            return Err(Error::Changed);
        }
        let ip = path(&device, "Ip4Config")?;
        if ip.as_str() == "/" {
            return Ok(false);
        }
        if !ip
            .as_str()
            .starts_with("/org/freedesktop/NetworkManager/IP4Config/")
        {
            return Err(Error::Changed);
        }
        let rows = addresses(
            &nm.properties(ip.as_str(), "org.freedesktop.NetworkManager.IP4Config")
                .await?,
        )?;
        if rows.len() > 16 {
            return Err(Error::Invalid);
        }
        let mut usable = false;
        for row in rows {
            let ip = text(&row, "address")?
                .parse::<Ipv4Addr>()
                .map_err(|_| Error::Invalid)?;
            let prefix = number(&row, "prefix")?;
            if prefix == 0 || subnet_conflict(&ip.to_string(), prefix)? {
                return Err(Error::Changed);
            }
            usable |= !ip.is_unspecified()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_multicast()
                && !ip.is_broadcast();
        }
        nm.fence().await?;
        Ok(usable)
    }
    async fn restore_ap(&mut self, c: &StationCredentials) -> Result<()> {
        let nm = &mut self.nm;
        nm.fence().await?;
        let Some(trial) = nm.station.as_ref() else {
            return if nm.ready(self.ap).await? {
                Ok(())
            } else {
                Err(Error::Changed)
            };
        };
        if trial.fingerprint != fingerprint(c) {
            return Err(Error::Changed);
        }
        let activated = trial.activation_requested;
        let profiles_before = trial.profiles_before.clone();
        let inner = trial.checkpoint.clone();
        let present = nm.trial_checkpoints(false).await?;
        nm.ap_profile_intact(self.ap).await?;
        if present {
            let cp = inner.as_ref().ok_or(Error::Denied)?;
            let results: HashMap<String, u32> = nm
                .proxy(ROOT, NAME)
                .await?
                .call("CheckpointRollback", &(cp,))
                .await
                .map_err(|_| Error::Io)?;
            if results.len() != 1
                || results.get(nm.wifi.as_ref().ok_or(Error::Denied)?.as_str()) != Some(&0)
            {
                return Err(Error::Changed);
            }
            if nm.trial_checkpoints(false).await? {
                let _: () = nm
                    .proxy(ROOT, NAME)
                    .await?
                    .call("CheckpointDestroy", &(cp,))
                    .await
                    .map_err(|_| Error::Io)?;
            }
        }
        // Allow NM's rollback activation to complete; never blindly reactivate on a lost reply.
        tokio::time::timeout(std::time::Duration::from_secs(4), async {
            loop {
                if nm.ready(self.ap).await? {
                    return Ok::<(), Error>(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| Error::Busy)??;
        let mut candidates = Vec::new();
        for p in nm.connections().await? {
            let s = nm.get_settings(&p).await?;
            if !profiles_before.contains(&p)
                && text(s.get("connection").ok_or(Error::Io)?, "uuid")? != c.uuid
            {
                return Err(Error::Changed);
            }
            if text(s.get("connection").ok_or(Error::Io)?, "uuid")? == c.uuid {
                if !activated || !matches(&s, c)? {
                    return Err(Error::Changed);
                }
                candidates.push(p);
            }
        }
        if candidates.len() > 1 {
            return Err(Error::Changed);
        }
        for p in candidates {
            nm.fence().await?;
            let _: () = nm
                .proxy(p.as_str(), SETTING)
                .await?
                .call("Delete", &())
                .await
                .map_err(|_| Error::Io)?;
        }
        let after = nm.connections().await?;
        if after.len() != profiles_before.len()
            || after.iter().any(|p| !profiles_before.contains(p))
        {
            return Err(Error::Changed);
        }
        for p in after {
            if text(
                nm.get_settings(&p)
                    .await?
                    .get("connection")
                    .ok_or(Error::Io)?,
                "uuid",
            )? == c.uuid
            {
                return Err(Error::Changed);
            }
        }
        if nm.trial_checkpoints(false).await? || !nm.ready(self.ap).await? {
            return Err(Error::Changed);
        }
        nm.station = None;
        Ok(())
    }
}
