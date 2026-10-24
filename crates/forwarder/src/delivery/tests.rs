// SPDX-License-Identifier: AGPL-3.0-only
//! Actual loopback TCP/TLS only. Every identity, token and SMS is synthetic.
//! The tiny request reader below is a bounded TEST fixture, not a production
//! HTTP implementation; production uses Hyper and rustls without bypasses.
use super::*;
use crate::{
    Acknowledgement, Binding, Destination, Policy, PreparedRequest, Queue, Rule, Secret, Share,
    Target,
};
use rcgen::generate_simple_self_signed;
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Notify,
    task::JoinHandle,
};
use tokio_rustls::{rustls::pki_types::PrivatePkcs8KeyDer, TlsAcceptor};

#[derive(Clone)]
enum Reply {
    Bytes(Vec<u8>),
    LoseAck,
}
struct Recorded {
    line: String,
    headers: String,
    body: Vec<u8>,
}
struct Server {
    addr: SocketAddr,
    certificate: CertificateDer<'static>,
    seen: Arc<Mutex<Vec<Recorded>>>,
    received: Arc<Notify>,
    closed: Arc<AtomicBool>,
    task: JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    async fn start(reply: Reply) -> Self {
        let certificate = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let root = certificate.cert.der().clone();
        let tls = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![root.clone()],
            PrivatePkcs8KeyDer::from(certificate.signing_key.serialize_der()).into(),
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(vec![]));
        let received = Arc::new(Notify::new());
        let closed = Arc::new(AtomicBool::new(false));
        let (s, n, c) = (seen.clone(), received.clone(), closed.clone());
        let task = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let Ok(mut stream) = TlsAcceptor::from(Arc::new(tls)).accept(tcp).await else {
                return;
            };
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                let Ok(byte) = stream.read_u8().await else {
                    c.store(true, Ordering::SeqCst);
                    return;
                };
                header.push(byte);
                assert!(header.len() <= 8192);
            }
            let header = String::from_utf8(header).unwrap();
            let mut lines = header.lines();
            let line = lines.next().unwrap().to_string();
            let length: usize = lines
                .clone()
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                        .map(|(_, value)| value.trim().parse().unwrap())
                })
                .unwrap();
            assert!(length <= 32768);
            let mut body = vec![0; length];
            stream.read_exact(&mut body).await.unwrap();
            s.lock().unwrap().push(Recorded {
                line,
                headers: header,
                body,
            });
            n.notify_one();
            match reply {
                Reply::Bytes(bytes) => {
                    let _ = stream.write_all(&bytes).await;
                    let _ = stream.shutdown().await;
                }
                Reply::LoseAck => {
                    let mut byte = [0];
                    let count = stream.read(&mut byte).await.unwrap_or(0);
                    c.store(count == 0, Ordering::SeqCst);
                }
            }
        });
        Self {
            addr,
            certificate: root,
            seen,
            received,
            closed,
            task,
        }
    }
    fn url(&self) -> String {
        format!("https://localhost:{}/synthetic?test=1", self.addr.port())
    }
    async fn joined(&mut self) {
        timeout(Duration::from_secs(3), &mut self.task)
            .await
            .unwrap()
            .unwrap();
    }
}
fn response(status: &str, body: &[u8]) -> Reply {
    let mut bytes = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    bytes.extend(body);
    Reply::Bytes(bytes)
}
struct Egress {
    addr: SocketAddr,
    calls: usize,
    requested: Vec<(String, u16)>,
}
impl Connector for Egress {
    async fn connect(&mut self, host: &str, port: u16) -> Result<TcpStream> {
        self.calls += 1;
        self.requested.push((host.to_string(), port));
        TcpStream::connect(self.addr)
            .await
            .map_err(|_| Error::Storage)
    }
}
struct Check {
    deny: Option<Stage>,
    stages: Vec<Stage>,
}
impl Fence for Check {
    async fn check(&mut self, stage: Stage, _: &Claim) -> Result<()> {
        self.stages.push(stage);
        if self.deny == Some(stage) {
            Err(Error::Forbidden)
        } else {
            Ok(())
        }
    }
}
fn claim(url: String) -> Claim {
    Claim {
        id: uuid::Uuid::new_v4().to_string(),
        attempt_id: uuid::Uuid::new_v4().to_string(),
        generation: 1,
        line: rustsimhub_domain::LineEpoch(2),
        request: PreparedRequest {
            url: Secret::new(url),
            bearer: Some(Secret::new("synthetic-bearer".into())),
            body: Secret::new(r#"{"synthetic":"合成文本 <img> 123456"}"#.into()),
            acknowledgement: Acknowledgement::Http2xx,
        },
    }
}
fn route(server: &Server) -> Egress {
    Egress {
        addr: server.addr,
        calls: 0,
        requested: vec![],
    }
}
fn check() -> Check {
    Check {
        deny: None,
        stages: vec![],
    }
}

#[tokio::test]
async fn verified_https_sends_exactly_one_json_request_and_bound_receipt_id() {
    let mut server = Server::start(response("200 OK", b"{}")).await;
    let sender = HttpsSender::new(vec![server.certificate.clone()]).unwrap();
    let job = claim(server.url());
    let mut egress = route(&server);
    let mut check = check();
    let (_stop, cancel) = watch::channel(false);
    assert_eq!(
        sender.deliver(&job, &mut egress, &mut check, cancel).await,
        DeliveryOutcome::Confirmed { status: 200 }
    );
    server.joined().await;
    assert_eq!(egress.calls, 1);
    assert_eq!(
        egress.requested,
        vec![("localhost".into(), server.addr.port())]
    );
    assert_eq!(
        check.stages,
        vec![Stage::BeforeConnect, Stage::BeforeTls, Stage::BeforeSubmit]
    );
    let rows = server.seen.lock().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].line, "POST /synthetic?test=1 HTTP/1.1");
    assert_eq!(rows[0].body, job.request.body.expose().as_bytes());
    let headers = rows[0].headers.to_ascii_lowercase();
    assert!(
        headers.contains("authorization: bearer synthetic-bearer")
            && headers.contains(&format!("x-rustsimhub-delivery-id: {}", job.id))
    );
}
#[tokio::test]
async fn certificate_trust_and_hostname_are_both_required_before_application_bytes() {
    for wrong_host in [false, true] {
        let mut server = Server::start(response("200 OK", b"{}")).await;
        let other = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let roots = if wrong_host {
            vec![server.certificate.clone()]
        } else {
            vec![other.cert.der().clone()]
        };
        let sender = HttpsSender::new(roots).unwrap();
        let url = if wrong_host {
            server.url().replace("localhost", "other.invalid")
        } else {
            server.url()
        };
        let mut egress = route(&server);
        let (_stop, cancel) = watch::channel(false);
        assert_eq!(
            sender
                .deliver(&claim(url), &mut egress, &mut check(), cancel)
                .await,
            DeliveryOutcome::NotSubmitted {
                reason: Failure::Tls
            }
        );
        server.joined().await;
        assert!(server.seen.lock().unwrap().is_empty());
        assert_eq!(egress.calls, 1);
    }
}
#[tokio::test]
async fn every_live_fence_can_stop_before_any_http_request() {
    for deny in [Stage::BeforeConnect, Stage::BeforeTls, Stage::BeforeSubmit] {
        let mut server = Server::start(response("200 OK", b"{}")).await;
        let sender = HttpsSender::new(vec![server.certificate.clone()]).unwrap();
        let mut egress = route(&server);
        let mut check = Check {
            deny: Some(deny),
            stages: vec![],
        };
        let (_stop, cancel) = watch::channel(false);
        assert_eq!(
            sender
                .deliver(&claim(server.url()), &mut egress, &mut check, cancel)
                .await,
            DeliveryOutcome::NotSubmitted {
                reason: Failure::Denied
            }
        );
        assert!(server.seen.lock().unwrap().is_empty());
        if deny == Stage::BeforeConnect {
            assert_eq!(egress.calls, 0);
        } else {
            assert_eq!(egress.calls, 1);
            server.joined().await;
        }
    }
}
#[tokio::test]
async fn initial_cancel_and_loss_of_supervisor_cannot_open_an_egress_socket() {
    for lost in [false, true] {
        let server = Server::start(response("200 OK", b"{}")).await;
        let sender = HttpsSender::new(vec![server.certificate.clone()]).unwrap();
        let mut egress = route(&server);
        let (stop, cancel) = watch::channel(!lost);
        if lost {
            drop(stop);
        }
        assert_eq!(
            sender
                .deliver(&claim(server.url()), &mut egress, &mut check(), cancel)
                .await,
            DeliveryOutcome::NotSubmitted {
                reason: Failure::Cancelled
            }
        );
        assert_eq!(egress.calls, 0);
    }
}
#[tokio::test]
async fn cancellation_during_tls_closes_the_owned_socket_without_http() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let accepted = Arc::new(Notify::new());
    let n = accepted.clone();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        n.notify_one();
        let mut bytes = vec![];
        tcp.take(16384).read_to_end(&mut bytes).await.unwrap();
        bytes
    });
    let certificate = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let sender = HttpsSender::new(vec![certificate.cert.der().clone()]).unwrap();
    let mut egress = Egress {
        addr,
        calls: 0,
        requested: vec![],
    };
    let (stop, cancel) = watch::channel(false);
    let cancelling = async {
        accepted.notified().await;
        stop.send(true).unwrap();
    };
    let job = claim(format!("https://localhost:{}/", addr.port()));
    let mut live = check();
    let (outcome, _) = tokio::join!(
        sender.deliver(&job, &mut egress, &mut live, cancel),
        cancelling
    );
    assert_eq!(
        outcome,
        DeliveryOutcome::NotSubmitted {
            reason: Failure::Cancelled
        }
    );
    let bytes = timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
    assert!(!bytes.windows(5).any(|b| b == b"POST "));
}
#[tokio::test]
async fn lost_http_reply_cancellation_is_unknown_and_closes_the_connection() {
    let mut server = Server::start(Reply::LoseAck).await;
    let sender = HttpsSender::new(vec![server.certificate.clone()]).unwrap();
    let mut egress = route(&server);
    let (stop, cancel) = watch::channel(false);
    let received = server.received.clone();
    let cancelling = async {
        received.notified().await;
        stop.send(true).unwrap();
    };
    let job = claim(server.url());
    let mut live = check();
    let (outcome, _) = tokio::join!(
        sender.deliver(&job, &mut egress, &mut live, cancel),
        cancelling
    );
    assert_eq!(
        outcome,
        DeliveryOutcome::Unknown {
            status: None,
            reason: Failure::Cancelled
        }
    );
    server.joined().await;
    assert!(server.closed.load(Ordering::SeqCst));
    assert_eq!(server.seen.lock().unwrap().len(), 1);
    assert_eq!(egress.calls, 1);
}
#[tokio::test]
async fn total_deadline_bounds_stalled_fence_and_post_submit_wait() {
    struct Stalled;
    impl Fence for Stalled {
        async fn check(&mut self, _: Stage, _: &Claim) -> Result<()> {
            std::future::pending().await
        }
    }
    let server = Server::start(response("200 OK", b"{}")).await;
    let mut sender = HttpsSender::new(vec![server.certificate.clone()]).unwrap();
    sender.budget = Duration::from_millis(100);
    let mut egress = route(&server);
    let (_stop, cancel) = watch::channel(false);
    assert_eq!(
        sender
            .deliver(&claim(server.url()), &mut egress, &mut Stalled, cancel)
            .await,
        DeliveryOutcome::NotSubmitted {
            reason: Failure::Deadline
        }
    );
    assert_eq!(egress.calls, 0);
    drop(server);
    let mut server = Server::start(Reply::LoseAck).await;
    let mut sender = HttpsSender::new(vec![server.certificate.clone()]).unwrap();
    sender.budget = Duration::from_millis(500);
    let mut egress = route(&server);
    let (_stop, cancel) = watch::channel(false);
    assert_eq!(
        sender
            .deliver(&claim(server.url()), &mut egress, &mut check(), cancel)
            .await,
        DeliveryOutcome::Unknown {
            status: None,
            reason: Failure::Deadline
        }
    );
    server.joined().await;
    assert!(server.closed.load(Ordering::SeqCst));
    assert_eq!(server.seen.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn redirects_and_server_errors_are_never_retried_or_followed() {
    for(status,reply) in [(302,Reply::Bytes(b"HTTP/1.1 302 Found\r\nLocation: https://other.invalid/secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec())),(503,response("503 Unavailable",b""))] {
        let mut server=Server::start(reply).await;let sender=HttpsSender::new(vec![server.certificate.clone()]).unwrap();let mut egress=route(&server);let(_stop,cancel)=watch::channel(false);
        assert_eq!(sender.deliver(&claim(server.url()),&mut egress,&mut check(),cancel).await,DeliveryOutcome::Unknown{status:Some(status),reason:Failure::Unconfirmed});server.joined().await;assert_eq!(egress.calls,1);assert_eq!(server.seen.lock().unwrap().len(),1);
    }
}
#[tokio::test]
async fn oversized_fixed_and_chunked_responses_are_bounded_without_decompression() {
    let replies=[response("200 OK",&vec![b'x';8193]),Reply::Bytes([b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n2001\r\n".as_slice(),&vec![b'x';8193],b"\r\n0\r\n\r\n"].concat())];
    for reply in replies {
        let mut server = Server::start(reply).await;
        let sender = HttpsSender::new(vec![server.certificate.clone()]).unwrap();
        let (_stop, cancel) = watch::channel(false);
        assert_eq!(
            sender
                .deliver(
                    &claim(server.url()),
                    &mut route(&server),
                    &mut check(),
                    cancel
                )
                .await,
            DeliveryOutcome::Unknown {
                status: Some(200),
                reason: Failure::TooLarge
            }
        );
        server.joined().await;
    }
    let mut server=Server::start(Reply::Bytes(b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_vec())).await;
    let sender = HttpsSender::new(vec![server.certificate.clone()]).unwrap();
    let (_stop, cancel) = watch::channel(false);
    assert_eq!(
        sender
            .deliver(
                &claim(server.url()),
                &mut route(&server),
                &mut check(),
                cancel
            )
            .await,
        DeliveryOutcome::Unknown {
            status: Some(200),
            reason: Failure::Protocol
        }
    );
    server.joined().await;
}
#[tokio::test]
async fn provider_json_error_is_not_a_successful_delivery() {
    for acknowledgement in [Acknowledgement::Telegram, Acknowledgement::Bark] {
        let mut server = Server::start(response("200 OK", br#"{"ok":false,"code":500}"#)).await;
        let sender = HttpsSender::new(vec![server.certificate.clone()]).unwrap();
        let mut job = claim(server.url());
        job.request.acknowledgement = acknowledgement;
        let (_stop, cancel) = watch::channel(false);
        assert_eq!(
            sender
                .deliver(&job, &mut route(&server), &mut check(), cancel)
                .await,
            DeliveryOutcome::Unknown {
                status: Some(200),
                reason: Failure::Unconfirmed
            }
        );
        server.joined().await;
    }
}

#[tokio::test]
async fn oversized_and_excessive_response_headers_are_rejected_by_http_codec() {
    let mut many = b"HTTP/1.1 200 OK\r\n".to_vec();
    for _ in 0..40 {
        many.extend(b"X-Synthetic: a\r\n");
    }
    many.extend(b"Content-Length: 0\r\nConnection: close\r\n\r\n");
    let large = [
        b"HTTP/1.1 200 OK\r\nX-Synthetic: ".as_slice(),
        &vec![b'x'; 10000],
        b"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    ]
    .concat();
    for bytes in [many, large] {
        let mut server = Server::start(Reply::Bytes(bytes)).await;
        let sender = HttpsSender::new(vec![server.certificate.clone()]).unwrap();
        let (_stop, cancel) = watch::channel(false);
        let result = sender
            .deliver(
                &claim(server.url()),
                &mut route(&server),
                &mut check(),
                cancel,
            )
            .await;
        assert!(matches!(
            result,
            DeliveryOutcome::Unknown {
                reason: Failure::Protocol,
                ..
            }
        ));
        server.joined().await;
    }
}
#[tokio::test]
async fn diagnostics_never_copy_untrusted_body_or_target_credentials() {
    let mut server = Server::start(response("500 Synthetic", b"synthetic-response-secret")).await;
    let sender = HttpsSender::new(vec![server.certificate.clone()]).unwrap();
    let (_stop, cancel) = watch::channel(false);
    let result = sender
        .deliver(
            &claim(server.url()),
            &mut route(&server),
            &mut check(),
            cancel,
        )
        .await;
    let text = format!("{result:?}");
    assert!(
        !text.contains("secret")
            && !text.contains("localhost")
            && !text.contains("bearer")
            && !text.contains("123456")
    );
    server.joined().await;
}
#[tokio::test]
async fn invalid_http_inputs_are_rejected_before_fence_or_connection() {
    let server = Server::start(response("200 OK", b"{}")).await;
    let sender = HttpsSender::new(vec![server.certificate.clone()]).unwrap();
    for kind in [
        "http",
        "credentials",
        "fragment",
        "bearer",
        "array",
        "large",
        "port",
    ] {
        let mut job = claim(server.url());
        match kind {
            "http" => job.request.url = Secret::new("http://localhost/".into()),
            "credentials" => {
                job.request.url = Secret::new("https://user:password@localhost/".into())
            }
            "fragment" => job.request.url = Secret::new("https://localhost/#fragment".into()),
            "bearer" => job.request.bearer = Some(Secret::new("synthetic\r\nHeader:value".into())),
            "array" => job.request.body = Secret::new("[]".into()),
            "large" => job.request.body = Secret::new("x".repeat(32769)),
            "port" => job.request.url = Secret::new("https://localhost:65536/".into()),
            _ => unreachable!(),
        }
        let mut egress = route(&server);
        let mut check = check();
        let (_stop, cancel) = watch::channel(false);
        assert_eq!(
            sender.deliver(&job, &mut egress, &mut check, cancel).await,
            DeliveryOutcome::NotSubmitted {
                reason: Failure::InvalidRequest
            },
            "{kind}"
        );
        assert_eq!(egress.calls, 0);
        assert!(check.stages.is_empty());
    }
    assert!(HttpsSender::new(vec![]).is_err());
    assert!(HttpsSender::new(vec![CertificateDer::from(vec![1, 2, 3])]).is_err());
}

// A real signed Owner -> real committed Store event -> real queue claim ->
// actual verified HTTPS -> durable receipt. This is still synthetic loopback.
struct Business {
    q: Queue,
    binding: Binding,
    auth: rustsimhub_auth::Auth,
    store: rustsimhub_storage::Store,
    claim: Claim,
    _directories: Vec<tempfile::TempDir>,
}
fn business(url: String) -> Business {
    use ed25519_dalek::{Signer, SigningKey};
    use rustsimhub_storage::{IncomingSms, LineObservation, LineStatus, Store};
    use std::{fs, os::unix::fs::PermissionsExt};
    let directories: Vec<_> = (0..3)
        .map(|_| {
            let d = tempfile::TempDir::new().unwrap();
            fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
            d
        })
        .collect();
    let store = Store::open(directories[0].path()).unwrap();
    let h = store.handle();
    let mut auth = rustsimhub_auth::Auth::new(
        rustsimhub_auth::Identity::open(directories[1].path()).unwrap(),
        h.clone(),
    )
    .unwrap();
    let key = SigningKey::from_bytes(&[91; 32]);
    let c = auth
        .bootstrap_challenge(key.verifying_key().to_bytes())
        .unwrap();
    auth.bootstrap_finish(&c.id, &key.sign(&c.transcript).to_bytes())
        .unwrap();
    let c = auth
        .login_challenge(key.verifying_key().to_bytes())
        .unwrap();
    let token = auth
        .login_finish(&c.id, &key.sign(&c.transcript).to_bytes())
        .unwrap();
    let line = h
        .observe_line(LineObservation {
            backend: "synthetic-modem".into(),
            fingerprint: Some([1; 32]),
            status: LineStatus::Ready,
        })
        .unwrap()
        .line;
    let binding = Binding::from_recent_owner(&mut auth, token.expose()).unwrap();
    let mut q = Queue::open(directories[2].path(), binding.clone()).unwrap();
    let target = uuid::Uuid::new_v4().to_string();
    let policy = Policy {
        enabled: true,
        targets: vec![Target {
            id: target.clone(),
            destination: Destination::Webhook {
                url: Secret::new(url),
                bearer: None,
            },
        }],
        rules: vec![Rule {
            id: uuid::Uuid::new_v4().to_string(),
            targets: vec![target],
            sender_equals: None,
            body_contains: None,
            line: None,
            share: Share::Notice,
        }],
    };
    q.configure_owner(
        &mut auth,
        &h,
        token.expose(),
        0,
        &uuid::Uuid::new_v4().to_string(),
        policy,
    )
    .unwrap();
    h.receive_sms(IncomingSms {
        line,
        source: "synthetic-source".into(),
        sender: "synthetic sender".into(),
        timestamp: "synthetic time".into(),
        text: "synthetic private SMS".into(),
        data: vec![],
    })
    .unwrap();
    assert_eq!(q.capture_current(&auth, &h, 1).unwrap(), 1);
    let claim = q.claim_current(&auth, &h, 1).unwrap().unwrap();
    Business {
        q,
        binding,
        auth,
        store,
        claim,
        _directories: directories,
    }
}
struct Current<'a> {
    q: &'a Queue,
    auth: &'a rustsimhub_auth::Auth,
    reader: rustsimhub_storage::Handle,
    revoke: Option<std::path::PathBuf>,
}
impl Fence for Current<'_> {
    async fn check(&mut self, stage: Stage, claim: &Claim) -> Result<()> {
        if stage == Stage::BeforeSubmit {
            if let Some(path) = self.revoke.take() {
                rusqlite::Connection::open(path)?.execute(
                    "UPDATE principals SET active=0,generation=generation+1 WHERE kind='owner'",
                    [],
                )?;
            }
        }
        self.q.check_current_claim(self.auth, &self.reader, claim)
    }
}
#[tokio::test]
async fn real_authority_queue_and_https_ack_commit_once_and_remain_terminal() {
    let mut server = Server::start(response("200 OK", b"{}")).await;
    let mut b = business(server.url());
    let sender = HttpsSender::new(vec![server.certificate.clone()]).unwrap();
    let (_stop, cancel) = watch::channel(false);
    let outcome = sender
        .deliver(
            &b.claim,
            &mut route(&server),
            &mut Current {
                q: &b.q,
                auth: &b.auth,
                reader: b.store.handle(),
                revoke: None,
            },
            cancel,
        )
        .await;
    assert_eq!(outcome, DeliveryOutcome::Confirmed { status: 200 });
    b.q.finish(&b.binding, &b.claim, outcome.queue_state())
        .unwrap();
    assert!(b
        .q
        .claim_current(&b.auth, &b.store.handle(), 1)
        .unwrap()
        .is_none());
    server.joined().await;
    assert_eq!(server.seen.lock().unwrap().len(), 1);
    assert_eq!(b.q.receipts(0, 32).unwrap()[0].state, State::Confirmed);
}
#[tokio::test]
async fn actual_owner_revocation_after_tls_prevents_application_data() {
    let mut server = Server::start(response("200 OK", b"{}")).await;
    let b = business(server.url());
    let sender = HttpsSender::new(vec![server.certificate.clone()]).unwrap();
    let (_stop, cancel) = watch::channel(false);
    let path = b._directories[0].path().join("store.sqlite3");
    let outcome = sender
        .deliver(
            &b.claim,
            &mut route(&server),
            &mut Current {
                q: &b.q,
                auth: &b.auth,
                reader: b.store.handle(),
                revoke: Some(path),
            },
            cancel,
        )
        .await;
    assert_eq!(
        outcome,
        DeliveryOutcome::NotSubmitted {
            reason: Failure::Denied
        }
    );
    server.joined().await;
    assert!(server.seen.lock().unwrap().is_empty());
}
#[tokio::test]
async fn real_received_request_with_lost_ack_survives_as_unknown_without_redelivery() {
    let mut server = Server::start(Reply::LoseAck).await;
    let mut b = business(server.url());
    let mut sender = HttpsSender::new(vec![server.certificate.clone()]).unwrap();
    sender.budget = Duration::from_millis(500);
    let (_stop, cancel) = watch::channel(false);
    let outcome = sender
        .deliver(
            &b.claim,
            &mut route(&server),
            &mut Current {
                q: &b.q,
                auth: &b.auth,
                reader: b.store.handle(),
                revoke: None,
            },
            cancel,
        )
        .await;
    assert_eq!(outcome.queue_state(), State::Unknown);
    b.q.finish(&b.binding, &b.claim, outcome.queue_state())
        .unwrap();
    assert!(b
        .q
        .claim_current(&b.auth, &b.store.handle(), 1)
        .unwrap()
        .is_none());
    server.joined().await;
    assert_eq!(server.seen.lock().unwrap().len(), 1);
    assert_eq!(b.q.receipts(0, 32).unwrap()[0].state, State::Unknown);
}
