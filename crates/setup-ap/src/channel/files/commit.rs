// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::channel::commit::{Intent, Phase, Request, Status};
impl Journal {
    fn commit_ready(&self, intent: &Intent) -> Result<()> {
        if !intent.valid() || self.pending()?.as_ref() != Some(&intent.trial.binding) {
            return Err(Error::Denied);
        }
        let done = self.trial_result(&intent.trial)?.ok_or(Error::Denied)?;
        if done.phase != crate::channel::station::Phase::Ready || !done.ap_https_verified {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub(crate) fn admit_commit(&self, r: &Request, parent: &Binding, now: u64) -> Result<Intent> {
        r.validate(parent, now)?;
        let intent = r.intent()?;
        self.commit_ready(&intent)?;
        Ok(intent)
    }
    pub(crate) fn commit_pending(&self, binding: &Binding) -> Result<Option<Intent>> {
        let Some(bytes) = self
            .dir
            .read_name(&format!("{}.commit-active", binding.journal_key()?), 0o600)?
        else {
            return Ok(None);
        };
        let i: Intent = serde_json::from_slice(&bytes).map_err(|_| Error::Invalid)?;
        if !i.valid() || i.trial.binding != *binding || !self.commit_consumed(&i)? {
            return Err(Error::Changed);
        }
        Ok(Some(i))
    }
    pub(crate) fn commit_consumed(&self, intent: &Intent) -> Result<bool> {
        let Some(bytes) = self
            .dir
            .read_name(&format!("{}.intent", intent.key()?), 0o600)?
        else {
            return Ok(false);
        };
        let old: Intent = serde_json::from_slice(&bytes).map_err(|_| Error::Invalid)?;
        if old != *intent {
            return Err(Error::Changed);
        }
        Ok(true)
    }
    pub(crate) fn consume_commit(&self, intent: &Intent) -> Result<()> {
        self.commit_ready(intent)?;
        if self.commit_pending(&intent.trial.binding)?.is_some() || self.commit_consumed(intent)? {
            return Err(Error::Denied);
        }
        if fs::read_dir(&self.dir.path)
            .map_err(|_| Error::Io)?
            .take(513)
            .count()
            >= 508
        {
            return Err(Error::Busy);
        }
        let b = serde_json::to_vec(intent).map_err(|_| Error::Invalid)?;
        self.dir
            .write_name(&format!("{}.intent", intent.key()?), &b, 0o600, true)?;
        self.dir.write_name(
            &format!("{}.commit-active", intent.trial.binding.journal_key()?),
            &b,
            0o600,
            true,
        )
    }
    pub(crate) fn commit_result(&self, intent: &Intent) -> Result<Option<Status>> {
        if !self.commit_consumed(intent)? {
            return Ok(None);
        }
        let Some(b) = self
            .dir
            .read_name(&format!("{}.result", intent.key()?), 0o600)?
        else {
            return Ok(None);
        };
        let s: Status = serde_json::from_slice(&b).map_err(|_| Error::Invalid)?;
        s.validate(s.sample_uptime_ms, &s.observation_boot_id)?;
        if s.intent != *intent || s.phase == Phase::Saving {
            return Err(Error::Changed);
        }
        Ok(Some(s))
    }
    fn remove_commit_active(&self, intent: &Intent) -> Result<()> {
        let Some(old) = self.commit_pending(&intent.trial.binding)? else {
            return Ok(());
        };
        if old != *intent {
            return Err(Error::Changed);
        }
        fs::remove_file(self.dir.path.join(format!(
            "{}.commit-active",
            intent.trial.binding.journal_key()?
        )))
        .map_err(|_| Error::Io)?;
        File::open(&self.dir.path)
            .and_then(|d| d.sync_all())
            .map_err(|_| Error::Io)
    }
    pub(crate) fn committed(&self, binding: &Binding) -> Result<Option<Status>> {
        let Some(b) = self
            .dir
            .read_name(&format!("{}.committed", binding.journal_key()?), 0o600)?
        else {
            return Ok(None);
        };
        let s: Status = serde_json::from_slice(&b).map_err(|_| Error::Invalid)?;
        s.validate(s.sample_uptime_ms, &s.observation_boot_id)?;
        if s.intent.trial.binding != *binding || s.phase != Phase::Saved {
            return Err(Error::Changed);
        }
        let result = self.commit_result(&s.intent)?.ok_or(Error::Changed)?;
        if serde_json::to_vec(&result).map_err(|_| Error::Invalid)? != b {
            return Err(Error::Changed);
        }
        Ok(Some(s))
    }
    /// Durable proof before clearing either pending marker. It never invents a
    /// legacy rollback/Closed receipt for a committed, still-active STA.
    pub(crate) fn finish_commit(&self, s: &Status) -> Result<()> {
        s.validate(s.sample_uptime_ms, &s.observation_boot_id)?;
        if s.phase == Phase::Saving || !self.commit_consumed(&s.intent)? {
            return Err(Error::Denied);
        }
        let bytes = serde_json::to_vec(s).map_err(|_| Error::Invalid)?;
        if let Some(old) = self.commit_result(&s.intent)? {
            if old.phase != s.phase || old.execution_uptime_ms != s.execution_uptime_ms {
                return Err(Error::Changed);
            }
        } else {
            self.dir
                .write_name(&format!("{}.result", s.intent.key()?), &bytes, 0o600, true)?;
        }
        if s.phase == Phase::Saved {
            let stored = self.commit_result(&s.intent)?.ok_or(Error::Changed)?;
            if self.committed(&s.intent.trial.binding)?.is_none() {
                self.dir.write_name(
                    &format!("{}.committed", s.intent.trial.binding.journal_key()?),
                    &serde_json::to_vec(&stored).map_err(|_| Error::Invalid)?,
                    0o600,
                    true,
                )?;
            }
            self.remove_commit_active(&s.intent)?;
            if let Some(parent) = self.pending()? {
                if parent != s.intent.trial.binding {
                    return Err(Error::Changed);
                }
                fs::remove_file(self.dir.path.join("active.json")).map_err(|_| Error::Io)?;
                File::open(&self.dir.path)
                    .and_then(|d| d.sync_all())
                    .map_err(|_| Error::Io)?;
            }
        } else if s.phase == Phase::RestoredAp {
            self.remove_commit_active(&s.intent)?;
        }
        Ok(())
    }
}
