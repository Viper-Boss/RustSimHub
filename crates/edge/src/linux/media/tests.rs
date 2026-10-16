// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use axum::{
    body::{to_bytes, Body},
    http::Request as HttpRequest,
};
use std::sync::atomic::AtomicUsize;
use tokio::net::UnixListener;
use tower::ServiceExt;

const SESSION: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
fn lease() -> CallLease {
    CallLease {
        call_id: "00000000-0000-4000-8000-000000000001".into(),
        boot: "a".repeat(64),
        line: rustsimhub_domain::LineEpoch(1),
        generation: 1,
        expires_in_seconds: 30,
    }
}
fn offer() -> String {
    "v=0\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\na=sendrecv\r\na=rtcp-mux\r\na=rtpmap:111 opus/48000/2\r\na=candidate:1 1 udp 100 127.0.0.1 12001 typ host\r\na=candidate:2 1 udp 100 10.20.30.40 12002 typ host\r\n".into()
}
fn request(action: serde_json::Value, token: char) -> Request {
    let body=serde_json::to_vec(&serde_json::json!({"protocol":1,"id":"request","token":token.to_string().repeat(64),"lease":lease(),"command":action})).unwrap();
    let mut r = super::super::tests::request("https://localhost:8443", "localhost:8443", body);
    *r.uri_mut() = "/api/media/v1".parse().unwrap();
    r.extensions_mut().insert(Connection {
        peer: "127.0.0.1".parse().unwrap(),
        local: "127.0.0.2".parse().unwrap(),
    });
    r.headers_mut()
        .insert("x-forwarded-for", HeaderValue::from_static("8.8.8.8"));
    r
}
fn command(op: &str) -> serde_json::Value {
    serde_json::json!({"op":op,"session":SESSION})
}
fn prepare() -> serde_json::Value {
    serde_json::json!({"op":"prepare","session":SESSION,"offer":offer()})
}
struct Fixture {
    _directory: tempfile::TempDir,
    app: App,
    phase: Arc<AtomicUsize>,
    issues: Arc<AtomicUsize>,
    worker_requests: Arc<AtomicUsize>,
    wrong_ticket: Arc<AtomicUsize>,
    core_task: tokio::task::JoinHandle<()>,
    worker_task: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.core_task.abort();
        self.worker_task.abort();
    }
}
impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let core_path = directory.path().join("core.sock");
        let worker_path = directory.path().join("worker.sock");
        let core_listener = UnixListener::bind(&core_path).unwrap();
        let worker_listener = UnixListener::bind(&worker_path).unwrap();
        let phase = Arc::new(AtomicUsize::new(0));
        let issues = Arc::new(AtomicUsize::new(0));
        let worker_requests = Arc::new(AtomicUsize::new(0));
        let wrong_ticket = Arc::new(AtomicUsize::new(0));
        let count = issues.clone();
        let wrong = wrong_ticket.clone();
        let core_task = tokio::spawn(async move {
            loop {
                let (mut stream, _) = core_listener.accept().await.unwrap();
                let len = stream.read_u32().await.unwrap();
                let mut bytes = vec![0; len as usize];
                stream.read_exact(&mut bytes).await.unwrap();
                let request = parse_auth_request(&bytes).unwrap();
                let result = match request.command {
                    AuthCommand::CheckMediaLease { token, .. } if token.starts_with('d') => {
                        serde_json::json!({"error":"permission_denied"})
                    }
                    AuthCommand::CheckMediaLease { token, .. } => {
                        serde_json::json!({"media_lease_valid":true,"binding":if token.starts_with('b') {"b".repeat(64)}else{"a".repeat(64)}})
                    }
                    AuthCommand::IssueMedia { peer_key, .. } => {
                        assert_eq!(peer_key, [7; 32]);
                        count.fetch_add(1, Ordering::Relaxed);
                        serde_json::json!({"ticket":vec![9;32],"boot":"c".repeat(64),"peer_key":if wrong.load(Ordering::Relaxed)==0 {[7;32]}else{[8;32]}})
                    }
                    _ => panic!("unexpected synthetic core operation"),
                };
                let bytes = serde_json::to_vec(
                    &serde_json::json!({"protocol":2,"id":request.id,"result":result}),
                )
                .unwrap();
                stream.write_u32(bytes.len() as u32).await.unwrap();
                stream.write_all(&bytes).await.unwrap();
            }
        });
        let state = phase.clone();
        let requests = worker_requests.clone();
        let worker_task = tokio::spawn(async move {
            loop {
                let (mut stream, _) = worker_listener.accept().await.unwrap();
                let len = stream.read_u32().await.unwrap();
                let mut bytes = vec![0; len as usize];
                stream.read_exact(&mut bytes).await.unwrap();
                let request = wire::parse(&bytes).unwrap();
                requests.fetch_add(1, Ordering::Relaxed);
                let session = request.command.session().to_owned();
                let mut answer = None;
                let result = match request.command {
                    Command::Prepare {
                        peer_ip,
                        bind_ip,
                        offer,
                        ..
                    } => {
                        assert_eq!(peer_ip, "127.0.0.1".parse::<IpAddr>().unwrap());
                        assert_eq!(bind_ip, Some("127.0.0.2".parse().unwrap()));
                        assert!(!offer.contains("10.20.30.40"));
                        answer = Some("synthetic_answer".into());
                        None
                    }
                    Command::Inspect { .. } if state.load(Ordering::Relaxed) == 4 => {
                        Some(wire::Failure::Closed)
                    }
                    Command::Inspect { .. } => None,
                    Command::Authorize { credentials, .. } => {
                        assert_eq!(credentials.peer_key, [7; 32]);
                        assert_eq!(credentials.ticket, [9; 32]);
                        state.store(2, Ordering::Relaxed);
                        None
                    }
                    Command::Stop { .. } | Command::StopAll { .. } => {
                        state.store(3, Ordering::Relaxed);
                        None
                    }
                };
                let current = state.load(Ordering::Relaxed);
                let phase = match current {
                    0 => Phase::Handshaking,
                    1 => Phase::Verified,
                    2 => Phase::Active,
                    _ => Phase::Closed,
                };
                let result = match result {
                    Some(e) => Err(e),
                    None => Ok(Snapshot {
                        session,
                        phase,
                        peer_key: (current >= 1).then_some([7; 32]),
                        answer,
                        pcm_opened: current == 2,
                        failure: None,
                        counters: wire::Counters::default(),
                    }),
                };
                let bytes = serde_json::to_vec(&wire::Reply {
                    protocol: 1,
                    id: request.id,
                    result,
                })
                .unwrap();
                stream.write_u32(bytes.len() as u32).await.unwrap();
                stream.write_all(&bytes).await.unwrap();
            }
        });
        let mut app = super::super::tests::app(core_path);
        let uid = std::fs::metadata(directory.path()).unwrap().uid();
        app.media = Some(Arc::new(Bridge::with_uid(worker_path, uid)));
        Self {
            _directory: directory,
            app,
            phase,
            issues,
            worker_requests,
            wrong_ticket,
            core_task,
            worker_task,
        }
    }
    async fn send(&self, r: Request) -> Response {
        router(self.app.clone()).oneshot(r).await.unwrap()
    }
}
#[tokio::test]
async fn brief_background_inspection_does_not_reject_poll_or_skip_authority() {
    let f = Fixture::new().await;
    assert_eq!(
        f.send(request(prepare(), 'a')).await.status(),
        StatusCode::OK
    );
    f.phase.store(2, Ordering::Relaxed);
    let bridge = f.app.media.as_ref().unwrap().clone();
    for token in ['a', 'b'] {
        let guard = bridge.slot.lock().await;
        let release = async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            drop(guard);
        };
        let (response, ()) = tokio::join!(f.send(request(command("poll"), token)), release);
        assert_eq!(
            response.status(),
            if token == 'a' {
                StatusCode::OK
            } else {
                StatusCode::FORBIDDEN
            }
        );
    }
    let before = f.worker_requests.load(Ordering::Relaxed);
    let _guard = bridge.slot.lock().await;
    assert_eq!(
        f.send(request(command("poll"), 'a')).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(f.worker_requests.load(Ordering::Relaxed), before);
    assert_eq!(f.issues.load(Ordering::Relaxed), 0);
}
#[tokio::test]
async fn observed_peer_owner_and_real_verified_phase_precede_single_ticket() {
    let f = Fixture::new().await;
    assert_eq!(
        f.send(request(prepare(), 'd')).await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(f.worker_requests.load(Ordering::Relaxed), 0);
    let response = f.send(request(prepare(), 'a')).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 32768).await.unwrap();
    let text = std::str::from_utf8(&body).unwrap();
    assert!(text.contains("synthetic_answer"));
    assert!(!text.contains("peer_key") && !text.contains("ticket") && !text.contains("binding"));
    assert_eq!(
        f.send(request(prepare(), 'a')).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        f.send(request(command("activate"), 'a')).await.status(),
        StatusCode::CONFLICT
    );
    assert_eq!(f.issues.load(Ordering::Relaxed), 0);
    let internal_check =
        serde_json::json!({"op":"check_media_lease","token":"a".repeat(64),"lease":lease()});
    let body = serde_json::to_vec(
        &serde_json::json!({"protocol":2,"id":"request","command":internal_check}),
    )
    .unwrap();
    let r = super::super::tests::request("https://localhost:8443", "localhost:8443", body);
    assert_eq!(f.send(r).await.status(), StatusCode::FORBIDDEN);
    let before = f.worker_requests.load(Ordering::Relaxed);
    assert_eq!(
        f.send(request(command("stop"), 'b')).await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(f.worker_requests.load(Ordering::Relaxed), before);
    f.phase.store(1, Ordering::Relaxed);
    let response = f.send(request(command("activate"), 'a')).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 32768).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["result"]["phase"], "active");
    assert!(value["result"].get("peer_key").is_none());
    assert_eq!(f.issues.load(Ordering::Relaxed), 1);
    assert_eq!(
        f.send(request(command("activate"), 'a')).await.status(),
        StatusCode::OK
    );
    assert_eq!(f.issues.load(Ordering::Relaxed), 1);
    assert_eq!(
        f.send(request(command("stop"), 'a')).await.status(),
        StatusCode::OK
    );
    f.app.media.as_ref().unwrap().reap().await;
    assert!(f.app.media.as_ref().unwrap().slot.lock().await.is_none());
}
#[tokio::test]
async fn injection_cross_origin_missing_connection_and_changed_offer_fail_closed() {
    let f = Fixture::new().await;
    for action in [
        serde_json::json!({"op":"stop_all","session":SESSION}),
        serde_json::json!({"op":"activate","session":SESSION,"peer_key":vec![7;32]}),
        serde_json::json!({"op":"prepare","session":SESSION,"offer":offer(),"bind_ip":"127.0.0.2"}),
    ] {
        assert_eq!(
            f.send(request(action, 'a')).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
    let mut r = request(prepare(), 'a');
    r.extensions_mut().remove::<Connection>();
    assert_eq!(f.send(r).await.status(), StatusCode::FORBIDDEN);
    let mut r = request(prepare(), 'a');
    r.headers_mut().insert(
        header::ORIGIN,
        HeaderValue::from_static("https://evil.invalid"),
    );
    assert_eq!(f.send(r).await.status(), StatusCode::FORBIDDEN);
    let mut r = request(prepare(), 'a');
    r.extensions_mut().insert(Connection {
        peer: "8.8.8.8".parse().unwrap(),
        local: "127.0.0.2".parse().unwrap(),
    });
    assert_eq!(f.send(r).await.status(), StatusCode::FORBIDDEN);
    assert_eq!(f.worker_requests.load(Ordering::Relaxed), 0);
    assert_eq!(
        f.send(request(prepare(), 'a')).await.status(),
        StatusCode::OK
    );
    let changed = serde_json::json!({"op":"prepare","session":SESSION,"offer":offer()+"a=x-test:changed\r\n"});
    assert_eq!(
        f.send(request(changed, 'a')).await.status(),
        StatusCode::CONFLICT
    );
    let mut r = request(command("poll"), 'a');
    r.extensions_mut().insert(Connection {
        peer: "127.0.0.2".parse().unwrap(),
        local: "127.0.0.2".parse().unwrap(),
    });
    assert_eq!(f.send(r).await.status(), StatusCode::FORBIDDEN);
}
#[tokio::test]
async fn ambiguous_activation_and_substituted_core_grant_stop_without_reissue() {
    for ambiguous in [true, false] {
        let f = Fixture::new().await;
        assert_eq!(
            f.send(request(prepare(), 'a')).await.status(),
            StatusCode::OK
        );
        f.phase.store(1, Ordering::Relaxed);
        if ambiguous {
            f.app
                .media
                .as_ref()
                .unwrap()
                .slot
                .lock()
                .await
                .as_mut()
                .unwrap()
                .activation_started = true;
        } else {
            f.wrong_ticket.store(1, Ordering::Relaxed);
        }
        assert_eq!(
            f.send(request(command("activate"), 'a')).await.status(),
            if ambiguous {
                StatusCode::CONFLICT
            } else {
                StatusCode::FORBIDDEN
            }
        );
        assert_eq!(f.phase.load(Ordering::Relaxed), 3);
        assert_eq!(f.issues.load(Ordering::Relaxed), usize::from(!ambiguous));
        assert_eq!(
            f.send(request(command("activate"), 'a')).await.status(),
            StatusCode::OK
        );
        assert_eq!(f.issues.load(Ordering::Relaxed), usize::from(!ambiguous));
    }
}
#[tokio::test]
async fn edge_takeover_and_reaper_clear_only_acknowledged_owned_resources() {
    let f = Fixture::new().await;
    assert_eq!(
        f.send(request(prepare(), 'a')).await.status(),
        StatusCode::OK
    );
    f.phase.store(4, Ordering::Relaxed);
    f.app.media.as_ref().unwrap().reap().await;
    assert!(f.app.media.as_ref().unwrap().slot.lock().await.is_none());
    f.phase.store(0, Ordering::Relaxed);
    assert_eq!(
        f.send(request(prepare(), 'a')).await.status(),
        StatusCode::OK
    );
    f.phase.store(2, Ordering::Relaxed);
    f.app.media.as_ref().unwrap().takeover().await.unwrap();
    assert_eq!(f.phase.load(Ordering::Relaxed), 3);
    assert!(f.app.media.as_ref().unwrap().slot.lock().await.is_none());
    assert_eq!(f.issues.load(Ordering::Relaxed), 0);
}
#[tokio::test]
async fn generic_http_cannot_issue_media_and_disabled_mode_keeps_microphone_off() {
    let f = Fixture::new().await;
    let body=serde_json::to_vec(&serde_json::json!({"protocol":2,"id":"request","command":{"op":"issue_media","token":"a".repeat(64),"lease":lease(),"peer_key":vec![7;32]}})).unwrap();
    let r = super::super::tests::request("https://localhost:8443", "localhost:8443", body);
    assert_eq!(f.send(r).await.status(), StatusCode::FORBIDDEN);
    assert_eq!(f.issues.load(Ordering::Relaxed), 0);
    let mut app = f.app.clone();
    app.media = None;
    let response = router(app).oneshot(request(prepare(), 'a')).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response.headers()["permissions-policy"],
        "camera=(), microphone=(), geolocation=()"
    );
    let response = f
        .send(
            HttpRequest::builder()
                .uri("/api/media/capabilities")
                .header(header::HOST, "localhost:8443")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["permissions-policy"],
        "camera=(), microphone=(self), geolocation=()"
    );
}
#[test]
fn candidate_filter_never_resolves_dns_or_accepts_other_destinations() {
    let peer = "127.0.0.1".parse().unwrap();
    let filtered = offer_for_peer(&offer(), peer).unwrap();
    assert!(filtered.contains("127.0.0.1"));
    assert!(!filtered.contains("10.20.30.40"));
    let mdns = offer_for_peer(&offer().replace("127.0.0.1", "client.local"), peer).unwrap();
    assert!(mdns.contains("127.0.0.1"));
    assert!(!mdns.contains("client.local"));
    for candidate in ["evil.example", "8.8.8.8", "::1"] {
        assert!(offer_for_peer(&offer().replace("127.0.0.1", candidate), peer).is_err());
    }
    assert!(offer_for_peer(&(offer() + &"a=long\r\n".repeat(161)), peer).is_err());
}
