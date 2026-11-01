// SPDX-License-Identifier: AGPL-3.0-only
//! Pause/scan/restore the owned RAM AP under a nested automatic rollback checkpoint.
use super::*;
use crate::ap_rescan::{RescanNetwork, RescanProvider};
use std::time::Duration;
use tokio::time::{sleep, timeout};

pub(super) struct Refresh {
    checkpoint: Option<OwnedObjectPath>,
    profiles: Vec<OwnedObjectPath>,
    paused: bool,
    scan_requested: bool,
    rollback_requested: bool,
    destroy_requested: bool,
}
pub struct Session<'a> {
    nm: &'a mut NetworkManager,
    ap: &'a Credentials,
    attempted: bool,
    may_scan: bool,
}
impl RescanProvider for NetworkManager {
    fn rescan_session<'a>(
        &'a mut self,
        ap: &'a Credentials,
    ) -> Result<impl RescanNetwork + Send + 'a> {
        if !self.retain_ap || self.station.is_some() {
            return Err(Error::Denied);
        }
        let attempted = self.refresh.is_some();
        // An uncertain session can be reopened only for exact cleanup/read-only reconciliation.
        // It cannot pause again or consume a fresh scan under the earlier request.
        Ok(Session {
            nm: self,
            ap,
            attempted,
            may_scan: false,
        })
    }
}
impl NetworkManager {
    async fn refresh_allowed_profiles(&self, profiles: &[OwnedObjectPath]) -> Result<()> {
        for profile in profiles {
            let settings = self.get_settings(profile).await?;
            let connection = settings.get("connection").ok_or(Error::Invalid)?;
            let kind = text(connection, "type")?;
            if kind == "802-11-wireless"
                && Some(profile) != self.profile.as_ref()
                && Some(profile) != self.saved.as_ref()
                || kind == "gsm" && boolean(connection, "autoconnect")?
            {
                return Err(Error::Changed);
            }
        }
        Ok(())
    }
    /// Only the original parent and this refresh's exact inner checkpoint may exist.
    async fn refresh_checkpoints(&self) -> Result<bool> {
        self.fence().await?;
        let refresh = self.refresh.as_ref().ok_or(Error::Denied)?;
        let parent = self.checkpoint.as_ref().ok_or(Error::Denied)?;
        let root = self.properties(ROOT, NAME).await?;
        let paths: Vec<OwnedObjectPath> = root
            .get("Checkpoints")
            .ok_or(Error::Invalid)?
            .try_clone()
            .map_err(|_| Error::Invalid)?
            .try_into()
            .map_err(|_| Error::Invalid)?;
        if !paths_valid(&paths, "/org/freedesktop/NetworkManager/Checkpoint/", 2)
            || !paths.contains(parent)
            || paths
                .iter()
                .any(|p| p != parent && Some(p) != refresh.checkpoint.as_ref())
        {
            return Err(Error::Changed);
        }
        Ok(refresh
            .checkpoint
            .as_ref()
            .is_some_and(|cp| paths.contains(cp)))
    }
    async fn refresh_profiles(&self, ap: &Credentials) -> Result<()> {
        self.ap_profile_intact(ap).await?;
        let original = &self.refresh.as_ref().ok_or(Error::Denied)?.profiles;
        let current = self.connections().await?;
        if current.len() != original.len() || current.iter().any(|p| !original.contains(p)) {
            return Err(Error::Changed);
        }
        self.refresh_allowed_profiles(&current).await?;
        self.fence().await
    }
    pub(super) async fn refresh_scan_context(&self) -> Result<()> {
        if !self.refresh.as_ref().is_some_and(|r| r.paused) || !self.refresh_checkpoints().await? {
            return Err(Error::Changed);
        }
        self.idle_or_saved().await?;
        let root = self.properties(ROOT, NAME).await?;
        if !boolean(&root, "NetworkingEnabled")? || !boolean(&root, "WirelessEnabled")? {
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
}
impl RescanNetwork for Session<'_> {
    async fn pause_ap(&mut self) -> Result<()> {
        if self.attempted {
            return Err(Error::Denied);
        }
        self.attempted = true;
        let nm = &mut self.nm;
        if nm.refresh.is_some() || nm.station.is_some() || !nm.ready(self.ap).await? {
            return Err(Error::Denied);
        }
        // At this point no inner checkpoint is owned. Refuse, never guess, a foreign one.
        let root = nm.properties(ROOT, NAME).await?;
        let checkpoints: Vec<OwnedObjectPath> = root
            .get("Checkpoints")
            .ok_or(Error::Invalid)?
            .try_clone()
            .map_err(|_| Error::Invalid)?
            .try_into()
            .map_err(|_| Error::Invalid)?;
        if checkpoints.len() != 1 || checkpoints.first() != nm.checkpoint.as_ref() {
            return Err(Error::Changed);
        }
        let profiles = nm.connections().await?;
        nm.refresh_allowed_profiles(&profiles).await?;
        // Retain uncertainty before CheckpointCreate is called, including a lost reply.
        nm.refresh = Some(Refresh {
            checkpoint: None,
            profiles,
            paused: false,
            scan_requested: false,
            rollback_requested: false,
            destroy_requested: false,
        });
        nm.refresh_profiles(self.ap).await?;
        let checkpoint: OwnedObjectPath = nm
            .proxy(ROOT, NAME)
            .await?
            .call(
                "CheckpointCreate",
                &(vec![nm.wifi.as_ref().ok_or(Error::Denied)?], 45u32, 8u32),
            )
            .await
            .map_err(|_| Error::Io)?;
        if !checkpoint
            .as_str()
            .starts_with("/org/freedesktop/NetworkManager/Checkpoint/")
            || Some(&checkpoint) == nm.checkpoint.as_ref()
        {
            return Err(Error::Changed);
        }
        nm.refresh.as_mut().ok_or(Error::Denied)?.checkpoint = Some(checkpoint);
        if !nm.refresh_checkpoints().await? || !nm.ready(self.ap).await? {
            return Err(Error::Changed);
        }
        let device = nm
            .properties(nm.wifi.as_ref().ok_or(Error::Denied)?.as_str(), DEVICE)
            .await?;
        let active = path(&device, "ActiveConnection")?;
        if !active
            .as_str()
            .starts_with("/org/freedesktop/NetworkManager/ActiveConnection/")
        {
            return Err(Error::Changed);
        }
        let connection = nm
            .properties(
                active.as_str(),
                "org.freedesktop.NetworkManager.Connection.Active",
            )
            .await?;
        if Some(&path(&connection, "Connection")?) != nm.profile.as_ref() {
            return Err(Error::Changed);
        }
        nm.fence().await?;
        let _: () = nm
            .proxy(ROOT, NAME)
            .await?
            .call("DeactivateConnection", &(active,))
            .await
            .map_err(|_| Error::Io)?;
        timeout(Duration::from_secs(4), async {
            loop {
                nm.fence().await?;
                if nm.idle_or_saved().await.is_ok() {
                    let wireless = nm
                        .properties(nm.wifi.as_ref().ok_or(Error::Denied)?.as_str(), WIRELESS)
                        .await?;
                    if matches!(number(&wireless, "Mode")?, 0 | 2) {
                        return Ok::<(), Error>(());
                    }
                }
                sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| Error::Busy)??;
        nm.refresh.as_mut().ok_or(Error::Denied)?.paused = true;
        nm.refresh_scan_context().await?;
        self.may_scan = true;
        Ok(())
    }
    async fn scan(&mut self) -> Result<scan::Snapshot> {
        if !self.may_scan {
            return Err(Error::Denied);
        }
        self.may_scan = false;
        let nm = &mut self.nm;
        let refresh = nm.refresh.as_mut().ok_or(Error::Denied)?;
        if !refresh.paused || refresh.scan_requested {
            return Err(Error::Denied);
        }
        refresh.scan_requested = true;
        nm.refresh_profiles(self.ap).await?;
        timeout(
            Duration::from_secs(scan::MAX_SCAN_SECONDS),
            nm.collect_scan(scan::Context::PausedAp),
        )
        .await
        .map_err(|_| Error::Busy)?
    }
    async fn restore_ap(&mut self) -> Result<()> {
        let nm = &mut self.nm;
        nm.fence().await?;
        let Some(refresh) = nm.refresh.as_ref() else {
            return if nm.ready(self.ap).await? {
                Ok(())
            } else {
                Err(Error::Changed)
            };
        };
        if refresh.rollback_requested || refresh.destroy_requested {
            // A lost acknowledgement may be resolved once the exact checkpoint is gone
            // and the original AP/profiles are visibly intact. Never issue either write again.
            if nm.refresh_checkpoints().await? || !nm.ready(self.ap).await? {
                return Err(Error::Changed);
            }
            nm.refresh_profiles(self.ap).await?;
            nm.refresh = None;
            return Ok(());
        }
        let checkpoint = refresh.checkpoint.clone();
        nm.refresh_profiles(self.ap).await?;
        if nm.refresh_checkpoints().await? {
            let checkpoint = checkpoint.ok_or(Error::Changed)?;
            nm.refresh.as_mut().ok_or(Error::Denied)?.rollback_requested = true;
            let result: HashMap<String, u32> = nm
                .proxy(ROOT, NAME)
                .await?
                .call("CheckpointRollback", &(&checkpoint,))
                .await
                .map_err(|_| Error::Io)?;
            if result.len() != 1
                || result.get(nm.wifi.as_ref().ok_or(Error::Denied)?.as_str()) != Some(&0)
            {
                return Err(Error::Changed);
            }
            if nm.refresh_checkpoints().await? {
                nm.refresh.as_mut().ok_or(Error::Denied)?.destroy_requested = true;
                let _: () = nm
                    .proxy(ROOT, NAME)
                    .await?
                    .call("CheckpointDestroy", &(&checkpoint,))
                    .await
                    .map_err(|_| Error::Io)?;
            }
        }
        // Rollback (or its own finite automatic timer) is the sole restoration mechanism.
        // A failed reply never authorizes ActivateConnection or a second rollback.
        timeout(Duration::from_secs(4), async {
            loop {
                if nm.ready(self.ap).await? {
                    return Ok::<(), Error>(());
                }
                sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| Error::Busy)??;
        nm.refresh_profiles(self.ap).await?;
        if nm.refresh_checkpoints().await? {
            return Err(Error::Changed);
        }
        nm.refresh = None;
        Ok(())
    }
}
