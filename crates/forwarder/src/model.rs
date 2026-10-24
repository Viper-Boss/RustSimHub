// SPDX-License-Identifier: AGPL-3.0-only
use crate::{Error, Result};
use rustsimhub_domain::{InboxCursor, LineEpoch};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;
use zeroize::Zeroizing;

pub const MAX_TARGETS: usize = 8;
pub const MAX_RULES: usize = 32;
pub const MAX_PENDING: usize = 256;
pub const MAX_RECEIPTS: usize = 4096;

pub(crate) fn uuid(s: &str) -> bool {
    Uuid::parse_str(s).is_ok_and(|v| v.hyphenated().to_string() == s)
}
pub(crate) fn seq(s: &str) -> Result<i64> {
    s.parse::<i64>()
        .ok()
        .filter(|n| *n >= 0 && n.to_string() == s)
        .ok_or(Error::Invalid)
}
pub(crate) fn cursor(c: &InboxCursor, realm: &str) -> Result<i64> {
    if c.db_epoch != realm {
        return Err(Error::Stale);
    }
    seq(&c.seq)
}
fn text(s: &str, maximum: usize) -> bool {
    s.len() <= maximum && !s.contains('\0')
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub device_key: [u8; 32],
    pub realm: String,
    pub owner_key: [u8; 32],
    pub owner_generation: u64,
}
impl Binding {
    pub fn validate(&self) -> Result<()> {
        if !uuid(&self.realm)
            || self.owner_generation == 0
            || self.device_key == [0; 32]
            || self.owner_key == [0; 32]
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}

// Secrets never implement Debug or Clone; Debug on containing types is omitted.
pub struct Secret(Zeroizing<String>);
impl Secret {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl Serialize for Secret {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        self.expose().serialize(s)
    }
}
impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        Ok(Self::new(String::deserialize(d)?))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Destination {
    Webhook { url: Secret, bearer: Option<Secret> },
    Bark { server: String, key: Secret },
    Telegram { bot_token: Secret, chat_id: Secret },
}
pub(crate) fn endpoint(url: &str) -> Result<()> {
    if url.len() > 2048
        || url.contains('#')
        || url.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(Error::Invalid);
    }
    let uri: http::Uri = url.parse().map_err(|_| Error::Invalid)?;
    let authority = uri.authority().ok_or(Error::Invalid)?;
    if uri.scheme_str() != Some("https")
        || authority.as_str().contains('@')
        || authority.host().is_empty()
    {
        return Err(Error::Invalid);
    }
    // http::Uri accepts an authority whose port does not fit u16; port_u16()
    // then returns None, which must NOT be mistaken for omitted port 443.
    let suffix = authority
        .as_str()
        .strip_prefix(authority.host())
        .ok_or(Error::Invalid)?;
    if !suffix.is_empty() {
        let port = suffix.strip_prefix(':').ok_or(Error::Invalid)?;
        if port.is_empty()
            || !port.bytes().all(|b| b.is_ascii_digit())
            || port.parse::<u16>().ok().is_none_or(|p| p == 0)
        {
            return Err(Error::Invalid);
        }
    }
    Ok(())
}
impl Destination {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Webhook { url, bearer } => {
                endpoint(url.expose())?;
                if bearer.as_ref().is_some_and(|s| {
                    s.expose().is_empty()
                        || s.expose().len() > 2048
                        || !s.expose().bytes().all(|b| (33..=126).contains(&b))
                }) {
                    return Err(Error::Invalid);
                }
            }
            Self::Bark { server, key } => {
                endpoint(server)?;
                let uri: http::Uri = server.parse().map_err(|_| Error::Invalid)?;
                if uri.path() != "/"
                    || uri.query().is_some()
                    || key.expose().is_empty()
                    || !text(key.expose(), 256)
                {
                    return Err(Error::Invalid);
                }
            }
            Self::Telegram { bot_token, chat_id } => {
                let (number, secret) = bot_token.expose().split_once(':').ok_or(Error::Invalid)?;
                if number.is_empty()
                    || !number.bytes().all(|b| b.is_ascii_digit())
                    || secret.is_empty()
                    || !secret
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                    || bot_token.expose().len() > 256
                    || chat_id.expose().is_empty()
                    || !text(chat_id.expose(), 128)
                {
                    return Err(Error::Invalid);
                }
            }
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub id: String,
    pub destination: Destination,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Share {
    Notice,
    FullText,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    pub targets: Vec<String>,
    pub sender_equals: Option<String>,
    pub body_contains: Option<String>,
    pub line: Option<LineEpoch>,
    pub share: Share,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub enabled: bool,
    pub targets: Vec<Target>,
    pub rules: Vec<Rule>,
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        if self.targets.len() > MAX_TARGETS
            || self.rules.len() > MAX_RULES
            || self.enabled && (self.targets.is_empty() || self.rules.is_empty())
        {
            return Err(Error::Invalid);
        }
        let mut ids = HashSet::new();
        for target in &self.targets {
            if !uuid(&target.id) || !ids.insert(target.id.as_str()) {
                return Err(Error::Invalid);
            }
            target.destination.validate()?;
        }
        let mut rules = HashSet::new();
        for rule in &self.rules {
            let mut seen = HashSet::new();
            if !uuid(&rule.id)
                || !rules.insert(&rule.id)
                || rule.targets.is_empty()
                || rule.targets.len() > MAX_TARGETS
                || rule
                    .targets
                    .iter()
                    .any(|id| !ids.contains(id.as_str()) || !seen.insert(id))
                || rule
                    .sender_equals
                    .as_ref()
                    .is_some_and(|s| s.is_empty() || !text(s, 256))
                || rule
                    .body_contains
                    .as_ref()
                    .is_some_and(|s| s.is_empty() || !text(s, 256))
                || rule.line.is_some_and(|l| l.0 == 0 || l.0 > i64::MAX as u64)
            {
                return Err(Error::Invalid);
            }
        }
        if serde_json::to_vec(self)?.len() > 65536 {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}

// Trusted Core input from a committed SMS detail, not a browser event or raw AT payload.
pub struct Message {
    pub id: String,
    pub line: LineEpoch,
    pub sender: String,
    pub timestamp: String,
    pub body: String,
    pub duplicate_candidate: bool,
    pub binary: bool,
}
impl Message {
    pub fn validate(&self) -> Result<()> {
        if !uuid(&self.id)
            || self.line.0 == 0
            || self.line.0 > i64::MAX as u64
            || self.sender.len() > 256
            || self.timestamp.len() > 128
            || self.body.len() > 4096
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
impl Rule {
    pub fn matches(&self, m: &Message) -> bool {
        !m.binary
            && !m.duplicate_candidate
            && self.sender_equals.as_ref().is_none_or(|s| s == &m.sender)
            && self
                .body_contains
                .as_ref()
                .is_none_or(|s| m.body.contains(s))
            && self.line.is_none_or(|l| l == m.line)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedRequest {
    pub url: Secret,
    pub bearer: Option<Secret>,
    pub body: Secret,
    pub acknowledgement: Acknowledgement,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Acknowledgement {
    Http2xx,
    Bark,
    Telegram,
}
impl PreparedRequest {
    /// Bounded provider acknowledgement. No retry follows any return value.
    /// false after application bytes were sent means Unknown, NOT safe-to-retry.
    pub fn acknowledged(&self, status: u16, response: &[u8]) -> bool {
        if !(200..300).contains(&status) || response.len() > 8192 {
            return false;
        }
        match self.acknowledgement {
            Acknowledgement::Http2xx => true,
            Acknowledgement::Bark => serde_json::from_slice::<serde_json::Value>(response)
                .is_ok_and(|v| v["code"].as_u64() == Some(200)),
            Acknowledgement::Telegram => {
                serde_json::from_slice::<serde_json::Value>(response).is_ok_and(|v| v["ok"] == true)
            }
        }
    }
}
pub fn prepare(
    target: &Target,
    rule: &Rule,
    m: &Message,
    delivery_id: &str,
) -> Result<PreparedRequest> {
    m.validate()?;
    target.destination.validate()?;
    if !uuid(delivery_id)
        || !uuid(&target.id)
        || !uuid(&rule.id)
        || !rule.matches(m)
        || !rule.targets.contains(&target.id)
    {
        return Err(Error::Invalid);
    }
    let content = if rule.share == Share::FullText {
        format!("{}\n{}\n{}", m.sender, m.timestamp, m.body)
    } else {
        "收到一条新短信，请在 RustSimHub 查看。".into()
    };
    let (url, bearer, body, ack) = match &target.destination {
        Destination::Webhook { url, bearer } => {
            let mut value = serde_json::json!({"format":1,"event":"sms_received","delivery_id":delivery_id,"message_id":m.id,"line":m.line,"notice":"new_sms"});
            if rule.share == Share::FullText {
                value["sender"] = m.sender.clone().into();
                value["timestamp"] = m.timestamp.clone().into();
                value["text"] = m.body.clone().into();
            }
            (
                url.expose().to_string(),
                bearer.as_ref().map(|v| Secret::new(v.expose().to_string())),
                value,
                Acknowledgement::Http2xx,
            )
        }
        Destination::Bark { server, key } => (
            format!("{}/push", server.trim_end_matches('/')),
            None,
            serde_json::json!({"device_key":key.expose(),"title":"RustSimHub 新短信","body":content,"group":"RustSimHub"}),
            Acknowledgement::Bark,
        ),
        Destination::Telegram { bot_token, chat_id } => {
            // Telegram accepts at most 4096 text characters. Preserve the
            // complete bounded body rather than truncating it or retrying a
            // split message; omit the metadata prefix only when it won't fit.
            let content = if content.encode_utf16().count() > 4096 {
                m.body.as_str()
            } else {
                content.as_str()
            };
            (
                format!(
                    "https://api.telegram.org/bot{}/sendMessage",
                    bot_token.expose()
                ),
                None,
                serde_json::json!({"chat_id":chat_id.expose(),"text":content,"link_preview_options":{"is_disabled":true}}),
                Acknowledgement::Telegram,
            )
        }
    };
    Ok(PreparedRequest {
        url: Secret::new(url),
        bearer,
        body: Secret::new(serde_json::to_string(&body)?),
        acknowledgement: ack,
    })
}
