// SPDX-License-Identifier: AGPL-3.0-only
//! Plain-HTTP relay for the AirSIM app's local protocol. Edge parses one
//! bounded request, hands it to Core unchanged and writes Core's answer back;
//! it holds no pairing secret and makes no authorization decision.
//!
//! The app parses responses itself: exactly one Content-Length, no
//! Transfer-Encoding, and the connection closed after the body.
use super::{forward, App, Rate};
use rustsimhub_wire::{
    AuthCommand, AuthRequest, Response as CoreResponse, AIRSIM_MAX_BODY, AUTH_PROTOCOL,
};
use serde_json::Value;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Semaphore,
    time::timeout,
};

pub(super) const CONTROL_PORT: u16 = 7592;
pub(super) const PAIRING_PORT: u16 = 7594;
const MAX_HEAD: usize = 8 * 1024;
const READ_WAIT: Duration = Duration::from_secs(10);
/// A long poll is capped at 30 s by Core; allow the last answer to arrive.
const POLL_BUDGET: Duration = Duration::from_secs(36);

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Parsed {
    pub method: String,
    pub target: String,
    pub version: Option<String>,
    pub timestamp: Option<String>,
    pub nonce: Option<String>,
    pub signature: Option<String>,
    pub content_length: usize,
}

/// Parses a request head (everything before the blank line). Anything the
/// relay does not need to understand is refused rather than guessed.
pub(super) fn parse_head(head: &[u8]) -> Option<Parsed> {
    let text = std::str::from_utf8(head).ok()?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, target, protocol) = (first.next()?, first.next()?, first.next()?);
    if first.next().is_some()
        || !matches!(method, "GET" | "POST")
        || !matches!(protocol, "HTTP/1.1" | "HTTP/1.0")
        || !target.starts_with('/')
        || target.len() > 512
        || !target.bytes().all(|b| b.is_ascii_graphic())
    {
        return None;
    }
    let mut parsed = Parsed {
        method: method.to_owned(),
        target: target.to_owned(),
        version: None,
        timestamp: None,
        nonce: None,
        signature: None,
        content_length: 0,
    };
    let mut length_seen = false;
    for line in lines {
        let (name, value) = line.split_once(':')?;
        let value = value.trim();
        if name.is_empty() || name.bytes().any(|b| !b.is_ascii_graphic()) {
            return None;
        }
        let slot = match name.to_ascii_lowercase().as_str() {
            // Chunked or otherwise transformed bodies are never accepted.
            "transfer-encoding" => return None,
            "content-length" => {
                if length_seen || value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                length_seen = true;
                parsed.content_length = value.parse().ok().filter(|n| *n <= AIRSIM_MAX_BODY)?;
                continue;
            }
            "x-airsim-vowlan-version" => &mut parsed.version,
            "x-airsim-vowlan-timestamp" => &mut parsed.timestamp,
            "x-airsim-vowlan-nonce" => &mut parsed.nonce,
            "x-airsim-vowlan-signature" => &mut parsed.signature,
            _ => continue,
        };
        // A repeated authentication header is ambiguous: refuse it.
        if slot.is_some() || value.len() > 256 || !value.bytes().all(|b| b.is_ascii_graphic()) {
            return None;
        }
        *slot = Some(value.to_owned());
    }
    Some(parsed)
}

pub(super) fn response(status: u16, body: &Value) -> Vec<u8> {
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        410 => "Gone",
        429 => "Too Many Requests",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Error",
    };
    let body = body.to_string();
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}
fn error(status: u16, code: &str) -> Vec<u8> {
    response(status, &serde_json::json!({ "error": code }))
}

async fn read_request(stream: &mut TcpStream) -> Option<(Parsed, Vec<u8>)> {
    let mut buffer = Vec::with_capacity(1024);
    let end = loop {
        if let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break at;
        }
        if buffer.len() >= MAX_HEAD {
            return None;
        }
        let mut chunk = [0u8; 1024];
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..n]);
    };
    let parsed = parse_head(&buffer[..end])?;
    let mut body = buffer.split_off(end + 4);
    if body.len() > parsed.content_length {
        return None;
    }
    let missing = parsed.content_length - body.len();
    if missing > 0 {
        let at = body.len();
        body.resize(parsed.content_length, 0);
        stream.read_exact(&mut body[at..]).await.ok()?;
    }
    Some((parsed, body))
}

async fn ask(app: &App, command: AuthCommand) -> Option<Value> {
    let mut raw = [0u8; 12];
    getrandom::fill(&mut raw).ok()?;
    let id: String = raw.iter().map(|b| format!("{b:02x}")).collect();
    let body = serde_json::to_vec(&AuthRequest {
        protocol: AUTH_PROTOCOL,
        id: id.clone(),
        command,
    })
    .ok()?;
    // The same validation Core applies; never send what it would refuse.
    rustsimhub_wire::parse_auth_request(&body).ok()?;
    let reply = forward(app, &body, &id).await.ok()?;
    let parsed: CoreResponse = serde_json::from_slice(&reply).ok()?;
    parsed
        .result
        .get("error")
        .is_none()
        .then_some(parsed.result)
}

/// One connection: one request, one response, then close.
async fn handle(app: &App, mut stream: TcpStream, peer: IpAddr, control: bool) {
    let Ok(Some((head, body))) = timeout(READ_WAIT, read_request(&mut stream)).await else {
        let _ = stream.write_all(&error(400, "bad_request")).await;
        return;
    };
    let Ok(body) = String::from_utf8(body) else {
        let _ = stream.write_all(&error(400, "bad_request")).await;
        return;
    };
    let peer = peer.to_string();
    let command = if control {
        AuthCommand::AirsimControl {
            method: head.method,
            target: head.target,
            version: head.version,
            timestamp: head.timestamp,
            nonce: head.nonce,
            signature: head.signature,
            body,
            peer,
        }
    } else {
        AuthCommand::AirsimPairing {
            method: head.method,
            target: head.target,
            body,
            peer,
        }
    };
    let started = Instant::now();
    let mut result = ask(app, command).await;
    let bytes = loop {
        let Some(value) = result else {
            break error(503, "agent_unavailable");
        };
        if let (Some(status), Some(body)) = (value["status"].as_u64(), value.get("body")) {
            break response(status.min(599) as u16, body);
        }
        // Core is holding a long poll: ask again until it answers.
        let (Some(ticket), true) = (value["ticket"].as_str(), value["wait"] == true) else {
            break error(503, "agent_unavailable");
        };
        if started.elapsed() >= POLL_BUDGET {
            break error(504, "timeout");
        }
        let wait = value["retry_ms"].as_u64().unwrap_or(400).clamp(100, 2000);
        tokio::time::sleep(Duration::from_millis(wait)).await;
        result = ask(
            app,
            AuthCommand::AirsimPoll {
                ticket: ticket.to_owned(),
            },
        )
        .await;
    };
    let _ = timeout(Duration::from_secs(5), stream.write_all(&bytes)).await;
    let _ = stream.shutdown().await;
}

/// Serves both AirSIM ports. `local_allowed` names the stick's own addresses
/// the app may reach it on (Wi-Fi, USB, loopback): a connection that arrived
/// on any other interface, cellular above all, is dropped unanswered.
pub(super) async fn serve(
    app: App,
    local_allowed: impl Fn(IpAddr) -> bool + Clone + Send + 'static,
) {
    // Own budgets: held long polls must not starve the HTTPS page.
    let app = App {
        rate: Arc::new(Mutex::new(Rate {
            since: Instant::now(),
            remaining: 120,
        })),
        limit: Arc::new(Semaphore::new(12)),
        ..app
    };
    let mut tasks = tokio::task::JoinSet::new();
    for (port, control) in [(CONTROL_PORT, true), (PAIRING_PORT, false)] {
        let (app, local_allowed) = (app.clone(), local_allowed.clone());
        tasks.spawn(async move {
            let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
            loop {
                let Ok(listener) = TcpListener::bind(address).await else {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                };
                loop {
                    let Ok((stream, peer)) = listener.accept().await else {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        continue;
                    };
                    let local = stream.local_addr().map(|a| a.ip());
                    if !local.is_ok_and(&local_allowed) {
                        continue;
                    }
                    let Ok(permit) = app.limit.clone().try_acquire_owned() else {
                        continue;
                    };
                    if !app.rate.lock().is_ok_and(|mut r| r.take()) {
                        continue;
                    }
                    let app = app.clone();
                    tokio::spawn(async move {
                        let _permit = permit;
                        handle(&app, stream, peer.ip(), control).await;
                    });
                }
            }
        });
    }
    while tasks.join_next().await.is_some() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(text: &str) -> Option<Parsed> {
        parse_head(text.as_bytes())
    }

    #[test]
    fn a_signed_request_head_is_parsed_case_insensitively() {
        let parsed = head("POST /api/calls/dial?x=1 HTTP/1.1\r\nHost: 192.168.1.9:7592\r\ncontent-length: 18\r\nX-AirSIM-VoWLAN-Version: 1\r\nx-airsim-vowlan-timestamp: 1793600000\r\nX-AIRSIM-VOWLAN-NONCE: nonce-abc\r\nX-AirSIM-VoWLAN-Signature:  sig-value \r\nUser-Agent: AirSIM").unwrap();
        assert_eq!(parsed.method, "POST");
        assert_eq!(parsed.target, "/api/calls/dial?x=1");
        assert_eq!(parsed.content_length, 18);
        assert_eq!(parsed.version.as_deref(), Some("1"));
        assert_eq!(parsed.timestamp.as_deref(), Some("1793600000"));
        assert_eq!(parsed.nonce.as_deref(), Some("nonce-abc"));
        assert_eq!(parsed.signature.as_deref(), Some("sig-value"));
        let plain = head("GET /pair/v1/info HTTP/1.1\r\nHost: x").unwrap();
        assert_eq!((plain.content_length, plain.version), (0, None));
        assert!(head("GET / HTTP/1.0").is_some());
    }

    #[test]
    fn ambiguous_or_unsupported_heads_are_refused() {
        for bad in [
            "",
            "GET",
            "GET /x",
            "DELETE /x HTTP/1.1",
            "GET x HTTP/1.1",
            "GET /x HTTP/2",
            "GET /x HTTP/1.1 extra",
            "GET /a b HTTP/1.1",
            "GET /x HTTP/1.1\r\nno-colon",
            "GET /x HTTP/1.1\r\n: empty-name",
            "GET /x HTTP/1.1\r\nbad name: v",
            "POST /x HTTP/1.1\r\nTransfer-Encoding: chunked",
            "POST /x HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 1",
            "POST /x HTTP/1.1\r\nContent-Length: -1",
            "POST /x HTTP/1.1\r\nContent-Length: +1",
            "POST /x HTTP/1.1\r\nContent-Length: ",
            "POST /x HTTP/1.1\r\nContent-Length: 16385",
            "GET /x HTTP/1.1\r\nX-AirSIM-VoWLAN-Nonce: a\r\nX-AirSIM-VoWLAN-Nonce: b",
            "GET /x HTTP/1.1\r\nX-AirSIM-VoWLAN-Signature: a b",
        ] {
            assert_eq!(head(bad), None, "{bad:?}");
        }
        assert!(head(&format!("GET /{} HTTP/1.1", "a".repeat(512))).is_none());
        assert!(head(&format!(
            "GET /x HTTP/1.1\r\nX-AirSIM-VoWLAN-Signature: {}",
            "s".repeat(257)
        ))
        .is_none());
        assert!(head("POST /x HTTP/1.1\r\nContent-Length: 16384").is_some());
        assert!(parse_head(b"GET /\xff HTTP/1.1").is_none());
    }

    #[test]
    fn responses_carry_one_length_and_close() {
        let bytes = response(200, &serde_json::json!({"ok": true, "名": "值"}));
        let text = String::from_utf8(bytes).unwrap();
        let (headers, body) = text.split_once("\r\n\r\n").unwrap();
        assert!(headers.starts_with("HTTP/1.1 200 OK\r\n"));
        assert_eq!(headers.matches("Content-Length:").count(), 1);
        assert!(
            headers.contains(&format!("Content-Length: {}", body.len())),
            "byte length, not characters"
        );
        assert!(headers.contains("Connection: close"));
        assert!(!headers.to_lowercase().contains("transfer-encoding"));
        assert!(String::from_utf8(response(418, &serde_json::json!({})))
            .unwrap()
            .starts_with("HTTP/1.1 418 Error"));
    }

    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use tokio::net::UnixListener;

    fn app(socket: std::path::PathBuf) -> App {
        let rate = || {
            Arc::new(Mutex::new(Rate {
                since: Instant::now(),
                remaining: 120,
            }))
        };
        App {
            origins: Arc::from(Vec::new()),
            uid: std::fs::metadata(socket.parent().unwrap()).unwrap().uid(),
            socket: Arc::new(socket),
            limit: Arc::new(Semaphore::new(8)),
            rate: rate(),
            legacy_compat: false,
            legacy_rate: rate(),
            legacy_limit: Arc::new(Semaphore::new(2)),
            media: None,
        }
    }

    /// A fake Core answers each framed command from a script and records what it was asked.
    async fn relay(raw: &'static [u8], control: bool, script: Vec<Value>) -> (String, Vec<Value>) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = dir.path().join("core.sock");
        let core = UnixListener::bind(&path).unwrap();
        let asked = tokio::spawn(async move {
            let mut seen = Vec::new();
            for result in script {
                let (mut stream, _) = core.accept().await.unwrap();
                let len = stream.read_u32().await.unwrap() as usize;
                let mut bytes = vec![0; len];
                stream.read_exact(&mut bytes).await.unwrap();
                let request: Value = serde_json::from_slice(&bytes).unwrap();
                let reply = serde_json::to_vec(
                    &serde_json::json!({"protocol": 2, "id": request["id"], "result": result}),
                )
                .unwrap();
                seen.push(request["command"].clone());
                stream.write_u32(reply.len() as u32).await.unwrap();
                stream.write_all(&reply).await.unwrap();
            }
            seen
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let a = app(path);
        let server = tokio::spawn(async move {
            let (stream, peer) = listener.accept().await.unwrap();
            handle(&a, stream, peer.ip(), control).await;
        });
        let mut client = TcpStream::connect(address).await.unwrap();
        client.write_all(raw).await.unwrap();
        let mut answer = Vec::new();
        client.read_to_end(&mut answer).await.unwrap();
        server.await.unwrap();
        (String::from_utf8(answer).unwrap(), asked.await.unwrap())
    }

    #[tokio::test]
    async fn a_control_request_reaches_core_with_the_observed_peer_and_exact_target() {
        let (answer, asked) = relay(
            b"POST /api/sms/send?x=1 HTTP/1.1\r\nHost: stick\r\nContent-Length: 2\r\nX-AirSIM-VoWLAN-Version: 1\r\nX-AirSIM-VoWLAN-Timestamp: 1793600000\r\nX-AirSIM-VoWLAN-Nonce: nonce-abc\r\nX-AirSIM-VoWLAN-Signature: sig\r\n\r\n{}",
            true,
            vec![serde_json::json!({"status": 401, "body": {"error": "authentication_failed"}})],
        )
        .await;
        assert!(
            answer.starts_with("HTTP/1.1 401 Unauthorized\r\n"),
            "{answer}"
        );
        assert!(answer.ends_with(r#"{"error":"authentication_failed"}"#));
        assert_eq!(asked[0]["op"], "airsim_control");
        assert_eq!(asked[0]["target"], "/api/sms/send?x=1");
        assert_eq!(asked[0]["body"], "{}");
        assert_eq!(
            asked[0]["peer"], "127.0.0.1",
            "the address Edge saw, never one the client names"
        );
        assert_eq!(asked[0]["signature"], "sig");
    }

    #[tokio::test]
    async fn pairing_requests_carry_no_headers_and_a_held_poll_is_continued() {
        let (answer, asked) = relay(
            b"GET /pair/v1/info HTTP/1.1\r\nX-AirSIM-VoWLAN-Signature: ignored-here\r\n\r\n",
            false,
            vec![serde_json::json!({"status": 410, "body": {"error": "pairing_expired"}})],
        )
        .await;
        assert!(answer.starts_with("HTTP/1.1 410 Gone\r\n"));
        assert_eq!(asked[0]["op"], "airsim_pairing");
        assert!(asked[0].get("signature").is_none());

        let ticket = "0123456789abcdef0123456789abcdef";
        let (answer, asked) = relay(
            b"GET /api/events?after=5 HTTP/1.1\r\n\r\n",
            true,
            vec![
                serde_json::json!({"wait": true, "ticket": ticket, "retry_ms": 100}),
                serde_json::json!({"wait": true, "ticket": ticket, "retry_ms": 100}),
                serde_json::json!({"status": 200, "body": {"revision": 6}}),
            ],
        )
        .await;
        assert!(answer.ends_with(r#"{"revision":6}"#), "{answer}");
        assert_eq!(asked.len(), 3);
        assert_eq!(
            asked[1],
            serde_json::json!({"op": "airsim_poll", "ticket": ticket})
        );
    }

    #[tokio::test]
    async fn malformed_requests_and_core_errors_never_look_like_success() {
        let (answer, asked) = relay(
            b"GET /x HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n",
            true,
            vec![],
        )
        .await;
        assert!(answer.starts_with("HTTP/1.1 400 "), "{answer}");
        assert!(asked.is_empty(), "nothing malformed reaches Core");
        let (answer, _) = relay(
            b"POST /x HTTP/1.1\r\nContent-Length: 2\r\n\r\n\xff\xfe",
            true,
            vec![],
        )
        .await;
        assert!(answer.starts_with("HTTP/1.1 400 "));
        for result in [
            serde_json::json!({"error": "denied"}),
            serde_json::json!({"unexpected": true}),
            serde_json::json!({"wait": true}),
        ] {
            let (answer, _) = relay(b"GET /v1/health HTTP/1.1\r\n\r\n", true, vec![result]).await;
            assert!(answer.starts_with("HTTP/1.1 503 "), "{answer}");
        }
    }
}
