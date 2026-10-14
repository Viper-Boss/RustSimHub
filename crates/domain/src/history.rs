// SPDX-License-Identifier: AGPL-3.0-only
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryEntity {
    Sms,
    Calls,
    Contacts,
}
impl HistoryEntity {
    pub fn bit(self) -> u8 {
        match self {
            Self::Sms => 1,
            Self::Calls => 2,
            Self::Contacts => 4,
        }
    }
    pub fn table(self) -> &'static str {
        match self {
            Self::Sms => "sms",
            Self::Calls => "call_history",
            Self::Contacts => "contacts",
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryCursor {
    pub archive_epoch: String,
    pub audience: String,
    pub source: String,
    pub entity: HistoryEntity,
    pub after: String,
}
