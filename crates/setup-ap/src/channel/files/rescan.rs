// SPDX-License-Identifier: AGPL-3.0-only
//! Durable at-most-once rescan intents. Queries never create hardware work or clear Unknown.
use super::*;
use crate::channel::rescan::{Intent, Phase, Status, COOLDOWN_MS, MAX_PER_PARENT};
impl Journal {
    pub(crate) fn ensure_rescans_closed(&self, binding: &Binding) -> Result<()> {
        let prefix = format!("{}-", binding.journal_key()?);
        let entries: Vec<_> = fs::read_dir(&self.dir.path)
            .map_err(|_| Error::Io)?
            .take(513)
            .collect::<std::io::Result<_>>()
            .map_err(|_| Error::Io)?;
        if entries.len() > 512 {
            return Err(Error::Busy);
        }
        for entry in entries {
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| Error::Invalid)?;
            if name.starts_with(&prefix) && name.ends_with(".rescan.intent") {
                let bytes = self.dir.read_name(&name, 0o600)?.ok_or(Error::Changed)?;
                let intent: Intent = serde_json::from_slice(&bytes).map_err(|_| Error::Invalid)?;
                if !intent.valid()
                    || intent.binding != *binding
                    || name != format!("{}.intent", intent.key()?)
                {
                    return Err(Error::Changed);
                }
                if !self
                    .rescan_result(&intent)?
                    .is_some_and(|s| matches!(s.phase, Phase::Ready | Phase::Failed))
                {
                    return Err(Error::Denied);
                }
            }
        }
        if let Some(active) = self.rescan_pending()? {
            if active.binding != *binding
                || !self
                    .rescan_result(&active)?
                    .is_some_and(|s| matches!(s.phase, Phase::Ready | Phase::Failed))
            {
                return Err(Error::Denied);
            }
            fs::remove_file(self.dir.path.join("rescan-active.json")).map_err(|_| Error::Io)?;
            File::open(&self.dir.path)
                .and_then(|d| d.sync_all())
                .map_err(|_| Error::Io)?;
        }
        Ok(())
    }
    pub(crate) fn rescan_consumed(&self, intent: &Intent) -> Result<bool> {
        let Some(bytes) = self
            .dir
            .read_name(&format!("{}.intent", intent.key()?), 0o600)?
        else {
            return Ok(false);
        };
        let old: Intent = serde_json::from_slice(&bytes).map_err(|_| Error::Invalid)?;
        if old != *intent || !old.valid() {
            return Err(Error::Changed);
        }
        Ok(true)
    }
    pub(crate) fn rescan_pending(&self) -> Result<Option<Intent>> {
        let Some(bytes) = self.dir.read_name("rescan-active.json", 0o600)? else {
            return Ok(None);
        };
        let intent: Intent = serde_json::from_slice(&bytes).map_err(|_| Error::Invalid)?;
        if !intent.valid()
            || self.pending()?.as_ref() != Some(&intent.binding)
            || !self.rescan_consumed(&intent)?
        {
            return Err(Error::Changed);
        }
        Ok(Some(intent))
    }
    pub(crate) fn rescan_result(&self, intent: &Intent) -> Result<Option<Status>> {
        if !self.rescan_consumed(intent)? {
            return Ok(None);
        }
        let Some(bytes) = self
            .dir
            .read_name(&format!("{}.result", intent.key()?), 0o600)?
        else {
            return Ok(None);
        };
        let status: Status = serde_json::from_slice(&bytes).map_err(|_| Error::Invalid)?;
        status.validate(status.sample_uptime_ms, &status.sample_boot_id)?;
        if status.intent != *intent || status.phase == Phase::Scanning {
            return Err(Error::Changed);
        }
        Ok(Some(status))
    }
    pub(crate) fn consume_rescan(&self, intent: &Intent) -> Result<()> {
        if !intent.valid()
            || self.pending()?.as_ref() != Some(&intent.binding)
            || self.rescan_consumed(intent)?
            || self.rescan_pending()?.is_some()
        {
            return Err(Error::Denied);
        }
        let entries: Vec<_> = fs::read_dir(&self.dir.path)
            .map_err(|_| Error::Io)?
            .take(513)
            .collect::<std::io::Result<_>>()
            .map_err(|_| Error::Io)?;
        if entries.len() >= 512 {
            return Err(Error::Busy);
        }
        let prefix = format!("{}-", intent.binding.journal_key()?);
        let mut count = 0;
        let mut latest = 0;
        for entry in entries {
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| Error::Invalid)?;
            if name.starts_with(&prefix) && name.ends_with(".rescan.intent") {
                let bytes = self.dir.read_name(&name, 0o600)?.ok_or(Error::Changed)?;
                let old: Intent = serde_json::from_slice(&bytes).map_err(|_| Error::Invalid)?;
                if !old.valid()
                    || old.binding != intent.binding
                    || name != format!("{}.intent", old.key()?)
                {
                    return Err(Error::Changed);
                }
                match self.rescan_result(&old)? {
                    Some(s) if matches!(s.phase, Phase::Ready | Phase::Failed) => (),
                    _ => return Err(Error::Busy),
                }
                count += 1;
                latest = latest.max(old.requested_uptime_ms);
            }
        }
        if count >= MAX_PER_PARENT
            || count > 0 && intent.requested_uptime_ms < latest.saturating_add(COOLDOWN_MS)
        {
            return Err(Error::Busy);
        }
        let bytes = serde_json::to_vec(intent).map_err(|_| Error::Invalid)?;
        self.dir
            .write_name(&format!("{}.intent", intent.key()?), &bytes, 0o600, true)?;
        // If this second write fails, the immutable consumed intent still prevents replay.
        // A later query can report Unknown, never infer permission to try Scan again.
        self.dir
            .write_name("rescan-active.json", &bytes, 0o600, true)
    }
    pub(crate) fn finish_rescan(&self, status: &Status) -> Result<()> {
        status.validate(super::super::uptime_ms()?, &super::super::boot_id()?)?;
        if status.phase == Phase::Scanning
            || !self.rescan_consumed(&status.intent)?
            || self.rescan_result(&status.intent)?.is_some()
        {
            return Err(Error::Denied);
        }
        let active = self.rescan_pending()?;
        if active.as_ref().is_some_and(|i| *i != status.intent)
            || status.phase != Phase::Unknown && active.as_ref() != Some(&status.intent)
        {
            return Err(Error::Changed);
        }
        self.dir.write_name(
            &format!("{}.result", status.intent.key()?),
            &serde_json::to_vec(status).map_err(|_| Error::Invalid)?,
            0o600,
            true,
        )?;
        if matches!(status.phase, Phase::Ready | Phase::Failed) {
            fs::remove_file(self.dir.path.join("rescan-active.json")).map_err(|_| Error::Io)?;
            File::open(&self.dir.path)
                .and_then(|d| d.sync_all())
                .map_err(|_| Error::Io)?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests;
