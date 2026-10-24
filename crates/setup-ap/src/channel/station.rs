// SPDX-License-Identifier: AGPL-3.0-only
//! Protected local Core/Root station requests. Metadata is not independent Owner authentication.
use super::*;
use crate::station_trial::StationCredentials;
use zeroize::Zeroizing;

pub struct Secret(Zeroizing<String>);
impl Secret {
    pub fn new(value: Zeroizing<String>) -> Self {
        Self(value)
    }
}
impl Serialize for Secret {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        self.0.as_str().serialize(s)
    }
}
impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        String::deserialize(d).map(|s| Self(Zeroizing::new(s)))
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Test { ssid: String, passphrase: Secret },
    Cancel,
}
// Deliberately no Debug or Clone, and no settings/path/WWAN/command field.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub format: u16,
    pub binding: Binding,
    pub trial_id: String,
    pub owner_key_sha256: String,
    pub owner_generation: u64,
    pub requested_uptime_ms: u64,
    pub deadline_uptime_ms: u64,
    pub operation: Operation,
}
impl Request {
    pub fn parse(bytes: &[u8], binding: &Binding, now: u64) -> Result<Self> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::Invalid);
        }
        let r: Self = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
        r.validate(binding, now)?;
        Ok(r)
    }
    pub fn validate(&self, binding: &Binding, now: u64) -> Result<()> {
        let intent = self.intent()?;
        if self.binding != *binding
            || !intent.valid()
            || self.requested_uptime_ms > now
            || self.deadline_uptime_ms <= now
        {
            return Err(Error::Denied);
        }
        if matches!(self.operation, Operation::Test { .. }) {
            if now.saturating_sub(self.requested_uptime_ms) > HEARTBEAT_MS
                || binding.deadline_uptime_ms.saturating_sub(now) < 60_000
            {
                return Err(Error::Denied);
            }
            self.credentials()?;
        }
        Ok(())
    }
    pub(crate) fn credentials(&self) -> Result<StationCredentials> {
        let Operation::Test { ssid, passphrase } = &self.operation else {
            return Err(Error::Denied);
        };
        StationCredentials::new(ssid.clone(), Zeroizing::new(passphrase.0.to_string()))
    }
    pub fn intent(&self) -> Result<Intent> {
        let encoded = Zeroizing::new(serde_json::to_vec(self).map_err(|_| Error::Invalid)?);
        let digest = Sha256::digest(encoded.as_slice())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(Intent {
            format: 1,
            binding: self.binding.clone(),
            trial_id: self.trial_id.clone(),
            owner_key_sha256: self.owner_key_sha256.clone(),
            owner_generation: self.owner_generation,
            requested_uptime_ms: self.requested_uptime_ms,
            deadline_uptime_ms: self.deadline_uptime_ms,
            request_sha256: digest,
        })
    }
    pub fn cancels(&self, intent: &Intent) -> bool {
        matches!(self.operation, Operation::Cancel)
            && self.binding == intent.binding
            && self.trial_id == intent.trial_id
            && self.owner_key_sha256 == intent.owner_key_sha256
            && self.owner_generation == intent.owner_generation
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    pub format: u16,
    pub binding: Binding,
    pub trial_id: String,
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
            && uuid(&self.trial_id)
            && hex(&self.owner_key_sha256)
            && self.owner_generation > 0
            && hex(&self.request_sha256)
            && self.requested_uptime_ms >= self.binding.created_uptime_ms
            && self.deadline_uptime_ms > self.requested_uptime_ms
            && self
                .deadline_uptime_ms
                .saturating_sub(self.requested_uptime_ms)
                <= 40_000
            && self.deadline_uptime_ms <= self.binding.deadline_uptime_ms.saturating_sub(20_000)
    }
    pub(crate) fn key(&self) -> Result<String> {
        if !self.valid() {
            return Err(Error::Invalid);
        }
        Ok(format!(
            "{}-{}.station",
            self.binding.journal_key()?,
            self.trial_id
        ))
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Testing,
    Ready,
    Failed,
    Unknown,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub format: u16,
    pub intent: Intent,
    pub phase: Phase,
    pub sample_uptime_ms: u64,
    pub ap_https_verified: bool,
}
impl Status {
    pub fn validate(&self, now: u64) -> Result<()> {
        if self.format != 1
            || !self.intent.valid()
            || self.sample_uptime_ms > now
            || now.saturating_sub(self.sample_uptime_ms) > HEARTBEAT_MS
            || self.ap_https_verified != matches!(self.phase, Phase::Ready | Phase::Failed)
            || self.phase != Phase::Unknown && now >= self.intent.binding.deadline_uptime_ms
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
}
#[cfg(test)]
pub(crate) mod tests;
