// SPDX-License-Identifier: AGPL-3.0-only
//! Protected explicit confirmation, separate from a successful station test.
use super::*;
use crate::station_trial::StationCredentials;
use zeroize::Zeroizing;

// No Debug/Clone: the original test payload contains its private credentials.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub format: u16,
    pub trial: station::Request,
    pub commit_id: String,
    pub requested_uptime_ms: u64,
    pub deadline_uptime_ms: u64,
}
impl Request {
    pub fn parse(bytes: &[u8], parent: &Binding, now: u64) -> Result<Self> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::Invalid);
        }
        let v: Self = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
        v.validate(parent, now)?;
        Ok(v)
    }
    pub fn validate(&self, parent: &Binding, now: u64) -> Result<()> {
        let intent = self.intent()?;
        if self.trial.binding != *parent
            || !intent.valid()
            || !matches!(self.trial.operation, station::Operation::Test { .. })
            || self.requested_uptime_ms > now
            || now.saturating_sub(self.requested_uptime_ms) > HEARTBEAT_MS
            || self.deadline_uptime_ms <= now
            || parent.deadline_uptime_ms.saturating_sub(now) < 80_000
        {
            return Err(Error::Denied);
        }
        self.credentials()?;
        Ok(())
    }
    pub fn intent(&self) -> Result<Intent> {
        let bytes = Zeroizing::new(serde_json::to_vec(self).map_err(|_| Error::Invalid)?);
        Ok(Intent {
            format: 1,
            trial: self.trial.intent()?,
            commit_id: self.commit_id.clone(),
            requested_uptime_ms: self.requested_uptime_ms,
            deadline_uptime_ms: self.deadline_uptime_ms,
            request_sha256: Sha256::digest(bytes.as_slice())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        })
    }
    pub(crate) fn credentials(&self) -> Result<StationCredentials> {
        let mut c = self.trial.credentials()?;
        // A crash-recovery candidate is determined before any hardware work,
        // not a fresh random UUID that would be lost with the Root process.
        if !uuid(&self.commit_id) {
            return Err(Error::Invalid);
        }
        c.uuid = self.commit_id.clone();
        Ok(c)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    pub format: u16,
    pub trial: station::Intent,
    pub commit_id: String,
    pub requested_uptime_ms: u64,
    pub deadline_uptime_ms: u64,
    pub request_sha256: String,
}
impl Intent {
    pub fn valid(&self) -> bool {
        self.format == 1
            && self.trial.valid()
            && uuid(&self.commit_id)
            && self.commit_id != self.trial.trial_id
            && self.commit_id != self.trial.binding.request_id
            && hex(&self.request_sha256)
            && self.requested_uptime_ms >= self.trial.requested_uptime_ms
            && self
                .trial
                .binding
                .deadline_uptime_ms
                .saturating_sub(self.requested_uptime_ms)
                >= 80_000
            && self.deadline_uptime_ms > self.requested_uptime_ms
            && self.deadline_uptime_ms - self.requested_uptime_ms <= 40_000
            && self.deadline_uptime_ms
                <= self.trial.binding.deadline_uptime_ms.saturating_sub(30_000)
    }
    pub(crate) fn key(&self) -> Result<String> {
        if !self.valid() {
            return Err(Error::Invalid);
        }
        Ok(format!(
            "{}-{}.commit",
            self.trial.binding.journal_key()?,
            self.commit_id
        ))
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Saving,
    Saved,
    RestoredAp,
    Unknown,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub format: u16,
    pub intent: Intent,
    pub phase: Phase,
    pub execution_uptime_ms: u64,
    pub observation_boot_id: String,
    pub sample_uptime_ms: u64,
    pub settings_saved: bool,
    pub ap_removed: bool,
    pub checkpoints_cleared: bool,
    pub guard_released: bool,
    pub ap_https_verified: bool,
}
impl Status {
    pub fn validate(&self, now: u64, boot: &str) -> Result<()> {
        let saved = self.phase == Phase::Saved;
        if self.format != 1
            || !self.intent.valid()
            || !uuid(&self.observation_boot_id)
            || self.observation_boot_id != boot
            || self.sample_uptime_ms > now
            || now.saturating_sub(self.sample_uptime_ms) > HEARTBEAT_MS
            || self.execution_uptime_ms < self.intent.requested_uptime_ms
            || self.observation_boot_id == self.intent.trial.binding.boot_id
                && self.execution_uptime_ms > self.sample_uptime_ms
            || self.settings_saved != saved
            || self.ap_removed != saved
            || self.checkpoints_cleared != saved
            || self.guard_released != saved
            || self.ap_https_verified != (self.phase == Phase::RestoredAp)
            || self.phase == Phase::Saving
                && (boot != self.intent.trial.binding.boot_id
                    || now >= self.intent.deadline_uptime_ms)
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub(crate) fn refresh(&mut self) -> Result<()> {
        if self.phase == Phase::Saving {
            return Err(Error::Denied);
        }
        self.observation_boot_id = boot_id()?;
        self.sample_uptime_ms = uptime_ms()?;
        self.validate(self.sample_uptime_ms, &self.observation_boot_id)
    }
}
#[cfg(test)]
pub(crate) mod tests;
