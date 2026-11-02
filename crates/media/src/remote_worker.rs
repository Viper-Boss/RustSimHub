// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit persistent local v2 controller. No network listener, call commands,
//! Root/TURN credentials, StopAll, LAN fallback or authorization flags.
use crate::{
    protocol::Credentials,
    worker::{Counters, Failure, Phase},
    Error,
};
use serde::{Deserialize, Serialize};
pub const PROTOCOL: u16 = 2;
pub const MAX_FRAME: usize = 24_576;
pub const MAX_SDP: usize = 16_384;
pub const PREPARE_MS: u64 = 10_000;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub protocol: u16,
    pub id: u64,
    pub command: Command,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Prepare {
        session: String,
        relay_handle: String,
        expected_peer: [u8; 32],
        offer: String,
    },
    Inspect {
        session: String,
    },
    Authorize {
        session: String,
        credentials: Credentials,
    },
    Stop {
        session: String,
    },
}
impl Command {
    pub fn session(&self) -> &str {
        match self {
            Self::Prepare { session, .. }
            | Self::Inspect { session }
            | Self::Authorize { session, .. }
            | Self::Stop { session } => session,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Release {
    NotRequested,
    Unknown,
}
// Metadata only. Never convert phase/peer_key into a Permit or Core capability.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub session: String,
    pub phase: Phase,
    pub peer_key: Option<[u8; 32]>,
    pub answer: Option<String>,
    pub pcm_opened: bool,
    pub counters: Counters,
    pub failure: Option<Failure>,
    pub release: Release,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub protocol: u16,
    pub id: u64,
    pub result: std::result::Result<Snapshot, Failure>,
}
fn object(bytes: &[u8]) -> Result<serde_json::Value, Error> {
    if bytes.is_empty()
        || bytes.len() > MAX_FRAME
        || bytes.iter().copied().find(|b| !b.is_ascii_whitespace()) != Some(b'{')
    {
        return Err(Error::Denied);
    }
    serde_json::from_slice(bytes).map_err(|_| Error::Denied)
}
pub fn relay_handle(value: &str) -> bool {
    (1..=32).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-'))
}
pub fn parse(bytes: &[u8]) -> Result<Request, Error> {
    let value = object(bytes)?;
    let command = value
        .get("command")
        .filter(|v| v.is_object())
        .ok_or(Error::Denied)?;
    if command.get("op").and_then(|v| v.as_str()) == Some("authorize")
        && !command.get("credentials").is_some_and(|v| v.is_object())
    {
        return Err(Error::Denied);
    }
    let request: Request = serde_json::from_slice(bytes).map_err(|_| Error::Denied)?;
    if request.protocol != PROTOCOL
        || request.id == 0
        || !crate::worker::session_id(request.command.session())
    {
        return Err(Error::Denied);
    }
    match &request.command {
        Command::Prepare {
            relay_handle: handle,
            expected_peer,
            offer,
            ..
        } if !relay_handle(handle)
            || *expected_peer == [0; 32]
            || offer.is_empty()
            || offer.len() > MAX_SDP =>
        {
            return Err(Error::Denied)
        }
        Command::Authorize { credentials, .. }
            if credentials.boot.len() != 64
                || !credentials
                    .boot
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) =>
        {
            return Err(Error::Denied)
        }
        _ => {}
    }
    Ok(request)
}
pub fn parse_reply(bytes: &[u8], id: u64, session: &str) -> Result<Reply, Error> {
    let value = object(bytes)?;
    if value
        .get("result")
        .and_then(|v| v.get("Ok"))
        .is_some_and(|v| !v.is_object())
    {
        return Err(Error::Denied);
    }
    let reply: Reply = serde_json::from_slice(bytes).map_err(|_| Error::Denied)?;
    if reply.protocol != PROTOCOL || reply.id != id || id == 0 {
        return Err(Error::Denied);
    }
    if let Ok(snapshot) = &reply.result {
        if !value
            .get("result")
            .and_then(|v| v.get("Ok"))
            .and_then(|v| v.get("counters"))
            .is_some_and(|v| v.is_object())
        {
            return Err(Error::Denied);
        }
        if snapshot.session != session
            || !crate::worker::session_id(&snapshot.session)
            || snapshot
                .answer
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.len() > MAX_SDP)
            || matches!(
                snapshot.phase,
                Phase::Verified | Phase::Authorizing | Phase::Active
            ) && snapshot.peer_key.is_none_or(|k| k == [0; 32])
            || matches!(
                snapshot.phase,
                Phase::Handshaking | Phase::Verified | Phase::Authorizing | Phase::Closed
            ) && snapshot.pcm_opened
            || snapshot.phase == Phase::Active && !snapshot.pcm_opened
        {
            return Err(Error::Denied);
        }
    }
    Ok(reply)
}
#[cfg(target_os = "linux")]
mod transport;
#[cfg(target_os = "linux")]
pub use transport::{CancellationGuard, Client};
#[cfg(test)]
mod tests;
