// SPDX-License-Identifier: AGPL-3.0-only
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NasEntity {
    Status,
    Sms,
    Calls,
    SmsDetail,
}
