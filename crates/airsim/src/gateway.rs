// SPDX-License-Identifier: AGPL-3.0-only
//! Transport-agnostic AirSIM gateway: pairing window, request authentication
//! and the control routes the app's Android gateway forwards. The caller owns
//! the sockets, the clock, persistence of [`State`] and the real business
//! behind [`Backend`]; nothing here opens a listener or touches a modem.
//! Ported from NasSimHub `internal/airsimgw` (control.go, pairing.go, store.go).
use crate::{
    base64url, canonical, decode_base64url, dial_proof, direct_route_proof, fresh_timestamp,
    health_proof, hex, valid_nonce, verify_signature, Error, PairingSession, ReplayCache,
    MIN_SECRET_BYTES,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::IpAddr;
use zeroize::Zeroizing;

pub const MAX_PAIRINGS: usize = 16;
pub const MAX_ACKED_MESSAGES: usize = 512;
pub const MAX_CONTROL_BODY: usize = 64 << 10;
pub const MAX_LONG_POLL_MS: u64 = 30_000;
pub const EVENT_POLL_PERIOD_MS: u64 = 400;
const HISTORY_LIMIT: usize = 20;
const MESSAGE_LIMIT: usize = 50;
const MESSAGE_LOOKBACK_MS: i64 = 7 * 24 * 3600 * 1000;
const GATEWAY_VERSION: &str = "rustsimhub-airsim/1";
const AGENT_KIND: &str = "rustsimhub";

// The routes the AirSIM Android gateway forwards (VoWLANControlPolicy at
// 93db925). Everything else is 404 before authentication, exactly as there;
// that includes routes newer app builds probe and accept a 404 for.
const GET_ROUTES: &[&str] = &[
    "/v1/health",
    "/api/health",
    "/api/status",
    "/api/debug",
    "/api/events",
    "/api/calls/status",
    "/api/calls/events",
    "/api/calls/audio/host/config",
    "/api/push/status",
    "/api/sms",
    "/api/sms/status",
];
const POST_ROUTES: &[&str] = &[
    "/api/calls/dial",
    "/api/calls/answer",
    "/api/calls/reject",
    "/api/calls/hangup",
    "/api/calls/dtmf",
    "/api/calls/audio/mute",
    "/api/calls/audio/host/warmup",
    "/api/calls/audio/host/register",
    "/api/push/register",
    "/api/sms/send",
    "/api/sms/ack",
    "/api/sms/refresh",
];

/// Only private networks may reach the gateway: this is the app's local-mode
/// protocol and has no TLS.
pub fn private_peer(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || (o[0] == 100 && (o[1] & 0xc0) == 64)
        }
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => private_peer(IpAddr::V4(v4)),
            None => {
                v6.is_loopback()
                    || (v6.segments()[0] & 0xfe00) == 0xfc00
                    || (v6.segments()[0] & 0xffc0) == 0xfe80
            }
        },
    }
}

/// What the app shows as "the phone" it is paired with.
#[derive(Debug, Clone, Default)]
pub struct Line {
    pub device_name: String,
    pub manufacturer: String,
    pub model: String,
    pub firmware: String,
    pub phone_number: String,
    pub operator: String,
    pub imei: String,
    pub iccid: String,
    pub imsi: String,
    pub signal_dbm: Option<i32>,
    pub registered: bool,
    pub searching: bool,
    pub network_mode: String,
    pub band: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Ringing,
    Dialing,
    Connecting,
    Active,
    Ended,
}
impl Phase {
    fn live(self) -> bool {
        self != Self::Ended
    }
}

/// Times are Unix milliseconds.
#[derive(Debug, Clone)]
pub struct Call {
    pub id: String,
    pub incoming: bool,
    pub phase: Phase,
    pub number: String,
    pub created_ms: i64,
    pub updated_ms: i64,
    pub ended_ms: Option<i64>,
    pub missed: bool,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub delivery_id: String,
    pub sender: String,
    pub content: String,
    pub outgoing: bool,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallAction {
    Answer,
    Reject,
    Hangup,
    Dtmf(char),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendError {
    /// The pairing's authority is gone: the pairing is removed.
    Forbidden,
    NotFound,
    Conflict,
    Unavailable,
    /// A message the app may show (for example "line in use").
    Message(u16, String),
    Other,
}

/// The stick's business as seen by one authenticated pairing. Implemented by
/// Core, which runs each command through its ordinary authorization.
pub trait Backend {
    fn authorize(&mut self, pairing_id: &str) -> Result<(), BackendError>;
    fn line(&mut self, pairing_id: &str) -> Result<Line, BackendError>;
    /// Live calls, then up to `history` most recent ended calls.
    fn calls(
        &mut self,
        pairing_id: &str,
        history: usize,
    ) -> Result<(Vec<Call>, Vec<Call>), BackendError>;
    /// Observe without extending an unattended call. Only the exact call
    /// renewed by this connection's owner may be named.
    fn observe_calls(
        &mut self,
        pairing_id: &str,
        _owned_call: Option<&str>,
    ) -> Result<Vec<Call>, BackendError> {
        self.calls(pairing_id, 0).map(|(live, _)| live)
    }
    fn dial(
        &mut self,
        pairing_id: &str,
        number: &str,
        request_id: &str,
    ) -> Result<Call, BackendError>;
    fn call_action(
        &mut self,
        pairing_id: &str,
        call_id: &str,
        action: CallAction,
        request_id: &str,
    ) -> Result<(), BackendError>;
    /// Received messages newer than `since_ms`, newest first, at most `limit`.
    fn messages(
        &mut self,
        pairing_id: &str,
        since_ms: i64,
        limit: usize,
    ) -> Result<Vec<Message>, BackendError>;
    fn send_message(
        &mut self,
        pairing_id: &str,
        phone: &str,
        text: &str,
        request_id: &str,
    ) -> Result<(), BackendError>;
}

/// One paired iPhone. `secret` is the app's VoWLAN secret (base64url in the
/// persisted form); it is never included in [`PairingView`].
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pairing {
    pub id: String,
    secret: String,
    pub device_hint: String,
    pub created_ms: i64,
    pub last_seen_ms: i64,
    pub last_peer: String,
    acked: Vec<String>,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PairingView {
    pub id: String,
    pub device_hint: String,
    pub created_ms: i64,
    pub last_seen_ms: i64,
    pub last_peer: String,
}
impl Pairing {
    fn secret(&self) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(decode_base64url(&self.secret).unwrap_or_default())
    }
    pub fn view(&self) -> PairingView {
        PairingView {
            id: self.id.clone(),
            device_hint: self.device_hint.clone(),
            created_ms: self.created_ms,
            last_seen_ms: self.last_seen_ms,
            last_peer: self.last_peer.clone(),
        }
    }
}

/// Everything that must survive a restart. Contains secrets: the caller
/// stores it only where Core keeps its other credentials.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub format: u32,
    pub agent_id: String,
    pub pairings: Vec<Pairing>,
}
impl State {
    pub fn new() -> Result<Self, Error> {
        let mut raw = [0u8; 8];
        getrandom::fill(&mut raw).map_err(|_| Error::Random)?;
        Ok(Self {
            format: 1,
            agent_id: format!("rustsimhub-{}", hex(&raw)),
            pairings: Vec::new(),
        })
    }
    pub fn validate(&self) -> Result<(), Error> {
        let ok = self.format == 1
            && (1..=128).contains(&self.agent_id.len())
            && self
                .agent_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
            && self.pairings.len() <= MAX_PAIRINGS
            && self.pairings.iter().all(|p| {
                p.id.len() == 24
                    && p.id.bytes().all(|b| b.is_ascii_hexdigit())
                    && p.secret().len() >= MIN_SECRET_BYTES
                    && p.acked.len() <= MAX_ACKED_MESSAGES
            });
        if ok {
            Ok(())
        } else {
            Err(Error::Invalid)
        }
    }
}

/// A plain HTTP request as received on the pairing or control port.
pub struct Request<'a> {
    pub method: &'a str,
    /// The request target exactly as sent (`/path?query`): signatures cover it.
    pub target: &'a str,
    pub version: Option<&'a str>,
    pub timestamp: Option<&'a str>,
    pub nonce: Option<&'a str>,
    pub signature: Option<&'a str>,
    pub body: &'a [u8],
    pub peer: IpAddr,
}
impl Request<'_> {
    fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or("")
    }
    fn query(&self, name: &str) -> Option<&str> {
        self.target.split_once('?')?.1.split('&').find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            (key == name).then_some(value)
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    pub status: u16,
    pub body: Value,
}
fn reply(status: u16, body: Value) -> Response {
    Response { status, body }
}
fn fail(status: u16, code: &str) -> Response {
    reply(status, json!({ "error": code }))
}

/// A control request's outcome. A long poll that has nothing new yet is
/// handed back so the transport can wait and ask again with [`Gateway::poll`].
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Done(Response),
    Wait(Poll),
}
#[derive(Debug, Clone, PartialEq)]
pub struct Poll {
    pub pairing_id: String,
    pub after: u64,
    pub calls_only: bool,
    /// How long the app asked to wait, already capped.
    pub timeout_ms: u64,
    peer: String,
}

#[derive(Default)]
struct Watch {
    call_print: String,
    sms_print: String,
    revision: u64,
    sms_revision: u64,
}

pub struct Gateway {
    state: State,
    dirty: bool,
    enabled: bool,
    control_port: u16,
    pcm_port: u16,
    window: Option<PairingSession>,
    replay: ReplayCache,
    watches: HashMap<String, Watch>,
    muted: HashMap<String, bool>,
}

fn random_hex(bytes: usize) -> Result<String, Error> {
    let mut raw = vec![0u8; bytes];
    getrandom::fill(&mut raw).map_err(|_| Error::Random)?;
    Ok(hex(&raw))
}

fn request_id(prefix: &str, operation_id: &str) -> String {
    if !operation_id.is_empty() && operation_id.len() <= 96 {
        return format!(
            "{prefix}{}",
            hex(&Sha256::digest(operation_id.as_bytes())[..12])
        );
    }
    format!("{prefix}{}", random_hex(12).unwrap_or_default())
}

fn fingerprint(value: &Value) -> String {
    hex(&Sha256::digest(value.to_string().as_bytes())[..16])
}

/// `2006-01-02T15:04:05.000Z` in UTC from Unix milliseconds.
pub(crate) fn iso(ms: i64) -> String {
    let ms = ms.max(0);
    let (days, rest) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    // Civil-from-days (Howard Hinnant), valid for the whole positive range.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3_600_000,
        rest / 60_000 % 60,
        rest / 1000 % 60,
        rest % 1000
    )
}

fn airsim_state(call: &Call) -> &'static str {
    match call.phase {
        Phase::Ringing => "incoming",
        Phase::Dialing => "dialing",
        Phase::Connecting if call.incoming => "active",
        Phase::Connecting => "alerting",
        Phase::Active => "active",
        Phase::Ended => "ended",
    }
}

fn record(call: &Call, index: usize) -> Value {
    let state = airsim_state(call);
    let mut value = json!({
        "id": call.id, "index": index,
        "direction": if call.incoming { "incoming" } else { "outgoing" },
        "state": state, "number": call.number,
        "started_at": iso(call.created_ms), "updated_at": iso(call.updated_ms),
        "missed": call.missed, "android_id": call.id,
    });
    if state == "ended" {
        value["ended_at"] = iso(call.ended_ms.unwrap_or(call.updated_ms)).into();
    }
    value
}

fn cellular_state(line: &Line) -> &'static str {
    if line.registered {
        "registered"
    } else if line.searching {
        "searching"
    } else {
        "unregistered"
    }
}

/// The SMS segment count the app displays after sending.
pub fn sms_segments(text: &str) -> usize {
    let (length, single, multi) = if text.is_ascii() {
        (text.len(), 160, 153)
    } else {
        (text.chars().count(), 70, 67)
    };
    if length <= single {
        1
    } else {
        length.div_ceil(multi)
    }
}

/// A 4 to 8 digit code from a message that says it carries one.
pub fn verification_code(content: &str) -> Option<&str> {
    let lower = content.to_lowercase();
    if !["验证码", "校验码", "動態碼"]
        .iter()
        .any(|k| content.contains(k))
        && !lower.contains("code")
    {
        return None;
    }
    let mut start = None;
    for (index, character) in content.char_indices().chain([(content.len(), ' ')]) {
        match (character.is_ascii_digit(), start) {
            (true, None) => start = Some(index),
            (true, Some(_)) => {}
            (false, Some(begin)) => {
                if (4..=8).contains(&(index - begin)) {
                    return Some(&content[begin..index]);
                }
                start = None;
            }
            (false, None) => {}
        }
    }
    None
}

fn device_hint(device_id: &str) -> String {
    let id = device_id.trim();
    match id.len().checked_sub(4).and_then(|at| id.get(at..)) {
        Some(hint) if hint.bytes().all(|b| b.is_ascii_graphic()) => hint.to_owned(),
        _ => String::new(),
    }
}

fn backend_error(error: BackendError) -> Response {
    match error {
        BackendError::Forbidden => fail(403, "forbidden"),
        BackendError::NotFound => fail(404, "call_not_found"),
        BackendError::Conflict => fail(409, "conflict"),
        BackendError::Unavailable => fail(503, "unavailable"),
        BackendError::Message(status, message) => fail(status, &message),
        BackendError::Other => fail(502, "agent_unavailable"),
    }
}

macro_rules! backend {
    ($call:expr) => {
        match $call {
            Ok(value) => value,
            Err(error) => return Outcome::Done(backend_error(error)),
        }
    };
}

impl Gateway {
    pub fn new(state: State, control_port: u16, pcm_port: u16) -> Result<Self, Error> {
        state.validate()?;
        Ok(Self {
            state,
            dirty: false,
            enabled: false,
            control_port,
            pcm_port,
            window: None,
            replay: ReplayCache::default(),
            watches: HashMap::new(),
            muted: HashMap::new(),
        })
    }
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.window = None;
        }
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn agent_id(&self) -> &str {
        &self.state.agent_id
    }
    pub fn pairings(&self) -> Vec<PairingView> {
        self.state.pairings.iter().map(Pairing::view).collect()
    }
    /// The state to persist, when it changed since the last call.
    pub fn take_dirty(&mut self) -> Option<&State> {
        std::mem::take(&mut self.dirty).then_some(&self.state)
    }
    /// Forces the next [`Gateway::take_dirty`] to return the state, for a
    /// caller that stores its own settings in the same file.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
    pub fn muted(&self, pairing_id: &str) -> bool {
        self.muted.get(pairing_id).copied().unwrap_or(false)
    }

    /// Opens a fresh pairing window and returns `(code, expires_at)` for the
    /// owner's page. Any earlier window is replaced.
    pub fn open_window(&mut self, now: i64) -> Result<(String, i64), Error> {
        if !self.enabled {
            return Err(Error::Invalid);
        }
        let session = PairingSession::new(now)?;
        let shown = (session.code().to_owned(), session.expires_at());
        self.window = Some(session);
        Ok(shown)
    }
    pub fn close_window(&mut self) {
        self.window = None;
    }
    /// Seconds left in the open window, if any.
    pub fn window_remaining(&mut self, now: i64) -> Option<i64> {
        if self.window.as_ref().is_some_and(|w| now > w.expires_at()) {
            self.window = None;
        }
        self.window.as_ref().map(|w| w.expires_at() - now)
    }
    /// Removes a pairing; its signatures stop verifying at once.
    pub fn revoke(&mut self, pairing_id: &str) -> bool {
        let before = self.state.pairings.len();
        self.state.pairings.retain(|p| p.id != pairing_id);
        self.watches.remove(pairing_id);
        self.muted.remove(pairing_id);
        let removed = self.state.pairings.len() != before;
        self.dirty |= removed;
        removed
    }

    /// The pairing port: `GET /pair/v1/info` and `POST /pair/v1/claim`.
    pub fn pairing(&mut self, request: &Request<'_>, now_ms: i64) -> Response {
        if !private_peer(request.peer) {
            return fail(403, "private_network_required");
        }
        let now = now_ms.div_euclid(1000);
        if !self.enabled || self.window_remaining(now).is_none() {
            // Also when no window was ever opened: the app then tells the user
            // to start pairing again, which is the fix.
            return fail(410, "pairing_expired");
        }
        match (request.method, request.path()) {
            ("GET", "/pair/v1/info") => {
                let window = self.window.as_ref().expect("checked above");
                reply(
                    200,
                    json!({
                        "product": "airsim-pair", "version": 1, "session_id": window.id(),
                        "server_public_key": base64url(&window.public_key()),
                        "expires_at": window.expires_at() * 1000, "server_time": now_ms,
                    }),
                )
            }
            ("POST", "/pair/v1/claim") => self.claim(request, now_ms),
            _ => fail(404, "not_found"),
        }
    }

    fn claim(&mut self, request: &Request<'_>, now_ms: i64) -> Response {
        #[derive(Deserialize)]
        struct Claim {
            version: i64,
            session_id: String,
            client_public_key: String,
            sealed_registration: String,
        }
        if request.body.len() > MAX_CONTROL_BODY {
            return fail(400, "bad_request");
        }
        let window = self.window.as_mut().expect("caller checked the window");
        let claim = match serde_json::from_slice::<Claim>(request.body) {
            Ok(claim) if claim.version == 1 && claim.session_id == window.id() => claim,
            _ => return fail(400, "invalid_session"),
        };
        let (Ok(client_key), Ok(sealed)) = (
            decode_base64url(&claim.client_public_key),
            decode_base64url(&claim.sealed_registration),
        ) else {
            return fail(401, "pairing_auth_failed");
        };
        let plaintext = match window.open(&client_key, &sealed, now_ms.div_euclid(1000)) {
            Ok(plaintext) => Zeroizing::new(plaintext),
            Err(Error::PairingExpired | Error::PairingUsed) => return fail(410, "pairing_expired"),
            Err(_) => return fail(401, "pairing_auth_failed"),
        };
        // Only the secret and a display hint are kept. The push tokens and
        // relay settings in the registration belong to AirSIM's cloud relay.
        let Ok(registration) = serde_json::from_slice::<Value>(&plaintext) else {
            return fail(400, "invalid_registration");
        };
        let secret = Zeroizing::new(
            registration["vowlan_secret"]
                .as_str()
                .and_then(|s| decode_base64url(s).ok())
                .unwrap_or_default(),
        );
        if secret.len() < MIN_SECRET_BYTES {
            return fail(400, "vowlan_secret_required");
        }
        if self.state.pairings.len() >= MAX_PAIRINGS {
            return fail(409, "too_many_paired_devices");
        }
        let Ok(id) = random_hex(12) else {
            return fail(500, "pairing_store_failed");
        };
        let hint = device_hint(registration["device_id"].as_str().unwrap_or(""));
        let session_id = window.id().to_owned();
        self.window = None;
        self.state.pairings.push(Pairing {
            id,
            secret: base64url(&secret),
            device_hint: hint.clone(),
            created_ms: now_ms,
            last_seen_ms: now_ms,
            last_peer: request.peer.to_string(),
            acked: Vec::new(),
        });
        self.dirty = true;
        let mut result = json!({
            "configured": true, "agent_id": self.state.agent_id,
            "control_port": self.control_port, "pcm_port": self.pcm_port,
            "route_proof": direct_route_proof(&secret, &session_id, &claim.client_public_key,
                &self.state.agent_id, self.control_port, self.pcm_port),
        });
        if !hint.is_empty() {
            result["device_id_hint"] = hint.into();
        }
        reply(200, result)
    }

    /// Finds the paired iPhone whose secret signed the request.
    fn authenticate(
        &mut self,
        request: &Request<'_>,
        now: i64,
    ) -> Option<(usize, Zeroizing<Vec<u8>>, String)> {
        if request.version != Some("1") {
            return None;
        }
        let raw = request.timestamp?;
        if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let timestamp: i64 = raw.parse().ok()?;
        let (nonce, signature) = (request.nonce?, request.signature?);
        if !fresh_timestamp(timestamp, now) || !valid_nonce(nonce) {
            return None;
        }
        let text = canonical(
            request.method,
            request.target,
            request.body,
            timestamp,
            nonce,
        );
        let index = self
            .state
            .pairings
            .iter()
            .position(|p| verify_signature(&p.secret(), signature, &text))?;
        // Only an authenticated request may spend a nonce slot.
        if !self.replay.accept(nonce, now) {
            return None;
        }
        Some((index, self.state.pairings[index].secret(), nonce.to_owned()))
    }

    /// Verifies a PCM preface line and returns the pairing it belongs to.
    pub fn authenticate_pcm(&mut self, line: &str, peer: IpAddr, now: i64) -> Option<String> {
        if !self.enabled || !private_peer(peer) {
            return None;
        }
        let preface = crate::parse_pcm_preface(line).ok()?;
        if !fresh_timestamp(preface.timestamp, now) {
            return None;
        }
        let text = crate::pcm_canonical(&preface);
        let id = self
            .state
            .pairings
            .iter()
            .find(|p| verify_signature(&p.secret(), &preface.signature, &text))?
            .id
            .clone();
        self.replay.accept(&preface.nonce, now).then_some(id)
    }

    fn touch(&mut self, index: usize, peer: &str, now_ms: i64) {
        let pairing = &mut self.state.pairings[index];
        // Persist at most once a minute, or when the address changes.
        if pairing.last_peer != peer || now_ms - pairing.last_seen_ms >= 60_000 {
            pairing.last_peer = peer.to_owned();
            pairing.last_seen_ms = now_ms;
            self.dirty = true;
        }
    }

    /// The control port.
    pub fn control(
        &mut self,
        request: &Request<'_>,
        now_ms: i64,
        backend: &mut dyn Backend,
    ) -> Outcome {
        let done = Outcome::Done;
        if !private_peer(request.peer) {
            return done(fail(403, "private_peer_required"));
        }
        let path = request.path().to_owned();
        let known = match request.method {
            "GET" => GET_ROUTES.contains(&path.as_str()),
            "POST" => POST_ROUTES.contains(&path.as_str()),
            _ => false,
        };
        if !known {
            return done(fail(404, "not_found"));
        }
        if request.body.len() > MAX_CONTROL_BODY {
            return done(fail(400, "bad_request"));
        }
        if !self.enabled {
            return done(fail(503, "gateway_disabled"));
        }
        let Some((index, secret, nonce)) = self.authenticate(request, now_ms.div_euclid(1000))
        else {
            return done(fail(401, "authentication_failed"));
        };
        let peer = request.peer.to_string();
        self.touch(index, &peer, now_ms);
        let id = self.state.pairings[index].id.clone();
        match backend.authorize(&id) {
            Ok(()) => {}
            Err(BackendError::Forbidden) => {
                // The authority behind this pairing is gone: it is dead, not merely refused.
                self.revoke(&id);
                return done(fail(401, "authentication_failed"));
            }
            Err(BackendError::Message(_, message)) => return done(fail(503, &message)),
            Err(_) => return done(fail(503, "agent_unavailable")),
        }
        let body: Value = if request.body.is_empty() {
            Value::Null
        } else {
            match serde_json::from_slice(request.body) {
                Ok(value) => value,
                Err(_) => return done(fail(400, "bad_request")),
            }
        };
        let text = |name: &str| body[name].as_str().unwrap_or("").trim().to_owned();
        match path.as_str() {
            "/v1/health" => {
                let line = backend!(backend.line(&id));
                done(reply(
                    200,
                    json!({
                        "ok": true, "transport": "vowlan", "version": 1,
                        "agent_id": self.state.agent_id,
                        "identity_proof": health_proof(&secret, &nonce, &self.state.agent_id),
                        "canonical_call_ids": true, "agent_kind": AGENT_KIND,
                        "device_name": line.device_name, "manufacturer": line.manufacturer,
                        "model": line.model, "phone_number": line.phone_number,
                        "sim_slots": [{
                            "id": "rustsimhub-line",
                            "label": if line.operator.trim().is_empty() { "SIM" } else { &line.operator },
                            "number": line.phone_number, "default": true, "slot_index": 0,
                            "carrier_name": line.operator,
                            "is_default_voice": true, "is_default_sms": true,
                        }],
                    }),
                ))
            }
            "/api/health" => {
                let line = backend!(backend.line(&id));
                done(reply(
                    200,
                    json!({"ok": true, "version": GATEWAY_VERSION, "control_plane": "vowlan",
                        "cellular_state": cellular_state(&line)}),
                ))
            }
            "/api/status" => {
                let line = backend!(backend.line(&id));
                let mut status = json!({
                    "imei": line.imei, "firmware": line.firmware, "iccid": line.iccid,
                    "imsi": line.imsi, "operator": line.operator,
                    "sim_inserted": !line.iccid.is_empty() || !line.imsi.is_empty(),
                    "network_mode": line.network_mode, "radio_band": line.band,
                    "cellular_state": cellular_state(&line),
                });
                if let Some(dbm) = line.signal_dbm {
                    status["signal_dbm"] = dbm.into();
                }
                done(reply(200, status))
            }
            "/api/events" | "/api/calls/events" => {
                let after = request
                    .query("after")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                let timeout_ms = request
                    .query("timeout_ms")
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(15_000)
                    .min(MAX_LONG_POLL_MS);
                let poll = Poll {
                    pairing_id: id,
                    after,
                    calls_only: path == "/api/calls/events",
                    timeout_ms,
                    peer,
                };
                match self.poll(&poll, now_ms, backend, timeout_ms == 0) {
                    Some(response) => done(response),
                    None => Outcome::Wait(poll),
                }
            }
            "/api/calls/status" if request.query("observe") == Some("1") => {
                let own = request.query("own").filter(|id| !id.is_empty());
                let live = backend!(backend.observe_calls(&id, own));
                let active = live
                    .iter()
                    .find(|call| call.phase.live())
                    .map(|call| record(call, 1));
                done(reply(200, json!({"active": active})))
            }
            "/api/calls/status" => {
                let status = backend!(self.snapshot(&id, now_ms, backend));
                done(reply(200, status["call"].clone()))
            }
            "/api/calls/dial" => {
                let number = text("number");
                if number.is_empty() || number.len() > 64 {
                    return done(fail(400, "number_required"));
                }
                let operation = body["operation_id"].as_str().unwrap_or("");
                let created =
                    backend!(backend.dial(&id, &number, &request_id("airsim-dial-", operation)));
                let mut result = json!({"ok": true, "call_id": created.id});
                if !operation.is_empty() {
                    result["operation_id"] = operation.into();
                    result["android_id"] = created.id.clone().into();
                    result["backend_id"] = created.id.clone().into();
                    result["operation_proof"] =
                        dial_proof(&secret, &nonce, operation, &created.id, &created.id).into();
                }
                done(reply(200, result))
            }
            "/api/calls/answer" | "/api/calls/reject" | "/api/calls/hangup" | "/api/calls/dtmf" => {
                let verb = path.rsplit('/').next().unwrap_or("");
                let wanted = text("call_id");
                let (live, _) = backend!(backend.calls(&id, 0));
                // The call the app names, or else the line's one live call.
                let target = live
                    .iter()
                    .find(|c| c.phase.live() && (wanted.is_empty() || wanted == c.id));
                let Some(target) = target else {
                    return done(if verb == "hangup" || verb == "reject" {
                        // Already gone: hanging up is idempotent for the app.
                        reply(200, json!({"ok": true, "rejected": verb == "reject"}))
                    } else {
                        fail(404, "no_active_call")
                    });
                };
                let action = match verb {
                    "answer" => CallAction::Answer,
                    "reject" if target.phase == Phase::Ringing => CallAction::Reject,
                    "reject" | "hangup" => CallAction::Hangup,
                    _ => {
                        let digit = text("digit");
                        let mut characters = digit.chars();
                        match (characters.next(), characters.next()) {
                            (Some(d), None) if "0123456789*#ABCD".contains(d) => {
                                CallAction::Dtmf(d)
                            }
                            _ => return done(fail(400, "invalid_digit")),
                        }
                    }
                };
                backend!(backend.call_action(
                    &id,
                    &target.id,
                    action,
                    &request_id(&format!("airsim-{verb}-"), "")
                ));
                done(reply(
                    200,
                    if verb == "reject" {
                        json!({"rejected": true})
                    } else {
                        json!({"ok": true})
                    },
                ))
            }
            "/api/calls/audio/mute" => {
                let muted = body["muted"].as_bool().unwrap_or(false);
                self.muted.insert(id, muted);
                done(reply(200, json!({"muted": muted})))
            }
            "/api/calls/audio/host/warmup"
            | "/api/calls/audio/host/register"
            | "/api/sms/refresh" => done(reply(200, json!({"ok": true}))),
            "/api/calls/audio/host/config" => done(reply(
                200,
                json!({"vendor_id": 0, "product_id": 0, "location_id": 0,
                    "route_ready": true, "route_running": false}),
            )),
            "/api/push/status" => done(reply(
                200,
                json!({"configured": true, "call_push_ready": false, "message_push_ready": false,
                    "local_mode": true, "cloud_enabled": false, "relay_url": "", "last_error": ""}),
            )),
            // Push needs AirSIM's cloud relay. The local pairing keeps working
            // while the app is open or kept alive in the background.
            "/api/push/register" => done(reply(200, json!({"configured": true}))),
            "/api/sms" => {
                let pending = backend!(self.pending(&id, now_ms, backend));
                let list: Vec<Value> = pending
                    .iter()
                    .map(|m| {
                        let mut item = json!({
                            "sender": m.sender, "content": m.content,
                            "timestamp": iso(m.timestamp_ms), "delivery_id": m.delivery_id,
                            "direction": "incoming",
                        });
                        if let Some(code) = verification_code(&m.content) {
                            item["code"] = code.into();
                        }
                        item
                    })
                    .collect();
                done(reply(200, Value::Array(list)))
            }
            "/api/sms/status" => {
                let pending = backend!(self.pending(&id, now_ms, backend));
                done(reply(
                    200,
                    json!({"auto_cleanup_me": false, "count": pending.len(), "last_poll_error": ""}),
                ))
            }
            "/api/sms/ack" => {
                let ids: Vec<String> = body["ids"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str())
                            .filter(|s| !s.is_empty() && s.len() <= 128)
                            .take(MAX_ACKED_MESSAGES)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                let acked = &mut self.state.pairings[index].acked;
                for value in &ids {
                    if !acked.contains(value) {
                        acked.push(value.clone());
                    }
                }
                let extra = acked.len().saturating_sub(MAX_ACKED_MESSAGES);
                acked.drain(..extra);
                self.dirty = true;
                done(reply(200, json!({"acknowledged": ids.len()})))
            }
            "/api/sms/send" => {
                let phone = text("phone");
                let message = body["message"].as_str().unwrap_or("");
                if phone.is_empty()
                    || phone.len() > 64
                    || message.is_empty()
                    || message.chars().count() > 1600
                {
                    return done(fail(400, "invalid_message"));
                }
                backend!(backend.send_message(
                    &id,
                    &phone,
                    message,
                    &request_id("airsim-sms-", "")
                ));
                done(reply(
                    200,
                    json!({"sent": true, "segments": sms_segments(message)}),
                ))
            }
            // /api/debug: the stick has its own diagnostics page.
            _ => done(fail(404, "not_found")),
        }
    }

    fn pending(
        &mut self,
        pairing_id: &str,
        now_ms: i64,
        backend: &mut dyn Backend,
    ) -> Result<Vec<Message>, BackendError> {
        let pairing = self
            .state
            .pairings
            .iter()
            .find(|p| p.id == pairing_id)
            .ok_or(BackendError::Forbidden)?;
        // Only messages that arrived after pairing, and at most a week back.
        let since = pairing.created_ms.max(now_ms - MESSAGE_LOOKBACK_MS);
        let mut messages = backend.messages(pairing_id, since, MESSAGE_LIMIT)?;
        messages.retain(|m| {
            !m.outgoing && !m.delivery_id.is_empty() && !pairing.acked.contains(&m.delivery_id)
        });
        Ok(messages)
    }

    fn snapshot(
        &mut self,
        pairing_id: &str,
        now_ms: i64,
        backend: &mut dyn Backend,
    ) -> Result<Value, BackendError> {
        let (live, history) = backend.calls(pairing_id, HISTORY_LIMIT)?;
        let pending = self.pending(pairing_id, now_ms, backend)?;
        let active = live.iter().find(|c| c.phase.live()).map(|c| record(c, 1));
        let history: Vec<Value> = history
            .iter()
            .enumerate()
            .map(|(i, c)| record(c, i + 1))
            .collect();
        let call_print = fingerprint(&json!([active, history]));
        let sms_print = fingerprint(&json!(pending
            .iter()
            .map(|m| &m.delivery_id)
            .collect::<Vec<_>>()));
        let watch = self
            .watches
            .entry(pairing_id.to_owned())
            .or_insert_with(|| {
                // A restart must not replay a revision the app already saw; the app
                // waits for "different", not "greater".
                let mut raw = [0u8; 4];
                let _ = getrandom::fill(&mut raw);
                let start = (now_ms.div_euclid(1000) as u64) << 20
                    | u64::from(u32::from_be_bytes(raw) & 0xf_ffff);
                Watch {
                    revision: start,
                    sms_revision: start,
                    ..Watch::default()
                }
            });
        if watch.call_print != call_print {
            watch.call_print = call_print;
            watch.revision += 1;
        }
        if watch.sms_print != sms_print {
            watch.sms_print = sms_print;
            watch.sms_revision += 1;
            watch.revision += 1;
        }
        Ok(json!({
            "revision": watch.revision, "sms_revision": watch.sms_revision,
            "sms_pending": pending.len(), "sim_revision": 1,
            "call": {
                "active": active, "history": history, "polling": false, "event_driven": true,
                "poll_interval_s": 1, "last_poll_error": "", "revision": watch.revision,
            },
        }))
    }

    /// Re-evaluates a held long poll. Returns the response once something
    /// changed since `after`, or when `expired` says the wait is over; `None`
    /// means keep waiting and call again after [`EVENT_POLL_PERIOD_MS`].
    pub fn poll(
        &mut self,
        poll: &Poll,
        now_ms: i64,
        backend: &mut dyn Backend,
        expired: bool,
    ) -> Option<Response> {
        let Some(index) = self
            .state
            .pairings
            .iter()
            .position(|p| p.id == poll.pairing_id)
        else {
            // Revoked while waiting.
            return Some(fail(401, "authentication_failed"));
        };
        if !self.enabled {
            return Some(fail(503, "gateway_disabled"));
        }
        // A phone holding a long poll is still here.
        self.touch(index, &poll.peer, now_ms);
        let status = match self.snapshot(&poll.pairing_id, now_ms, backend) {
            Ok(status) => status,
            Err(error) => return Some(backend_error(error)),
        };
        let changed = poll.after == 0 || status["revision"].as_u64() != Some(poll.after);
        (changed || expired).then(|| {
            reply(
                200,
                if poll.calls_only {
                    status["call"].clone()
                } else {
                    status
                },
            )
        })
    }
}

#[cfg(test)]
#[path = "gateway_tests.rs"]
mod tests;
