// SPDX-License-Identifier: AGPL-3.0-only
//! Authenticated signaling. Core credentials and actual peer keys never cross HTTP.
use super::*;
use axum::extract::Extension;
use rustsimhub_domain::CallLease;
use rustsimhub_media::worker::{self as wire, Command, Phase, Snapshot};
use serde::Serialize;
use std::{
    net::IpAddr,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy)]
pub(super) struct Connection {
    pub peer: IpAddr,
    pub local: IpAddr,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestBody {
    protocol: u16,
    id: String,
    token: String,
    lease: Option<CallLease>,
    command: Action,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    Connect { session: String, offer: String },
    Prepare { session: String, offer: String },
    Poll { session: String },
    Activate { session: String },
    Stop { session: String },
}
impl Action {
    fn session(&self) -> &str {
        match self {
            Self::Connect { session, .. }
            | Self::Prepare { session, .. }
            | Self::Poll { session }
            | Self::Activate { session }
            | Self::Stop { session } => session,
        }
    }
}
struct Slot {
    session: String,
    binding: String,
    peer: IpAddr,
    local: IpAddr,
    offer_hash: [u8; 32],
    activation_started: bool,
    preflight_until: Option<Instant>,
}
pub(super) struct Bridge {
    path: PathBuf,
    uid: u32,
    sequence: AtomicU64,
    slot: tokio::sync::Mutex<Option<Slot>>,
    rate: Mutex<Rate>,
}
impl Bridge {
    pub fn new(path: PathBuf) -> Self {
        Self::with_uid(path, 984)
    }
    fn with_uid(path: PathBuf, uid: u32) -> Self {
        Self {
            path,
            uid,
            sequence: AtomicU64::new(1),
            slot: tokio::sync::Mutex::new(None),
            rate: Mutex::new(Rate {
                since: Instant::now(),
                remaining: 120,
            }),
        }
    }
    async fn raw(&self, command: Command) -> Result<wire::Reply> {
        let id = self.sequence.fetch_add(1, Ordering::Relaxed);
        if id == 0 {
            return Err("media sequence exhausted");
        }
        wire::exchange(
            &self.path,
            self.uid,
            wire::Request {
                protocol: wire::PROTOCOL,
                id,
                command,
            },
        )
        .await
        .map_err(|_| "media unavailable")
    }
    async fn exchange(&self, command: Command) -> Result<Snapshot> {
        self.raw(command).await?.result.map_err(|_| "media refused")
    }
    async fn stop(&self, session: &str) {
        let _ = self
            .exchange(Command::Stop {
                session: session.into(),
            })
            .await;
    }
    pub async fn takeover(&self) -> Result<()> {
        let receipt = self
            .exchange(Command::StopAll {
                session: "0".repeat(32),
            })
            .await?;
        if receipt.phase != Phase::Closed || receipt.pcm_opened {
            return Err("media takeover incomplete");
        }
        *self.slot.lock().await = None;
        Ok(())
    }
    pub async fn reap(&self) {
        let Ok(mut slot) = self.slot.try_lock() else {
            return;
        };
        if let Some(active) = slot.as_ref() {
            if active
                .preflight_until
                .is_some_and(|end| Instant::now() >= end)
            {
                self.stop(&active.session).await;
                *slot = None;
                return;
            }
            if self
                .raw(Command::Inspect {
                    session: active.session.clone(),
                })
                .await
                .is_ok_and(|reply| {
                    matches!(reply.result, Err(wire::Failure::Closed))
                        || reply.result.is_ok_and(|s| s.phase == Phase::Closed)
                })
            {
                *slot = None;
            }
        }
    }
}
#[derive(Serialize)]
struct View {
    session: String,
    phase: Phase,
    answer: Option<String>,
    pcm_opened: bool,
    failure: Option<wire::Failure>,
    counters: wire::Counters,
}
impl From<Snapshot> for View {
    fn from(s: Snapshot) -> Self {
        Self {
            session: s.session,
            phase: s.phase,
            answer: s.answer,
            pcm_opened: s.pcm_opened,
            failure: s.failure,
            counters: s.counters,
        }
    }
}
fn hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
async fn core(app: &App, id: &str, command: AuthCommand) -> Result<serde_json::Value> {
    let request = rustsimhub_wire::AuthRequest {
        protocol: AUTH_PROTOCOL,
        id: id.into(),
        command,
    };
    let body = serde_json::to_vec(&request).map_err(|_| "invalid media request")?;
    parse_auth_request(&body).map_err(|_| "invalid media request")?;
    let bytes = forward(app, &body, id).await?;
    let response: CoreResponse =
        serde_json::from_slice(&bytes).map_err(|_| "invalid core reply")?;
    if response.result.get("error").is_some() {
        return Err("media authorization denied");
    }
    Ok(response.result)
}
// Literal host candidates must match the TLS peer. Privacy .local candidates
// map to that same peer, without resolving their names or choosing another IP.
fn offer_for_peer(offer: &str, peer: IpAddr) -> Result<String> {
    if offer.is_empty()
        || offer.len() > wire::MAX_SDP
        || !offer.is_ascii()
        || offer.bytes().any(|b| b < 32 && !matches!(b, b'\r' | b'\n'))
    {
        return Err("invalid media offer");
    }
    let mut result = String::new();
    let mut kept = 0;
    for (index, line) in offer.lines().enumerate() {
        if index >= 160 || line.len() > 1024 {
            return Err("invalid media offer");
        }
        if let Some(candidate) = line.strip_prefix("a=candidate:") {
            let parts: Vec<_> = candidate.split_ascii_whitespace().collect();
            if parts.len() < 8 {
                return Err("invalid media candidate");
            }
            if parts[1] != "1"
                || !parts[2].eq_ignore_ascii_case("udp")
                || parts[5]
                    .parse::<u16>()
                    .ok()
                    .filter(|p| *p >= 1024)
                    .is_none()
                || parts[6] != "typ"
                || parts[7] != "host"
            {
                continue;
            }
            let mdns = parts[4].ends_with(".local")
                && parts[4].len() <= 253
                && parts[4].split('.').all(|label| {
                    !label.is_empty()
                        && label.len() <= 63
                        && label
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                });
            if parts[4].parse::<IpAddr>().ok() != Some(peer) && !mdns {
                continue;
            }
            kept += 1;
            if kept > 8 {
                return Err("too many media candidates");
            }
            if mdns {
                let mut parts = parts.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
                parts[4] = peer.to_string();
                result.push_str("a=candidate:");
                result.push_str(&parts.join(" "));
                result.push_str("\r\n");
                continue;
            }
        }
        result.push_str(line);
        result.push_str("\r\n");
    }
    if kept == 0 {
        return Err("unsupported media network");
    }
    Ok(result)
}
pub(super) async fn capabilities(State(app): State<App>) -> Response {
    axum::Json(serde_json::json!({"protocol":1,"enabled":app.media.is_some(),"profile":"lan_ipv4","remote_relay":false})).into_response()
}
pub(super) async fn api(
    State(app): State<App>,
    connection: Option<Extension<Connection>>,
    body: Bytes,
) -> Response {
    let Some(bridge) = app.media.as_ref() else {
        return denied(StatusCode::SERVICE_UNAVAILABLE);
    };
    if !bridge.rate.lock().is_ok_and(|mut r| r.take()) {
        return denied(StatusCode::TOO_MANY_REQUESTS);
    }
    if body.len() > wire::MAX_FRAME {
        return denied(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let Ok(request) = serde_json::from_slice::<RequestBody>(&body) else {
        return denied(StatusCode::BAD_REQUEST);
    };
    if request.protocol != 1
        || !hex64(&request.token)
        || !wire::session_id(request.command.session())
    {
        return denied(StatusCode::BAD_REQUEST);
    }
    let Some(Extension(connection)) = connection else {
        return denied(StatusCode::FORBIDDEN);
    };
    if !wire::local_ip(connection.peer) || !wire::local_ip(connection.local) {
        return denied(StatusCode::FORBIDDEN);
    }
    // Serialize signaling, including ambiguous cancelled authorization attempts.
    // The background reaper briefly holds this same lock while inspecting the
    // worker. Wait within the existing bounded HTTP admission instead of
    // turning an ordinary inspection into a spurious busy response.
    let Ok(mut state) = timeout(Duration::from_millis(1500), bridge.slot.lock()).await else {
        eprintln!("web_media_rejected reason=slot_busy");
        return denied(StatusCode::TOO_MANY_REQUESTS);
    };
    let connecting = matches!(&request.command, Action::Connect { .. });
    let preflight = connecting || state.as_ref().is_some_and(|s| s.preflight_until.is_some());
    if preflight && request.lease.is_some() && !matches!(&request.command, Action::Activate { .. })
    {
        return denied(StatusCode::BAD_REQUEST);
    }
    let authorization = if preflight {
        AuthCommand::CheckMediaPreflight {
            token: request.token.clone(),
        }
    } else {
        let Some(lease) = request.lease.clone() else {
            return denied(StatusCode::BAD_REQUEST);
        };
        AuthCommand::CheckMediaLease {
            token: request.token.clone(),
            lease,
        }
    };
    let binding = match core(&app, &request.id, authorization).await {
        Ok(value)
            if value[if preflight {
                "media_preflight_valid"
            } else {
                "media_lease_valid"
            }] == true =>
        {
            match value["binding"].as_str().filter(|s| hex64(s)) {
                Some(s) => s.to_owned(),
                None => return denied(StatusCode::SERVICE_UNAVAILABLE),
            }
        }
        _ => {
            eprintln!("web_media_rejected reason=core_lease");
            return denied(StatusCode::FORBIDDEN);
        }
    };
    let session = request.command.session().to_owned();
    if let Some(active) = state.as_ref() {
        if active
            .preflight_until
            .is_some_and(|end| Instant::now() >= end)
            && active.binding == binding
            && active.peer == connection.peer
            && active.local == connection.local
        {
            bridge.stop(&active.session).await;
            *state = None;
            return denied(StatusCode::GONE);
        }
    }
    let result = match request.command {
        Action::Connect { offer, .. } | Action::Prepare { offer, .. } => {
            let hash: [u8; 32] = Sha256::digest(offer.as_bytes()).into();
            if let Some(active) = state.as_ref() {
                if active.session != session
                    || active.binding != binding
                    || active.peer != connection.peer
                    || active.local != connection.local
                    || active.offer_hash != hash
                {
                    return denied(StatusCode::CONFLICT);
                }
            }
            let offer = match offer_for_peer(&offer, connection.peer) {
                Ok(v) => v,
                Err(_) => return denied(StatusCode::BAD_REQUEST),
            };
            if state.is_none() {
                *state = Some(Slot {
                    session: session.clone(),
                    binding,
                    peer: connection.peer,
                    local: connection.local,
                    offer_hash: hash,
                    activation_started: false,
                    preflight_until: connecting.then(|| Instant::now() + Duration::from_secs(30)),
                });
            }
            // Store ownership before sending: a lost reply must not orphan a session.
            bridge
                .exchange(Command::Prepare {
                    session: session.clone(),
                    peer_ip: connection.peer,
                    offer,
                    bind_ip: Some(connection.local),
                })
                .await
        }
        action => {
            let Some(active) = state.as_mut() else {
                return denied(StatusCode::CONFLICT);
            };
            if active.session != session
                || active.binding != binding
                || active.peer != connection.peer
                || active.local != connection.local
            {
                return denied(StatusCode::FORBIDDEN);
            }
            match action {
                Action::Stop { .. } => {
                    eprintln!("web_media_stop reason=client_request");
                    bridge
                        .exchange(Command::Stop {
                            session: session.clone(),
                        })
                        .await
                }
                Action::Poll { .. } => {
                    bridge
                        .exchange(Command::Inspect {
                            session: session.clone(),
                        })
                        .await
                }
                Action::Activate { .. } => {
                    let Some(lease) = request.lease else {
                        return denied(StatusCode::BAD_REQUEST);
                    };
                    let snapshot = match bridge
                        .exchange(Command::Inspect {
                            session: session.clone(),
                        })
                        .await
                    {
                        Ok(v) => v,
                        Err(_) => return denied(StatusCode::SERVICE_UNAVAILABLE),
                    };
                    if active.preflight_until.is_some() && snapshot.phase != Phase::Verified {
                        return denied(StatusCode::CONFLICT);
                    }
                    if matches!(
                        snapshot.phase,
                        Phase::Authorizing | Phase::Active | Phase::Closing | Phase::Closed
                    ) {
                        Ok(snapshot)
                    } else if snapshot.phase != Phase::Verified {
                        return denied(StatusCode::CONFLICT);
                    } else if active.activation_started {
                        bridge.stop(&session).await;
                        return denied(StatusCode::CONFLICT);
                    } else {
                        active.activation_started = true;
                        if active.preflight_until.is_some() {
                            // Only this exact verified transport may move from an
                            // identity/line reservation to an actual ACTIVE call.
                            let bound = core(
                                &app,
                                &request.id,
                                AuthCommand::CheckMediaLease {
                                    token: request.token.clone(),
                                    lease: lease.clone(),
                                },
                            )
                            .await;
                            match bound {
                                Ok(value)
                                    if value["media_lease_valid"] == true
                                        && value["binding"].as_str().is_some_and(hex64) =>
                                {
                                    active.binding = value["binding"].as_str().unwrap().to_owned();
                                    active.preflight_until = None;
                                }
                                _ => {
                                    bridge.stop(&session).await;
                                    return denied(StatusCode::FORBIDDEN);
                                }
                            }
                        }
                        let Some(peer_key) = snapshot.peer_key else {
                            bridge.stop(&session).await;
                            return denied(StatusCode::SERVICE_UNAVAILABLE);
                        };
                        let issued = core(
                            &app,
                            &request.id,
                            AuthCommand::IssueMedia {
                                token: request.token,
                                lease,
                                peer_key,
                            },
                        )
                        .await;
                        let credentials = match issued.and_then(|v| {
                            serde_json::from_value::<rustsimhub_media::protocol::Credentials>(v)
                                .map_err(|_| "invalid media grant")
                        }) {
                            Ok(c) if c.peer_key == peer_key => c,
                            _ => {
                                bridge.stop(&session).await;
                                return denied(StatusCode::FORBIDDEN);
                            }
                        };
                        bridge
                            .exchange(Command::Authorize {
                                session: session.clone(),
                                credentials,
                            })
                            .await
                    }
                }
                Action::Connect { .. } | Action::Prepare { .. } => unreachable!(),
            }
        }
    };
    match result {
        Ok(snapshot) => axum::Json(
            serde_json::json!({"protocol":1,"id":request.id,"result":View::from(snapshot)}),
        )
        .into_response(),
        Err(_) => {
            eprintln!("web_media_stop reason=worker_exchange");
            bridge.stop(&session).await;
            denied(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

#[cfg(test)]
mod tests;
