// SPDX-License-Identifier: AGPL-3.0-only
//! Private Root/Core health v1. Never an Owner/NAS/BLE command or a JSON capability.
use serde::{Deserialize, Serialize};
pub const VERSION: u16 = 1;
pub const MAX_FRAME: usize = 8192;
pub const READ_MS: u64 = 250;
pub const MAX_OBSERVATION_MS: u64 = 5000;
pub const SOCKET: &str = "/run/rustsimhub/update-health/core.sock";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Observe,
    Recheck,
    Finish,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub protocol: u16,
    pub step: Step,
    pub challenge: String,
    pub connection: Option<String>,
}
impl Request {
    pub fn valid(&self) -> bool {
        self.protocol == VERSION
            && digest(&self.challenge)
            && match self.step {
                Step::Observe => self.connection.is_none(),
                Step::Recheck | Step::Finish => self.connection.as_deref().is_some_and(digest),
            }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Policy {
    Off,
    On,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Forwarding {
    pub policy: Policy,
    pub generation: u64,
    pub outbox_schema: Option<u32>,
    pub worker_admitted: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub protocol: u16,
    pub challenge: String,
    pub connection: String,
    pub core_instance: String,
    pub source_sha256: Option<String>,
    pub business_ready: bool,
    pub data_schema: u32,
    pub data_realm: String,
    pub line_generation: u64,
    pub data_device: u64,
    pub data_inode: u64,
    pub authorization_generation: u64,
    pub authorization_binding: String,
    pub forwarding: Forwarding,
}
impl Snapshot {
    pub fn valid(&self) -> bool {
        self.protocol == VERSION
            && digest(&self.challenge)
            && digest(&self.connection)
            && digest(&self.core_instance)
            && self.source_sha256.as_deref().is_none_or(digest)
            && matches!(self.data_schema, 6 | 7 | 9)
            && realm(&self.data_realm)
            && self.line_generation > 0
            && self.data_inode > 0
            && self.authorization_generation > 0
            && digest(&self.authorization_binding)
            && match self.forwarding.outbox_schema {
                None => self.forwarding.generation == 0 && self.forwarding.policy == Policy::Off,
                Some(3 | 4) => true,
                _ => false,
            }
            && match self.forwarding.policy {
                Policy::Off => !self.forwarding.worker_admitted,
                Policy::On => self.forwarding.worker_admitted && self.forwarding.generation > 0,
            }
    }
}
pub fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn realm(s: &str) -> bool {
    s.len() == 36
        && s != "00000000-0000-0000-0000-000000000000"
        && s.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
/// Reject unknown/duplicate fields before any business source is reserved.
pub fn parse_request(bytes: &[u8]) -> Result<Request, &'static str> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err("health frame length");
    }
    let request: Request = serde_json::from_slice(bytes).map_err(|_| "health request")?;
    if !request.valid() {
        return Err("health request binding");
    }
    Ok(request)
}
pub fn parse_snapshot(bytes: &[u8]) -> Result<Snapshot, &'static str> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err("health frame length");
    }
    let snapshot: Snapshot = serde_json::from_slice(bytes).map_err(|_| "health snapshot")?;
    if !snapshot.valid() {
        return Err("health snapshot binding");
    }
    Ok(snapshot)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closed_request_has_no_pid_path_clock_or_claimed_health_inputs() {
        let request = Request {
            protocol: VERSION,
            step: Step::Observe,
            challenge: "a".repeat(64),
            connection: None,
        };
        let value = serde_json::to_value(request).unwrap();
        assert!(parse_request(&serde_json::to_vec(&value).unwrap()).is_ok());
        for field in ["pid", "path", "healthy", "authorization", "now_ms", "epoch"] {
            let mut bad = value.clone();
            bad[field] = 1.into();
            assert!(parse_request(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        let raw = serde_json::to_string(&value).unwrap();
        let duplicate = raw.replacen("\"protocol\":1", "\"protocol\":1,\"protocol\":1", 1);
        assert!(parse_request(duplicate.as_bytes()).is_err());
        assert!(parse_request(&vec![b' '; MAX_FRAME + 1]).is_err());
        let mut bad = value.clone();
        bad["protocol"] = 2.into();
        assert!(parse_request(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
}
