// SPDX-License-Identifier: AGPL-3.0-only
//! Bounded, credential-free rule extensions. No clock, network or filesystem IO.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use time::{format_description::well_known::Rfc3339, OffsetDateTime, UtcOffset};

pub const RULES_REVISION: u32 = 2;
pub const MAX_PATTERN_BYTES: usize = 256;
pub const MAX_PATTERN_PROGRAM: usize = 8192;

/// Compiled once when a policy is decoded, shared when its public view is cloned.
/// Never expose the parser error (which can contain the owner's filter text).
#[derive(Clone)]
pub struct BodyPattern {
    source: String,
    regex: Arc<regex_lite::Regex>,
}
impl BodyPattern {
    pub fn new(source: String) -> Result<Self> {
        if source.is_empty() || source.len() > MAX_PATTERN_BYTES || source.contains('\0') {
            return Err(Error::Invalid);
        }
        let regex = regex_lite::RegexBuilder::new(&source)
            .size_limit(MAX_PATTERN_PROGRAM)
            .nest_limit(16)
            .build()
            .map_err(|_| Error::Invalid)?;
        Ok(Self {
            source,
            regex: Arc::new(regex),
        })
    }
    pub fn is_match(&self, body: &str) -> bool {
        body.len() <= 4096 && self.regex.is_match(body)
    }
}
impl Serialize for BodyPattern {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.source)
    }
}
impl<'de> Deserialize<'de> for BodyPattern {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        Self::new(String::deserialize(d)?)
            .map_err(|_| serde::de::Error::custom("invalid_rule_pattern"))
    }
}

/// Fixed UTC offset, inclusive start/exclusive end. Cross-midnight is explicit.
/// Use the SMS's RFC3339 timestamp, never the time at which a backlog is read.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeWindow {
    pub start_minute: u16,
    pub end_minute: u16,
    pub utc_offset_minutes: i16,
}
impl TimeWindow {
    pub fn validate(&self) -> Result<()> {
        if self.start_minute >= 1440
            || self.end_minute >= 1440
            || self.start_minute == self.end_minute
            || !(-840..=840).contains(&self.utc_offset_minutes)
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
    pub fn matches(&self, timestamp: &str) -> bool {
        if self.validate().is_err() || timestamp.len() > 128 {
            return false;
        }
        let Ok(value) = OffsetDateTime::parse(timestamp, &Rfc3339) else {
            return false;
        };
        let Ok(offset) = UtcOffset::from_whole_seconds(i32::from(self.utc_offset_minutes) * 60)
        else {
            return false;
        };
        let Some(local) = value.checked_to_offset(offset) else {
            return false;
        };
        let minute = u16::from(local.hour()) * 60 + u16::from(local.minute());
        if self.start_minute < self.end_minute {
            (self.start_minute..self.end_minute).contains(&minute)
        } else {
            minute >= self.start_minute || minute < self.end_minute
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleConditions {
    pub body_regex: Option<BodyPattern>,
    pub time_window: Option<TimeWindow>,
}
impl RuleConditions {
    pub fn validate(&self) -> Result<()> {
        if self.body_regex.is_none() && self.time_window.is_none() {
            return Err(Error::Invalid);
        }
        if let Some(window) = &self.time_window {
            window.validate()?;
        }
        Ok(())
    }
    pub fn matches(&self, body: &str, timestamp: &str) -> bool {
        self.validate().is_ok()
            && self
                .body_regex
                .as_ref()
                .is_none_or(|regex| regex.is_match(body))
            && self
                .time_window
                .as_ref()
                .is_none_or(|window| window.matches(timestamp))
    }
}

/// Deliberately mechanical, not an OTP detector: hide ASCII letters and all
/// Unicode numeric characters; keep Chinese prose and punctuation. Sensitive
/// messages should use Notice, which never includes any original body at all.
pub fn masked_text(body: &str) -> String {
    body.chars()
        .map(|c| {
            if c.is_ascii_alphabetic() || c.is_numeric() {
                '*'
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
