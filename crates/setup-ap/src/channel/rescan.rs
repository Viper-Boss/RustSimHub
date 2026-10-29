// SPDX-License-Identifier: AGPL-3.0-only
//! Exact, bounded Core/Root rescan metadata; not independent Owner authentication.
use super::*;
use crate::nm::scan::Snapshot;
pub const MAX_PER_PARENT: usize = 8;
pub const COOLDOWN_MS: u64 = 30_000;
pub const MAX_REQUEST_MS: u64 = 40_000;
pub const MAX_REFUSALS_PER_PARENT: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Scan,
    Cancel,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub format: u16,
    pub binding: Binding,
    pub scan_id: String,
    pub owner_key_sha256: String,
    pub owner_generation: u64,
    pub requested_uptime_ms: u64,
    pub deadline_uptime_ms: u64,
    pub action: Action,
}
impl Request {
    pub fn intent(&self) -> Result<Intent> {
        // Cancel is an exact reference to the original Scan, not a new operation body.
        let mut original = self.clone();
        original.action = Action::Scan;
        let bytes = serde_json::to_vec(&original).map_err(|_| Error::Invalid)?;
        let intent = Intent {
            format: self.format,
            binding: self.binding.clone(),
            scan_id: self.scan_id.clone(),
            owner_key_sha256: self.owner_key_sha256.clone(),
            owner_generation: self.owner_generation,
            requested_uptime_ms: self.requested_uptime_ms,
            deadline_uptime_ms: self.deadline_uptime_ms,
            request_sha256: Sha256::digest(bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        };
        if !intent.valid() {
            return Err(Error::Invalid);
        }
        Ok(intent)
    }
    pub fn validate(&self, binding: &Binding, now: u64) -> Result<()> {
        self.intent()?;
        if self.binding != *binding
            || self.requested_uptime_ms > now
            || now >= binding.deadline_uptime_ms
            || self.action == Action::Scan
                && (now >= self.deadline_uptime_ms
                    || now.saturating_sub(self.requested_uptime_ms) > HEARTBEAT_MS
                    || binding.deadline_uptime_ms.saturating_sub(now) < 60_000)
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub fn parse(bytes: &[u8], binding: &Binding, now: u64) -> Result<Self> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::Invalid);
        }
        let request: Self = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
        request.validate(binding, now)?;
        Ok(request)
    }
    pub fn cancels(&self, intent: &Intent) -> bool {
        self.action == Action::Cancel && self.intent().is_ok_and(|i| i == *intent)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    pub format: u16,
    pub binding: Binding,
    pub scan_id: String,
    pub owner_key_sha256: String,
    pub owner_generation: u64,
    pub requested_uptime_ms: u64,
    pub deadline_uptime_ms: u64,
    pub request_sha256: String,
}
impl Intent {
    pub fn valid(&self) -> bool {
        self.format == 1
            && self.binding.valid()
            && uuid(&self.scan_id)
            && hex(&self.owner_key_sha256)
            && self.owner_generation > 0
            && hex(&self.request_sha256)
            && self.requested_uptime_ms >= self.binding.created_uptime_ms
            && self.deadline_uptime_ms > self.requested_uptime_ms
            && self
                .deadline_uptime_ms
                .saturating_sub(self.requested_uptime_ms)
                <= MAX_REQUEST_MS
            && self.deadline_uptime_ms <= self.binding.deadline_uptime_ms.saturating_sub(20_000)
    }
    pub(crate) fn key(&self) -> Result<String> {
        if !self.valid() {
            return Err(Error::Invalid);
        }
        Ok(format!(
            "{}-{}.rescan",
            self.binding.journal_key()?,
            self.scan_id
        ))
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Scanning,
    Ready,
    Failed,
    Unknown,
    Refused,
}
/// A durable decision made before consuming a hardware intent. Never means AP restoration.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    Cooldown,
    Limit,
    Conflict,
    Stale,
    WindowClosing,
    Superseded,
    Cancelled,
    Capacity,
    ParentClosed,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub format: u16,
    pub intent: Intent,
    pub phase: Phase,
    pub sample_boot_id: String,
    pub sample_uptime_ms: u64,
    pub finished_uptime_ms: Option<u64>,
    pub ap_https_verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<Snapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal: Option<Refusal>,
}
impl Status {
    pub fn validate(&self, now: u64, boot: &str) -> Result<()> {
        let same_boot = self.sample_boot_id == self.intent.binding.boot_id;
        if self.format != 1
            || !self.intent.valid()
            || !uuid(&self.sample_boot_id)
            || self.sample_boot_id != boot
            || same_boot && self.sample_uptime_ms < self.intent.requested_uptime_ms
            || self.sample_uptime_ms > now
            || now.saturating_sub(self.sample_uptime_ms) > HEARTBEAT_MS
            || self.ap_https_verified != matches!(self.phase, Phase::Ready | Phase::Failed)
            || (self.phase == Phase::Refused) != self.refusal.is_some()
            || self.phase == Phase::Refused
                && self.refusal != Some(Refusal::ParentClosed)
                && self.finished_uptime_ms.is_none()
            || self.phase == Phase::Scanning && (!same_boot || self.finished_uptime_ms.is_some())
            || self.finished_uptime_ms.is_some_and(|t| {
                t < self.intent.requested_uptime_ms || same_boot && t > self.sample_uptime_ms
            })
            || matches!(self.phase, Phase::Ready | Phase::Failed)
                && self
                    .finished_uptime_ms
                    .is_none_or(|t| t > self.intent.deadline_uptime_ms)
        {
            return Err(Error::Denied);
        }
        match (&self.snapshot, self.phase) {
            (Some(s), Phase::Ready)
                if s.valid()
                    && s.requested_uptime_ms >= self.intent.requested_uptime_ms
                    && s.completed_uptime_ms <= self.finished_uptime_ms.ok_or(Error::Denied)? =>
            {
                Ok(())
            }
            (None, Phase::Scanning | Phase::Failed | Phase::Unknown | Phase::Refused) => Ok(()),
            _ => Err(Error::Denied),
        }
    }
    pub fn parse(bytes: &[u8], intent: &Intent, now: u64, boot: &str) -> Result<Self> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::Invalid);
        }
        let status: Self = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
        status.validate(now, boot)?;
        if status.intent != *intent {
            return Err(Error::Changed);
        }
        Ok(status)
    }
    pub fn refresh(&mut self) -> Result<()> {
        self.sample_uptime_ms = uptime_ms()?;
        self.sample_boot_id = boot_id()?;
        self.validate(self.sample_uptime_ms, &self.sample_boot_id)
    }
}
/// Independent fixed slot: old Core keeps parsing the unchanged parent Availability.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    pub format: u16,
    pub binding: Binding,
    pub sample_uptime_ms: u64,
    pub available: bool,
    pub max_per_parent: usize,
    pub cooldown_ms: u64,
    pub remaining: usize,
    pub refusal: Option<Refusal>,
    pub retry_after_uptime_ms: Option<u64>,
}
impl Capability {
    pub fn parse(bytes: &[u8], binding: &Binding, now: u64, boot: &str) -> Result<Self> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::Invalid);
        }
        let c: Self = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
        if c.format != 1
            || c.binding != *binding
            || !binding.valid()
            || c.binding.boot_id != boot
            || c.sample_uptime_ms < binding.created_uptime_ms
            || c.sample_uptime_ms > now
            || now.saturating_sub(c.sample_uptime_ms) > HEARTBEAT_MS
            || now >= binding.deadline_uptime_ms
            || c.max_per_parent != MAX_PER_PARENT
            || c.cooldown_ms != COOLDOWN_MS
            || c.remaining > MAX_PER_PARENT
            || c.available != c.refusal.is_none()
            || c.available
                && (c.remaining == 0
                    || binding
                        .deadline_uptime_ms
                        .saturating_sub(c.sample_uptime_ms)
                        < 60_000)
            || c.refusal == Some(Refusal::Limit) && c.remaining != 0
            || match (c.refusal, c.retry_after_uptime_ms) {
                (Some(Refusal::Cooldown), Some(t)) => {
                    t <= c.sample_uptime_ms || t >= binding.deadline_uptime_ms
                }
                (Some(Refusal::Cooldown), None) => true,
                (_, Some(_)) => true,
                _ => false,
            }
        {
            return Err(Error::Denied);
        }
        Ok(c)
    }
}
#[cfg(test)]
pub(crate) mod tests;
