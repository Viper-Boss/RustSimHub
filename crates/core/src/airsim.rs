// SPDX-License-Identifier: AGPL-3.0-only
//! Core side of the AirSIM gateway. Edge only relays the app's plain requests;
//! pairing secrets, authentication and every business decision stay here.
//!
//! The gateway acts as one ordinary Client principal whose Ed25519 seed never
//! leaves Core. A recent owner creates it by enabling AirSIM and revokes it by
//! disabling AirSIM or from the client list, so paired iPhones get exactly the
//! scopes below through the same sender and authorization as a browser.
//!
//! Calls go through the ordinary call controller: the gateway principal
//! prepares and submits each action, holds the call lease on the phone's
//! behalf and renews it only while that phone keeps asking. When the phone
//! goes away the lease expires and Core's own cleanup ends the call.
//! PCM credentials use the same active-call/media checks as browser audio.
use rustsimhub_airsim::{
    BackendError, Call, CallAction, Gateway, Line, Message, Outcome, Phase, Poll, Request, State,
    EVENT_POLL_PERIOD_MS,
};
use rustsimhub_auth::{Auth, Error};
use rustsimhub_domain::{CallLease, InboxCursor};
use rustsimhub_policy::{Kind, Scope};
use rustsimhub_storage::LineStatus;
use rustsimhub_wire::AuthCommand;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::Write,
    net::IpAddr,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;
#[path = "airsim_pcm.rs"]
mod pcm;

pub(crate) const CONTROL_PORT: u16 = 7592;
pub(crate) const PCM_PORT: u16 = 7593;
pub(crate) const PAIRING_PORT: u16 = 7594;
const FILE: &str = "airsim.json";
const MAX_FILE: u64 = 256 * 1024;
const MAX_POLLS: usize = 16;
const SCOPES: [Scope; 6] = [
    Scope::StatusRead,
    Scope::SmsRead,
    Scope::SmsSend,
    Scope::CallsRead,
    Scope::CallsControl,
    Scope::MediaUse,
];

/// Everything that survives a restart. Holds the gateway key and the phones'
/// secrets, so it lives only in Core's private state directory, mode 0600.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    format: u32,
    enabled: bool,
    /// 8000 today; 16000 once the app speaks wideband.
    pcm_rate: u32,
    seed: Option<String>,
    gateway: State,
    /// Per pairing: the inbox position when it paired. Only later messages are shown.
    baselines: HashMap<String, InboxCursor>,
}

/// The call controller as the gateway needs it; a trait so tests can script a modem.
pub(crate) trait CallPort: Send + Sync {
    fn request(
        &self,
        runtime: &tokio::runtime::Handle,
        command: AuthCommand,
    ) -> Result<Value, &'static str>;
}
impl CallPort for crate::calls::Controller {
    fn request(
        &self,
        runtime: &tokio::runtime::Handle,
        command: AuthCommand,
    ) -> Result<Value, &'static str> {
        runtime.block_on(crate::calls::Controller::request(self, command))
    }
}

/// How often a present phone's lease is renewed, and how many calls are remembered.
#[cfg(not(test))]
const RENEW_EVERY_MS: i64 = 5_000;
/// Tests renew on every observation instead of waiting.
#[cfg(test)]
const RENEW_EVERY_MS: i64 = 0;
const MAX_SEEN_CALLS: usize = 32;

/// A call the gateway itself observed. Core's call list carries no times, so
/// the phone is shown only calls seen since this Core started, with real times.
struct Seen {
    id: String,
    incoming: bool,
    number: String,
    phase: Phase,
    created_ms: i64,
    updated_ms: i64,
    ended_ms: Option<i64>,
    answered: bool,
}
#[derive(Default)]
struct CallBook {
    /// The pairing that controls the call, its lease, and when it was last renewed.
    lease: Option<(String, CallLease, i64)>,
    seen: Vec<Seen>,
}

struct Inner {
    pcm: Option<pcm::State>,
    call_book: CallBook,
    gateway: Gateway,
    enabled: bool,
    pcm_rate: u32,
    seed: Option<Zeroizing<[u8; 32]>>,
    baselines: HashMap<String, InboxCursor>,
    session: Option<Zeroizing<String>>,
    polls: HashMap<String, (Poll, i64)>,
}

#[derive(Clone)]
pub(crate) struct Controller {
    inner: Arc<Mutex<Inner>>,
    path: PathBuf,
    sender: Option<crate::sender::Controller>,
    calls: Option<Arc<dyn CallPort>>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
fn random<const N: usize>() -> Result<[u8; N], Error> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|_| Error::Entropy)?;
    Ok(bytes)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `YYYY-MM-DDTHH:MM:SS` with an optional `Z` or `±HH:MM`, as the modem
/// reports it. Anything else is unknown rather than guessed.
pub(crate) fn sms_time_ms(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() < 19
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let number = |range: std::ops::Range<usize>| -> Option<i64> {
        let part = text.get(range)?;
        part.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| part.parse().ok())?
    };
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let offset_minutes = match text.get(19..)? {
        "" | "Z" => 0,
        zone if zone.len() == 6 && (zone.starts_with('+') || zone.starts_with('-')) => {
            if zone.as_bytes()[3] != b':' {
                return None;
            }
            let minutes = number(20..22)? * 60 + number(23..25)?;
            if zone.starts_with('-') {
                -minutes
            } else {
                minutes
            }
        }
        _ => return None,
    };
    // Days-from-civil (Howard Hinnant).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some((days * 86_400 + hour * 3600 + minute * 60 + second - offset_minutes * 60) * 1000)
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension("json.new");
    let _ = std::fs::remove_file(&temporary);
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    std::fs::rename(&temporary, path)?;
    if let Some(parent) = path.parent() {
        std::fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn lock_auth(auth: &Arc<Mutex<Auth>>) -> Result<std::sync::MutexGuard<'_, Auth>, BackendError> {
    // Auth is shared with every other request; wait briefly instead of failing a phone's poll.
    for _ in 0..50 {
        if let Ok(guard) = auth.try_lock() {
            return Ok(guard);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err(BackendError::Unavailable)
}

struct Core<'a> {
    auth: &'a Arc<Mutex<Auth>>,
    seed: Option<&'a Zeroizing<[u8; 32]>>,
    session: &'a mut Option<Zeroizing<String>>,
    baselines: &'a HashMap<String, InboxCursor>,
    sender: Option<&'a crate::sender::Controller>,
    calls: Option<&'a dyn CallPort>,
    book: &'a mut CallBook,
    runtime: &'a tokio::runtime::Handle,
}
fn call_error(code: &'static str) -> BackendError {
    match code {
        "call_owned" => BackendError::Message(409, "线路正由其他设备控制".into()),
        "modem_busy" => BackendError::Message(409, "线路正忙".into()),
        "line_unavailable" | "calls_read_only" | "call_controller_busy" | "controller_stopped" => {
            BackendError::Unavailable
        }
        // Still pending in Core: the outcome is unknown, never "failed".
        "pending_query_operation" => BackendError::Message(502, "结果未确认，请勿重复操作".into()),
        other => BackendError::Message(502, other.into()),
    }
}
/// Digits with an optional leading `+`, as Core requires; common separators are dropped.
pub(crate) fn dial_number(text: &str) -> Option<String> {
    let cleaned: String = text
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '(' | ')'))
        .collect();
    let digits = cleaned.strip_prefix('+').unwrap_or(&cleaned);
    ((3..=32).contains(&cleaned.len())
        && !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit()))
    .then_some(cleaned)
}
impl Core<'_> {
    fn call(&mut self, command: AuthCommand) -> Result<Value, BackendError> {
        let calls = self.calls.ok_or(BackendError::Unavailable)?;
        calls.request(self.runtime, command).map_err(call_error)
    }
    /// Prepares and submits one call action as the gateway principal.
    /// Returns the call id and the lease Core reports for it, if any.
    fn act(
        &mut self,
        action: rustsimhub_domain::CallAction,
        call_id: Option<String>,
        payload: String,
        lease: Option<CallLease>,
        request_id: &str,
    ) -> Result<(String, Option<CallLease>), BackendError> {
        let token = self.token()?;
        let line = lock_auth(self.auth)?.line_status(&token).map_err(storage)?;
        let urgent = action == rustsimhub_domain::CallAction::Hangup;
        let prepared = self.call(AuthCommand::PrepareCall {
            token: token.to_string(),
            line_epoch: line.line.0,
            idempotency_key: request_id.to_owned(),
            action,
            call_id,
            payload,
        })?;
        let operation_id = prepared["operation"]["id"]
            .as_str()
            .ok_or(BackendError::Other)?
            .to_owned();
        let submitted = self.call(AuthCommand::SubmitCall {
            token: token.to_string(),
            operation_id,
            lease,
            urgent,
        })?;
        match submitted["operation"]["state"].as_str() {
            Some("submitted") => {}
            // The modem may or may not have acted: say so, never repeat it here.
            Some("unknown") | Some("submitting") => {
                return Err(BackendError::Message(
                    502,
                    "结果未确认，请勿重复操作".into(),
                ))
            }
            _ => return Err(BackendError::Message(502, "操作未执行".into())),
        }
        let id = submitted["operation"]["call_id"]
            .as_str()
            .ok_or(BackendError::Other)?
            .to_owned();
        let lease = serde_json::from_value::<CallLease>(submitted["lease"].clone())
            .ok()
            .filter(|l| l.call_id == id);
        Ok((id, lease))
    }
    /// Renews the lease while its phone is present; forgets it once the call is gone.
    fn keep(&mut self, pairing_id: &str, live: &[String], now: i64) {
        let Some((owner, lease, renewed)) = self.book.lease.clone() else {
            return;
        };
        if !live.contains(&lease.call_id) {
            self.book.lease = None;
            return;
        }
        if owner != pairing_id || now - renewed < RENEW_EVERY_MS {
            return;
        }
        let Ok(token) = self.token() else { return };
        match self.call(AuthCommand::RenewCall {
            token: token.to_string(),
            lease,
        }) {
            Ok(value) => match serde_json::from_value::<CallLease>(value["lease"].clone()) {
                Ok(next) => self.book.lease = Some((owner, next, now)),
                Err(_) => self.book.lease = None,
            },
            // Core no longer honours it: control is lost, and Core cleans the call up.
            Err(_) => self.book.lease = None,
        }
    }
    /// The gateway principal's current bearer, logging in again when it expired.
    fn token(&mut self) -> Result<Zeroizing<String>, BackendError> {
        let mut auth = lock_auth(self.auth)?;
        if let Some(token) = self.session.as_ref() {
            if auth.authorize(token, Scope::StatusRead).is_ok() {
                return Ok(token.clone());
            }
        }
        *self.session = None;
        let seed = self.seed.ok_or(BackendError::Forbidden)?;
        match auth.gateway_session(seed) {
            Ok(token) => {
                let token = Zeroizing::new(token.expose().to_owned());
                *self.session = Some(token.clone());
                Ok(token)
            }
            // Revoked by the owner: the phones' authority is gone.
            Err(Error::Forbidden) => Err(BackendError::Forbidden),
            Err(_) => Err(BackendError::Unavailable),
        }
    }
}
fn storage(error: Error) -> BackendError {
    match error {
        Error::Forbidden => BackendError::Forbidden,
        _ => BackendError::Unavailable,
    }
}
impl Core<'_> {
    fn calls_for(
        &mut self,
        pairing_id: &str,
        history: usize,
        keep: impl Fn(&CallLease) -> bool,
    ) -> Result<(Vec<Call>, Vec<Call>), BackendError> {
        if self.calls.is_none() {
            return Ok((Vec::new(), Vec::new()));
        }
        let token = self.token()?;
        let status = self.call(AuthCommand::CallStatus {
            token: token.to_string(),
        })?;
        // An unconfirmed catalogue is unknown, not "no calls".
        if status["available"] != true {
            return Err(BackendError::Unavailable);
        }
        let now = now_ms();
        let mut live_ids = Vec::new();
        for summary in status["live_calls"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            let (Some(id), Some(state)) = (summary["id"].as_str(), summary["modem_state"].as_i64())
            else {
                continue;
            };
            if summary["availability"] != "observed" {
                continue;
            }
            let incoming = summary["direction"] == 1;
            let phase = match state {
                0 | 1 => Phase::Dialing,
                2 => Phase::Connecting,
                3 | 6 => Phase::Ringing,
                4 | 5 => Phase::Active,
                _ => Phase::Ended,
            };
            if phase != Phase::Ended {
                live_ids.push(id.to_owned());
            }
            match self.book.seen.iter_mut().find(|c| c.id == id) {
                Some(seen) => {
                    if seen.phase != phase {
                        seen.phase = phase;
                        seen.updated_ms = now;
                    }
                    seen.answered |= phase == Phase::Active;
                    if phase == Phase::Ended && seen.ended_ms.is_none() {
                        seen.ended_ms = Some(now);
                    }
                }
                None if phase != Phase::Ended => {
                    // The number lives only in the call's detail; read it once.
                    let number = lock_auth(self.auth)?
                        .call_detail(&token, id.to_owned())
                        .ok()
                        .and_then(|d| d.number)
                        .unwrap_or_default();
                    self.book.seen.push(Seen {
                        id: id.to_owned(),
                        incoming,
                        number,
                        phase,
                        created_ms: now,
                        updated_ms: now,
                        ended_ms: None,
                        answered: phase == Phase::Active,
                    });
                }
                None => {}
            }
        }
        // A call that left the live list has ended.
        for seen in &mut self.book.seen {
            if seen.ended_ms.is_none() && !live_ids.contains(&seen.id) {
                seen.phase = Phase::Ended;
                seen.updated_ms = now;
                seen.ended_ms = Some(now);
            }
        }
        if self.book.seen.len() > MAX_SEEN_CALLS {
            let extra = self.book.seen.len() - MAX_SEEN_CALLS;
            let mut dropped = 0;
            self.book.seen.retain(|c| {
                let drop = dropped < extra && c.ended_ms.is_some();
                dropped += usize::from(drop);
                !drop
            });
        }
        if self
            .book
            .lease
            .as_ref()
            .is_some_and(|(_, lease, _)| keep(lease))
        {
            self.keep(pairing_id, &live_ids, now);
        }
        let view = |c: &Seen| Call {
            id: c.id.clone(),
            incoming: c.incoming,
            phase: c.phase,
            number: c.number.clone(),
            created_ms: c.created_ms,
            updated_ms: c.updated_ms,
            ended_ms: c.ended_ms,
            missed: c.incoming && c.ended_ms.is_some() && !c.answered,
        };
        let live = self
            .book
            .seen
            .iter()
            .filter(|c| c.ended_ms.is_none())
            .map(view)
            .collect();
        let ended = self
            .book
            .seen
            .iter()
            .rev()
            .filter(|c| c.ended_ms.is_some())
            .take(history)
            .map(view)
            .collect();
        Ok((live, ended))
    }
}
impl rustsimhub_airsim::Backend for Core<'_> {
    fn authorize(&mut self, _: &str) -> Result<(), BackendError> {
        self.token().map(|_| ())
    }
    fn line(&mut self, _: &str) -> Result<Line, BackendError> {
        let token = self.token()?;
        let view = lock_auth(self.auth)?.line_status(&token).map_err(storage)?;
        let system = crate::system_status::snapshot();
        let text = |name: &str| system[name].as_str().unwrap_or("").to_owned();
        Ok(Line {
            device_name: "RustSimHub".into(),
            manufacturer: "RustSimHub".into(),
            model: text("model"),
            firmware: text("firmware_version"),
            registered: view.status == LineStatus::Ready,
            searching: view.status == LineStatus::Observing,
            ..Line::default()
        })
    }
    fn calls(
        &mut self,
        pairing_id: &str,
        history: usize,
    ) -> Result<(Vec<Call>, Vec<Call>), BackendError> {
        self.calls_for(pairing_id, history, |_| true)
    }
    fn observe_calls(
        &mut self,
        pairing_id: &str,
        owned_call: Option<&str>,
    ) -> Result<Vec<Call>, BackendError> {
        self.calls_for(pairing_id, 0, |lease| {
            owned_call == Some(lease.call_id.as_str())
        })
        .map(|(live, _)| live)
    }
    fn observe_history(
        &mut self,
        pairing_id: &str,
        history: usize,
    ) -> Result<(Vec<Call>, Vec<Call>), BackendError> {
        self.calls_for(pairing_id, history, |_| false)
    }
    fn dial(
        &mut self,
        pairing_id: &str,
        number: &str,
        request_id: &str,
    ) -> Result<Call, BackendError> {
        let number =
            dial_number(number).ok_or(BackendError::Message(400, "number_required".into()))?;
        let (id, lease) = self.act(
            rustsimhub_domain::CallAction::Dial,
            None,
            number.clone(),
            None,
            request_id,
        )?;
        let now = now_ms();
        // Without the lease the phone could not hang this call up: report it honestly.
        let lease = lease.ok_or(BackendError::Message(
            502,
            "已拨出但控制权未确认，请在设备页面核对".into(),
        ))?;
        self.book.lease = Some((pairing_id.to_owned(), lease, now));
        if !self.book.seen.iter().any(|c| c.id == id) {
            self.book.seen.push(Seen {
                id: id.clone(),
                incoming: false,
                number: number.clone(),
                phase: Phase::Dialing,
                created_ms: now,
                updated_ms: now,
                ended_ms: None,
                answered: false,
            });
        }
        Ok(Call {
            id,
            incoming: false,
            phase: Phase::Dialing,
            number,
            created_ms: now,
            updated_ms: now,
            ended_ms: None,
            missed: false,
        })
    }
    fn call_action(
        &mut self,
        pairing_id: &str,
        call_id: &str,
        action: CallAction,
        request_id: &str,
    ) -> Result<(), BackendError> {
        use rustsimhub_domain::CallAction as Core;
        // Hanging up and tones need the lease this same phone holds for this call.
        let held = || -> Result<CallLease, BackendError> {
            match &self.book.lease {
                Some((owner, lease, _)) if lease.call_id == call_id && owner == pairing_id => {
                    Ok(lease.clone())
                }
                Some((_, lease, _)) if lease.call_id == call_id => {
                    Err(BackendError::Message(409, "通话由另一台手机控制".into()))
                }
                _ => Err(BackendError::Message(409, "这通电话不由本机控制".into())),
            }
        };
        let (core, payload, lease) = match action {
            CallAction::Answer => (Core::Accept, String::new(), None),
            // A ringing call nobody owns yet is refused without a lease.
            CallAction::Reject => (Core::Hangup, String::new(), None),
            CallAction::Hangup => (Core::Hangup, String::new(), Some(held()?)),
            CallAction::Dtmf(digit) => (Core::Dtmf, digit.to_string(), Some(held()?)),
        };
        let (id, lease) = self.act(core, Some(call_id.to_owned()), payload, lease, request_id)?;
        match action {
            CallAction::Answer => {
                let lease = lease.ok_or(BackendError::Message(
                    502,
                    "已接听但控制权未确认，请在设备页面核对".into(),
                ))?;
                self.book.lease = Some((pairing_id.to_owned(), lease, now_ms()));
            }
            CallAction::Hangup | CallAction::Reject => {
                if self
                    .book
                    .lease
                    .as_ref()
                    .is_some_and(|(_, l, _)| l.call_id == id)
                {
                    self.book.lease = None;
                }
            }
            CallAction::Dtmf(_) => {}
        }
        Ok(())
    }
    fn messages(
        &mut self,
        pairing_id: &str,
        _: i64,
        limit: usize,
    ) -> Result<Vec<Message>, BackendError> {
        // Without a recorded starting point nothing older is ever exposed.
        let Some(cursor) = self.baselines.get(pairing_id).cloned() else {
            return Ok(Vec::new());
        };
        let token = self.token()?;
        let mut auth = lock_auth(self.auth)?;
        let page = auth
            .inbox(&token, Some(cursor), limit.clamp(1, 32))
            .map_err(storage)?;
        let mut messages = Vec::new();
        for summary in page.messages {
            if summary.binary {
                continue;
            }
            let detail = auth.sms_detail(&token, summary.id).map_err(storage)?;
            messages.push(Message {
                delivery_id: detail.summary.id,
                sender: detail.sender,
                content: detail.text,
                outgoing: false,
                timestamp_ms: sms_time_ms(&detail.timestamp).unwrap_or_else(now_ms),
            });
        }
        messages.reverse();
        Ok(messages)
    }
    fn send_message(
        &mut self,
        _: &str,
        phone: &str,
        text: &str,
        request_id: &str,
    ) -> Result<(), BackendError> {
        let sender = self.sender.ok_or(BackendError::Unavailable)?;
        if !(3..=32).contains(&phone.len()) {
            return Err(BackendError::Message(400, "invalid_message".into()));
        }
        let token = self.token()?;
        let line = lock_auth(self.auth)?.line_status(&token).map_err(storage)?;
        let failed = |reason: &'static str| BackendError::Message(502, reason.into());
        let prepared = self
            .runtime
            .block_on(sender.request(AuthCommand::PrepareSms {
                token: token.to_string(),
                line_epoch: line.line.0,
                idempotency_key: request_id.to_owned(),
                recipient: phone.to_owned(),
                text: text.to_owned(),
            }))
            .map_err(failed)?;
        let operation_id = prepared["id"]
            .as_str()
            .ok_or(BackendError::Other)?
            .to_owned();
        let submitted = self
            .runtime
            .block_on(sender.request(AuthCommand::SubmitSms {
                token: token.to_string(),
                operation_id,
            }))
            .map_err(failed)?;
        match submitted["state"].as_str() {
            Some("submitted") => Ok(()),
            // The modem may or may not have sent it: say so, never resend here.
            Some("unknown") => Err(BackendError::Message(
                502,
                "结果未确认，请勿重复发送".into(),
            )),
            _ => Err(BackendError::Message(502, "短信未发送".into())),
        }
    }
}

impl Controller {
    /// Loads the private state. A missing file is a fresh, disabled gateway;
    /// a damaged one is an error and is never overwritten.
    pub(crate) fn open(
        state: &Path,
        sender: Option<crate::sender::Controller>,
        calls: Option<Arc<dyn CallPort>>,
    ) -> Result<Self, Error> {
        let path = state.join(FILE);
        let stored = match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Stored {
                format: 1,
                enabled: false,
                pcm_rate: 8_000,
                seed: None,
                gateway: State::new().map_err(|_| Error::Entropy)?,
                baselines: HashMap::new(),
            },
            Ok(meta) if meta.is_file() && meta.len() <= MAX_FILE => {
                serde_json::from_slice(&std::fs::read(&path).map_err(|_| Error::Io)?)
                    .map_err(|_| Error::Storage)?
            }
            _ => return Err(Error::Storage),
        };
        let seed = match &stored.seed {
            None => None,
            Some(text) => {
                let bytes = Zeroizing::new(
                    rustsimhub_airsim::decode_base64url(text).map_err(|_| Error::Storage)?,
                );
                Some(Zeroizing::new(
                    <[u8; 32]>::try_from(&bytes[..]).map_err(|_| Error::Storage)?,
                ))
            }
        };
        if stored.format != 1
            || !matches!(stored.pcm_rate, 8_000 | 16_000)
            || (stored.enabled && seed.is_none())
        {
            return Err(Error::Storage);
        }
        let mut gateway =
            Gateway::new(stored.gateway, CONTROL_PORT, PCM_PORT).map_err(|_| Error::Storage)?;
        gateway.set_enabled(stored.enabled);
        Ok(Self {
            inner: Arc::new(Mutex::new(Inner {
                pcm: None,
                call_book: CallBook::default(),
                gateway,
                enabled: stored.enabled,
                pcm_rate: stored.pcm_rate,
                seed,
                baselines: stored.baselines,
                session: None,
                polls: HashMap::new(),
            })),
            path,
            sender,
            calls,
        })
    }

    pub(crate) async fn request(
        &self,
        auth: Arc<Mutex<Auth>>,
        command: AuthCommand,
    ) -> Result<Value, Error> {
        let (this, runtime) = (self.clone(), tokio::runtime::Handle::current());
        tokio::task::spawn_blocking(move || this.process(&auth, &runtime, command))
            .await
            .map_err(|_| Error::Busy)?
    }

    fn save(&self, inner: &mut Inner, force: bool) -> Result<(), Error> {
        let state = match inner.gateway.take_dirty() {
            Some(state) => serde_json::to_value(state).map_err(|_| Error::Storage)?,
            None if !force => return Ok(()),
            None => return self.save_all(inner),
        };
        let stored = json!({
            "format": 1, "enabled": inner.enabled, "pcm_rate": inner.pcm_rate,
            "seed": inner.seed.as_ref().map(|s| rustsimhub_airsim::base64url(&s[..])),
            "gateway": state, "baselines": inner.baselines,
        });
        let bytes = Zeroizing::new(serde_json::to_vec(&stored).map_err(|_| Error::Storage)?);
        write_private(&self.path, &bytes).map_err(|_| Error::Io)
    }
    fn save_all(&self, inner: &mut Inner) -> Result<(), Error> {
        // Nothing in the gateway changed; rewrite with its current state.
        inner.gateway.mark_dirty();
        self.save(inner, false)
    }

    fn view(inner: &mut Inner, calls_supported: bool) -> Value {
        let now = now_ms().div_euclid(1000);
        json!({
            "enabled": inner.enabled, "pcm_rate": inner.pcm_rate,
            "pairing_port": PAIRING_PORT, "control_port": CONTROL_PORT, "pcm_port": PCM_PORT,
            "window_seconds": inner.gateway.window_remaining(now),
            "pairings": inner.gateway.pairings(),
            "calls_supported": calls_supported, "audio_supported": calls_supported,
        })
    }

    fn process(
        &self,
        auth: &Arc<Mutex<Auth>>,
        runtime: &tokio::runtime::Handle,
        command: AuthCommand,
    ) -> Result<Value, Error> {
        let mut guard = self.inner.lock().map_err(|_| Error::Busy)?;
        let inner = &mut *guard;
        let owner = |token: &str, recent: bool| -> Result<(), Error> {
            let mut auth = auth.try_lock().map_err(|_| Error::Busy)?;
            if recent {
                auth.recent_owner(token).map(|_| ())
            } else {
                match auth.authorize(token, Scope::ClientsManage)?.kind {
                    Kind::Owner => Ok(()),
                    _ => Err(Error::Forbidden),
                }
            }
        };
        match command {
            command @ (AuthCommand::AirsimPcmOpen { .. } | AuthCommand::AirsimPcmPulse { .. }) => {
                pcm::process(self, inner, auth, runtime, command)
            }
            AuthCommand::AirsimStatus { token } => {
                owner(&token, false)?;
                Ok(Self::view(inner, self.calls.is_some()))
            }
            AuthCommand::AirsimConfigure {
                token,
                enabled,
                pcm_rate,
            } => {
                owner(&token, true)?;
                if !matches!(pcm_rate, 8_000 | 16_000) {
                    return Err(Error::InvalidInput);
                }
                {
                    let mut auth = auth.try_lock().map_err(|_| Error::Busy)?;
                    if enabled {
                        let seed = match inner.seed.take() {
                            Some(seed) => seed,
                            None => Zeroizing::new(random::<32>()?),
                        };
                        if auth
                            .current_principal(Auth::gateway_public_key(&seed))
                            .is_err()
                        {
                            auth.approve_gateway_client(&token, &seed, &SCOPES)?;
                        }
                        inner.seed = Some(seed);
                    } else if let Some(seed) = &inner.seed {
                        // Disabling withdraws the gateway's authority, not only a flag.
                        match auth.revoke(&token, Auth::gateway_public_key(seed)) {
                            Ok(())
                            | Err(Error::Forbidden | Error::InvalidInput | Error::Storage) => {}
                            Err(error) => return Err(error),
                        }
                        inner.session = None;
                        inner.polls.clear();
                    }
                }
                inner.enabled = enabled;
                inner.pcm = None;
                inner.pcm_rate = pcm_rate;
                inner.gateway.set_enabled(enabled);
                self.save(inner, true)?;
                Ok(Self::view(inner, self.calls.is_some()))
            }
            AuthCommand::AirsimOpenPairing { token } => {
                owner(&token, true)?;
                let (code, expires_at) = inner
                    .gateway
                    .open_window(now_ms().div_euclid(1000))
                    .map_err(|_| Error::Forbidden)?;
                Ok(json!({"code": code, "expires_at": expires_at, "pairing_port": PAIRING_PORT}))
            }
            AuthCommand::AirsimClosePairing { token } => {
                owner(&token, false)?;
                inner.gateway.close_window();
                Ok(Self::view(inner, self.calls.is_some()))
            }
            AuthCommand::AirsimRevoke { token, pairing_id } => {
                owner(&token, true)?;
                let removed = inner.gateway.revoke(&pairing_id);
                inner.baselines.remove(&pairing_id);
                inner
                    .polls
                    .retain(|_, (poll, _)| poll.pairing_id != pairing_id);
                self.save(inner, true)?;
                Ok(json!({"revoked": removed}))
            }
            AuthCommand::AirsimPairing {
                method,
                target,
                body,
                peer,
            } => {
                let peer: IpAddr = peer.parse().map_err(|_| Error::InvalidInput)?;
                let known: Vec<String> =
                    inner.gateway.pairings().into_iter().map(|p| p.id).collect();
                let response = inner.gateway.pairing(
                    &Request {
                        method: &method,
                        target: &target,
                        version: None,
                        timestamp: None,
                        nonce: None,
                        signature: None,
                        body: body.as_bytes(),
                        peer,
                    },
                    now_ms(),
                );
                let added: Vec<String> = inner
                    .gateway
                    .pairings()
                    .into_iter()
                    .map(|p| p.id)
                    .filter(|id| !known.contains(id))
                    .collect();
                for id in added {
                    // Messages that arrived before pairing are never shown to the phone.
                    // Without a starting point the pairing shows none at all.
                    let mut core = Core {
                        auth,
                        seed: inner.seed.as_ref(),
                        session: &mut inner.session,
                        baselines: &inner.baselines,
                        sender: None,
                        calls: None,
                        book: &mut inner.call_book,
                        runtime,
                    };
                    let cursor = core.token().ok().and_then(|token| {
                        lock_auth(auth)
                            .ok()?
                            .inbox_recent(&token, None, 1)
                            .ok()
                            .map(|page| match page.messages.is_empty() {
                                // Empty inbox: start from the beginning of this database.
                                true => InboxCursor {
                                    db_epoch: page.cursor.db_epoch,
                                    seq: "0".into(),
                                },
                                false => page.cursor,
                            })
                    });
                    if let Some(cursor) = cursor {
                        inner.baselines.insert(id, cursor);
                    }
                }
                self.save(inner, false)?;
                Ok(json!({"status": response.status, "body": response.body}))
            }
            AuthCommand::AirsimControl {
                method,
                target,
                version,
                timestamp,
                nonce,
                signature,
                body,
                peer,
            } => {
                let peer: IpAddr = peer.parse().map_err(|_| Error::InvalidInput)?;
                let now = now_ms();
                let Inner {
                    gateway,
                    seed,
                    session,
                    baselines,
                    polls,
                    call_book,
                    ..
                } = inner;
                let mut core = Core {
                    auth,
                    seed: seed.as_ref(),
                    session,
                    baselines,
                    sender: self.sender.as_ref(),
                    calls: self.calls.as_deref(),
                    book: call_book,
                    runtime,
                };
                let outcome = gateway.control(
                    &Request {
                        method: &method,
                        target: &target,
                        version: version.as_deref(),
                        timestamp: timestamp.as_deref(),
                        nonce: nonce.as_deref(),
                        signature: signature.as_deref(),
                        body: body.as_bytes(),
                        peer,
                    },
                    now,
                    &mut core,
                );
                let reply = match outcome {
                    Outcome::Done(response) => {
                        json!({"status": response.status, "body": response.body})
                    }
                    Outcome::Wait(poll) => {
                        polls.retain(|_, (_, deadline)| *deadline + 5_000 > now);
                        if polls.len() >= MAX_POLLS {
                            json!({"status": 503, "body": {"error": "busy"}})
                        } else {
                            let ticket = hex(&random::<16>()?);
                            let deadline = now + poll.timeout_ms as i64;
                            polls.insert(ticket.clone(), (poll, deadline));
                            json!({"wait": true, "ticket": ticket, "retry_ms": EVENT_POLL_PERIOD_MS})
                        }
                    }
                };
                self.save(inner, false)?;
                Ok(reply)
            }
            AuthCommand::AirsimPoll { ticket } => {
                let now = now_ms();
                let Inner {
                    gateway,
                    seed,
                    session,
                    baselines,
                    polls,
                    call_book,
                    ..
                } = inner;
                let Some((poll, deadline)) = polls.get(&ticket).cloned() else {
                    return Ok(json!({"status": 410, "body": {"error": "poll_expired"}}));
                };
                let mut core = Core {
                    auth,
                    seed: seed.as_ref(),
                    session,
                    baselines,
                    sender: None,
                    calls: self.calls.as_deref(),
                    book: call_book,
                    runtime,
                };
                let reply = match gateway.poll(&poll, now, &mut core, now >= deadline) {
                    Some(response) => {
                        polls.remove(&ticket);
                        json!({"status": response.status, "body": response.body})
                    }
                    None => {
                        json!({"wait": true, "ticket": ticket, "retry_ms": EVENT_POLL_PERIOD_MS})
                    }
                };
                self.save(inner, false)?;
                Ok(reply)
            }
            _ => Err(Error::Forbidden),
        }
    }
}

#[cfg(test)]
#[path = "airsim_tests.rs"]
mod tests;
