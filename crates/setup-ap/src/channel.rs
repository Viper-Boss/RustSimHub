// SPDX-License-Identifier: AGPL-3.0-only
//! Finite Core/Root lease. No AP password, bearer, settings dictionary or command input.
use crate::{Error, Result, MAX_WINDOW_SECONDS};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_BYTES: usize = 8192;
pub const HEARTBEAT_MS: u64 = 15_000;
pub fn uptime_ms() -> Result<u64> {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
    let sec = u64::try_from(t.tv_sec).map_err(|_| Error::Io)?;
    let nano = u64::try_from(t.tv_nsec).map_err(|_| Error::Io)?;
    sec.checked_mul(1000)
        .and_then(|v| v.checked_add(nano / 1_000_000))
        .ok_or(Error::Io)
}
pub fn boot_id() -> Result<String> {
    let value =
        std::fs::read_to_string("/proc/sys/kernel/random/boot_id").map_err(|_| Error::Io)?;
    if value.len() > 64 || !uuid(value.trim()) {
        return Err(Error::Invalid);
    }
    Ok(value.trim().into())
}
fn hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Start,
    KeepAlive,
    Cancel,
}
// Deliberately no Debug: the nonce belongs to the protected local lease channel.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub request_id: String,
    pub boot_id: String,
    pub realm: String,
    pub device_key_sha256: String,
    pub material_sha256: String,
    pub core_instance: String,
    pub nonce: String,
    pub created_uptime_ms: u64,
    pub deadline_uptime_ms: u64,
}
impl Binding {
    pub fn valid(&self) -> bool {
        uuid(&self.request_id)
            && uuid(&self.boot_id)
            && uuid(&self.realm)
            && hex(&self.device_key_sha256)
            && hex(&self.material_sha256)
            && hex(&self.core_instance)
            && hex(&self.nonce)
            && self.deadline_uptime_ms > self.created_uptime_ms
            && self
                .deadline_uptime_ms
                .saturating_sub(self.created_uptime_ms)
                <= u64::from(MAX_WINDOW_SECONDS) * 1000
    }
    pub fn journal_key(&self) -> Result<String> {
        if !self.valid() {
            return Err(Error::Invalid);
        }
        Ok(Sha256::digest(self.request_id.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect())
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hint {
    pub format: u16,
    pub binding: Binding,
    pub action: Action,
    pub sequence: u64,
    pub valid_until_uptime_ms: u64,
}
impl Hint {
    pub fn validate(&self, now: u64, boot: &str) -> Result<()> {
        if self.format != 1
            || !self.binding.valid()
            || self.binding.boot_id != boot
            || self.sequence == 0
            || self.sequence > 1024
            || self.binding.created_uptime_ms > now
            || now >= self.binding.deadline_uptime_ms
            || self.valid_until_uptime_ms <= now
            || self.valid_until_uptime_ms > self.binding.deadline_uptime_ms
            || self.valid_until_uptime_ms.saturating_sub(now) > HEARTBEAT_MS
            || self.action == Action::Start && self.sequence != 1
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub fn parse(bytes: &[u8], now: u64, boot: &str) -> Result<Self> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::Invalid);
        }
        let v: Self = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
        v.validate(now, boot)?;
        Ok(v)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Idle,
    Starting,
    Ready,
    Closing,
    Closed,
    Unknown,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Availability {
    pub format: u16,
    pub boot_id: String,
    pub realm: String,
    pub device_key_sha256: String,
    pub material_sha256: String,
    pub sample_uptime_ms: u64,
    pub tls_origin_ready: bool,
    pub gate_ready: bool,
}
impl Availability {
    pub fn validate(&self, now: u64, boot: &str, realm: &str, key: &str) -> Result<()> {
        if self.format != 1
            || self.boot_id != boot
            || self.realm != realm
            || self.device_key_sha256 != key
            || !uuid(&self.boot_id)
            || !uuid(&self.realm)
            || !hex(&self.device_key_sha256)
            || !hex(&self.material_sha256)
            || self.sample_uptime_ms > now
            || now.saturating_sub(self.sample_uptime_ms) > HEARTBEAT_MS
            || !self.tls_origin_ready
            || !self.gate_ready
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub fn parse(bytes: &[u8], now: u64, boot: &str, realm: &str, key: &str) -> Result<Self> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::Invalid);
        }
        let v: Self = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
        v.validate(now, boot, realm, key)?;
        Ok(v)
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub format: u16,
    pub binding: Binding,
    pub state: State,
    pub sequence: u64,
    pub sample_uptime_ms: u64,
    pub rollback_verified: bool,
    pub guard_released: bool,
}
impl Status {
    pub fn validate(&self, binding: &Binding, now: u64) -> Result<()> {
        if self.format != 1
            || self.binding != *binding
            || self.state == State::Idle
            || !binding.valid()
            || self.sequence == 0
            || self.sequence > 1024
            || self.sample_uptime_ms > now
            || now.saturating_sub(self.sample_uptime_ms) > HEARTBEAT_MS
            || self.state == State::Closed && !(self.rollback_verified && self.guard_released)
            || self.guard_released && !self.rollback_verified
            || self.state != State::Closed && self.guard_released
            || matches!(self.state, State::Idle | State::Starting | State::Ready)
                && self.rollback_verified
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub fn parse(bytes: &[u8], binding: &Binding, now: u64) -> Result<Self> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::Invalid);
        }
        let v: Self = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
        v.validate(binding, now)?;
        Ok(v)
    }
}
/// A Root worker retains this exact binding; another start never retargets a live AP.
pub struct RootLease {
    binding: Binding,
    last_sequence: u64,
    last_hint: Hint,
}
impl RootLease {
    pub fn begin(
        hint: Hint,
        now: u64,
        boot: &str,
        realm: &str,
        device_key_sha256: &str,
        material_sha256: &str,
    ) -> Result<Self> {
        hint.validate(now, boot)?;
        if hint.action != Action::Start
            || hint.binding.realm != realm
            || hint.binding.device_key_sha256 != device_key_sha256
            || hint.binding.material_sha256 != material_sha256
        {
            return Err(Error::Denied);
        }
        Ok(Self {
            binding: hint.binding.clone(),
            last_sequence: hint.sequence,
            last_hint: hint,
        })
    }
    pub fn check(&mut self, hint: Hint, now: u64, boot: &str) -> Result<bool> {
        hint.validate(now, boot)?;
        if hint.binding != self.binding
            || hint.sequence < self.last_sequence
            || hint.sequence == self.last_sequence && hint != self.last_hint
        {
            return Err(Error::Changed);
        }
        if hint.sequence > self.last_sequence && hint.action == Action::Start {
            return Err(Error::Denied);
        }
        if self.last_hint.action == Action::Cancel {
            return Ok(false);
        }
        self.last_sequence = hint.sequence;
        self.last_hint = hint;
        Ok(self.last_hint.action != Action::Cancel)
    }
}

pub mod files;
#[cfg(test)]
mod tests;
