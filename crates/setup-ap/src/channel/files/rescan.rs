// SPDX-License-Identifier: AGPL-3.0-only
//! Durable at-most-once rescan intents. Queries never create hardware work or clear Unknown.
use super::*;
use crate::channel::rescan::{
    Intent, Phase, Refusal, Status, COOLDOWN_MS, MAX_PER_PARENT, MAX_REFUSALS_PER_PARENT,
};
pub(crate) struct Admission {
    pub remaining: usize,
    pub refusal: Option<Refusal>,
    pub retry_after_uptime_ms: Option<u64>,
}
impl Journal {
    fn rescan_entries(&self) -> Result<Vec<fs::DirEntry>> {
        let entries: Vec<_> = fs::read_dir(&self.dir.path)
            .map_err(|_| Error::Io)?
            .take(513)
            .collect::<std::io::Result<_>>()
            .map_err(|_| Error::Io)?;
        if entries.len() > 512 {
            return Err(Error::Busy);
        }
        Ok(entries)
    }
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
    fn rescan_accepted(&self, intent: &Intent) -> Result<bool> {
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
    fn rescan_refusal(&self, intent: &Intent) -> Result<Option<Status>> {
        let Some(raw) = self
            .dir
            .read_name(&format!("{}.refusal", intent.key()?), 0o600)?
        else {
            return Ok(None);
        };
        let s: Status = serde_json::from_slice(&raw).map_err(|_| Error::Invalid)?;
        s.validate(s.sample_uptime_ms, &s.sample_boot_id)?;
        if s.intent != *intent || s.phase != Phase::Refused {
            return Err(Error::Changed);
        }
        Ok(Some(s))
    }
    pub(crate) fn rescan_consumed(&self, intent: &Intent) -> Result<bool> {
        let accepted = self.rescan_accepted(intent)?;
        let refused = self.rescan_refusal(intent)?.is_some();
        if accepted && refused {
            return Err(Error::Changed);
        }
        Ok(accepted || refused)
    }
    pub(crate) fn rescan_pending(&self) -> Result<Option<Intent>> {
        let Some(bytes) = self.dir.read_name("rescan-active.json", 0o600)? else {
            return Ok(None);
        };
        let intent: Intent = serde_json::from_slice(&bytes).map_err(|_| Error::Invalid)?;
        if !intent.valid()
            || self.pending()?.as_ref() != Some(&intent.binding)
            || !self.rescan_accepted(&intent)?
        {
            return Err(Error::Changed);
        }
        Ok(Some(intent))
    }
    pub(crate) fn rescan_result(&self, intent: &Intent) -> Result<Option<Status>> {
        let accepted = self.rescan_accepted(intent)?;
        if let Some(refused) = self.rescan_refusal(intent)? {
            if accepted {
                return Err(Error::Changed);
            }
            return Ok(Some(refused));
        }
        if !accepted {
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
        if status.intent != *intent || matches!(status.phase, Phase::Scanning | Phase::Refused) {
            return Err(Error::Changed);
        }
        Ok(Some(status))
    }
    pub(crate) fn rescan_admission(&self, binding: &Binding, at: u64) -> Result<Admission> {
        if !binding.valid() || self.pending()?.as_ref() != Some(binding) {
            return Err(Error::Denied);
        }
        let entries = self.rescan_entries()?;
        let capacity = entries.len() > 509;
        let prefix = format!("{}-", binding.journal_key()?);
        let mut count = 0;
        let mut refusals = 0;
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
                    || old.binding != *binding
                    || name != format!("{}.intent", old.key()?)
                {
                    return Err(Error::Changed);
                }
                match self.rescan_result(&old)? {
                    Some(s) if matches!(s.phase, Phase::Ready | Phase::Failed) => (),
                    _ => return Err(Error::Denied),
                }
                count += 1;
                latest = latest.max(old.requested_uptime_ms);
            } else if name.starts_with(&prefix) && name.ends_with(".rescan.refusal") {
                let raw = self.dir.read_name(&name, 0o600)?.ok_or(Error::Changed)?;
                let old: Status = serde_json::from_slice(&raw).map_err(|_| Error::Invalid)?;
                old.validate(old.sample_uptime_ms, &old.sample_boot_id)?;
                if old.phase != Phase::Refused
                    || old.intent.binding != *binding
                    || name != format!("{}.refusal", old.intent.key()?)
                {
                    return Err(Error::Changed);
                }
                refusals += 1;
            }
        }
        let remaining = MAX_PER_PARENT.saturating_sub(count);
        let next = latest.saturating_add(COOLDOWN_MS);
        let refusal = if remaining == 0 {
            Some(Refusal::Limit)
        } else if self.rescan_pending()?.is_some() {
            Some(Refusal::Conflict)
        } else if capacity || refusals >= MAX_REFUSALS_PER_PARENT {
            Some(Refusal::Capacity)
        } else if count > 0 && at < next {
            Some(Refusal::Cooldown)
        } else {
            None
        };
        Ok(Admission {
            remaining,
            refusal,
            retry_after_uptime_ms: if refusal == Some(Refusal::Cooldown) {
                Some(next)
            } else {
                None
            },
        })
    }
    pub(crate) fn refuse_rescan(&self, status: &Status) -> Result<()> {
        status.validate(super::super::uptime_ms()?, &super::super::boot_id()?)?;
        let closed = status.refusal == Some(Refusal::ParentClosed)
            && self.closed(&status.intent.binding)?.is_some();
        if status.phase != Phase::Refused
            || !closed && self.pending()?.as_ref() != Some(&status.intent.binding)
            || status.refusal == Some(Refusal::ParentClosed) && !closed
            || self.rescan_consumed(&status.intent)?
        {
            return Err(Error::Denied);
        }
        let entries = self.rescan_entries()?;
        // Reserve the parent close proof too. A storage failure cannot create permission to Scan.
        if entries.len() >= 511 {
            return Err(Error::Busy);
        }
        let prefix = format!("{}-", status.intent.binding.journal_key()?);
        let mut count = 0;
        for entry in entries {
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| Error::Invalid)?;
            if name.starts_with(&prefix) && name.ends_with(".rescan.refusal") {
                let raw = self.dir.read_name(&name, 0o600)?.ok_or(Error::Changed)?;
                let old: Status = serde_json::from_slice(&raw).map_err(|_| Error::Invalid)?;
                old.validate(old.sample_uptime_ms, &old.sample_boot_id)?;
                if old.phase != Phase::Refused
                    || old.intent.binding != status.intent.binding
                    || name != format!("{}.refusal", old.intent.key()?)
                {
                    return Err(Error::Changed);
                }
                count += 1;
            }
        }
        if count >= MAX_REFUSALS_PER_PARENT {
            return Err(Error::Busy);
        }
        self.dir.write_name(
            &format!("{}.refusal", status.intent.key()?),
            &serde_json::to_vec(status).map_err(|_| Error::Invalid)?,
            0o600,
            true,
        )
    }
    pub(crate) fn consume_rescan(&self, intent: &Intent) -> Result<()> {
        if !intent.valid() || self.rescan_consumed(intent)? {
            return Err(Error::Denied);
        }
        if self
            .rescan_admission(&intent.binding, intent.requested_uptime_ms)?
            .refusal
            .is_some()
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
        if matches!(status.phase, Phase::Scanning | Phase::Refused)
            || !self.rescan_accepted(&status.intent)?
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
