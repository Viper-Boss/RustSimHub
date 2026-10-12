// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum Scope {
    #[serde(rename = "status.read")]
    StatusRead = 0,
    #[serde(rename = "sms.read")]
    SmsRead = 1,
    #[serde(rename = "sms.send")]
    SmsSend = 2,
    #[serde(rename = "calls.read")]
    CallsRead = 3,
    #[serde(rename = "calls.control")]
    CallsControl = 4,
    #[serde(rename = "media.use")]
    MediaUse = 5,
    #[serde(rename = "network.manage")]
    NetworkManage = 6,
    #[serde(rename = "clients.manage")]
    ClientsManage = 7,
    #[serde(rename = "update.manage")]
    UpdateManage = 8,
    #[serde(rename = "system.manage")]
    SystemManage = 9,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Owner,
    Client,
    Nas,
}

pub const ALL_SCOPES: u16 = (1 << 10) - 1;
pub fn mask(scopes: &[Scope]) -> Option<u16> {
    if scopes.is_empty() || scopes.len() > 10 {
        return None;
    }
    let mut bits = 0;
    for scope in scopes {
        let bit = 1 << (*scope as u16);
        if bits & bit != 0 {
            return None;
        }
        bits |= bit;
    }
    Some(bits)
}
pub fn allows(bits: u16, scope: Scope) -> bool {
    bits & (1 << scope as u16) != 0
}

/// Durable record; never inferred from an SSH UID or supplied remote role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Principal {
    pub id: String,
    pub public_key: [u8; 32],
    pub kind: Kind,
    pub scopes: u16,
    pub generation: u64,
    pub active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_empty_and_ungranted_scopes_are_rejected() {
        assert_eq!(mask(&[]), None);
        assert_eq!(mask(&[Scope::SmsSend, Scope::SmsSend]), None);
        assert!(!allows(mask(&[Scope::StatusRead]).unwrap(), Scope::SmsSend));
    }
}
