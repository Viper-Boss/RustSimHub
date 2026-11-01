// SPDX-License-Identifier: AGPL-3.0-only
#![cfg(target_os = "linux")]
//! Actual production sender + loopback TCP/rustls. No external SMTP or real mail.
use base64::{engine::general_purpose::STANDARD, Engine};
use rustsimhub_domain::LineEpoch;
use rustsimhub_forwarder::{
    email::{
        EmailClaim, EmailConfig, EmailFence, EmailOutcome, EmailRequest, EmailSender, EmailStage,
    },
    Binding, Connector, Error, Failure, Message, Secret, Share,
};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::{watch, Notify},
    task::JoinHandle,
    time::timeout,
};
use tokio_rustls::{
    rustls::{
        self,
        pki_types::{CertificateDer, PrivatePkcs8KeyDer},
    },
    server::TlsStream,
    TlsAcceptor,
};

const ID: &str = "00000000-0000-4000-8000-000000000001";
const ATTEMPT: &str = "00000000-0000-4000-8000-000000000002";
const PHASES: [EmailStage; 7] = [
    EmailStage::BeforeConnect,
    EmailStage::BeforeTls,
    EmailStage::BeforeAuthenticate,
    EmailStage::BeforeMailFrom,
    EmailStage::BeforeRecipient,
    EmailStage::BeforeData,
    EmailStage::BeforeSubmit,
];
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Success,
    AuthReject,
    MailReject,
    RecipientReject,
    DataReject,
    DropBeforeData,
    DropAfterData,
    StallBeforeData,
    StallAfterData,
    FinalReject,
    FinalLong,
    AggregateAfterData,
    LongLine,
    ManyLines,
    LargeAggregate,
    MixedCode,
    EarlyReplies,
    BareLf,
    NoPlain,
    GreetingPlainOnly,
    GreetingPlainThenLogin,
    GreetingPlainThenSize,
    PlainExtensionAfterGreeting,
}
#[derive(Default)]
struct Seen {
    commands: Vec<String>,
    auth_matches: bool,
    data: Vec<u8>,
    complete: bool,
    closed: bool,
}
struct Server {
    address: SocketAddr,
    certificate: CertificateDer<'static>,
    seen: Arc<Mutex<Seen>>,
    data_command: Arc<Notify>,
    data_complete: Arc<Notify>,
    task: JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn line(io: &mut BufReader<TlsStream<TcpStream>>) -> Option<Vec<u8>> {
    let mut data = Vec::new();
    for _ in 0..512 {
        data.push(io.read_u8().await.ok()?);
        if data.ends_with(b"\r\n") {
            return Some(data);
        }
    }
    None
}
async fn command(
    io: &mut BufReader<TlsStream<TcpStream>>,
    seen: &Arc<Mutex<Seen>>,
) -> Option<Vec<u8>> {
    let value = line(io).await?;
    let kind = value.split(|b| *b == b' ' || *b == b'\r').next()?;
    seen.lock()
        .unwrap()
        .commands
        .push(String::from_utf8(kind.to_vec()).ok()?);
    Some(value)
}
async fn serve(
    tls: TlsStream<TcpStream>,
    mode: Mode,
    seen: Arc<Mutex<Seen>>,
    data_command: Arc<Notify>,
    data_complete: Arc<Notify>,
) -> Option<()> {
    let mut io = BufReader::new(tls);
    if mode == Mode::EarlyReplies {
        io.write_all(b"220 SYNTHETIC\r\n250 AUTH PLAIN\r\n")
            .await
            .ok()?;
        let _ = command(&mut io, &seen).await;
        return None;
    }
    io.write_all(b"220 SYNTHETIC LOCAL FIXTURE\r\n")
        .await
        .ok()?;
    let ehlo = command(&mut io, &seen).await?;
    if ehlo != b"EHLO rustsimhub.invalid\r\n" {
        return None;
    }
    let response = match mode {
        Mode::LongLine => format!("250-{}\r\n", "X".repeat(600)).into_bytes(),
        Mode::ManyLines => b"250-more\r\n".repeat(33),
        Mode::LargeAggregate => format!("250-{}\r\n", "X".repeat(290))
            .repeat(32)
            .into_bytes(),
        Mode::MixedCode => b"250-first\r\n251 AUTH PLAIN\r\n".to_vec(),
        Mode::BareLf => b"250 AUTH PLAIN\n".to_vec(),
        Mode::NoPlain => b"250 AUTH LOGIN\r\n".to_vec(),
        Mode::GreetingPlainOnly => b"250 AUTH PLAIN\r\n".to_vec(),
        Mode::GreetingPlainThenLogin => b"250-AUTH PLAIN\r\n250 AUTH LOGIN\r\n".to_vec(),
        Mode::GreetingPlainThenSize => b"250-AUTH PLAIN LOGIN\r\n250 SIZE 16384\r\n".to_vec(),
        Mode::PlainExtensionAfterGreeting => b"250-auth PLAIN\r\n250 aUtH pLaIn LOGIN\r\n".to_vec(),
        Mode::AggregateAfterData => {
            (format!("250-{}\r\n", "X".repeat(440)).repeat(14) + "250 AUTH PLAIN\r\n").into_bytes()
        }
        _ => b"250-SYNTHETIC LOCAL FIXTURE\r\n250 AUTH PLAIN LOGIN\r\n".to_vec(),
    };
    io.write_all(&response).await.ok()?;
    let auth = command(&mut io, &seen).await?;
    if let Some(encoded) = auth
        .strip_prefix(b"AUTH PLAIN ")
        .and_then(|v| v.strip_suffix(b"\r\n"))
    {
        seen.lock().unwrap().auth_matches = STANDARD.decode(encoded).ok().as_deref()
            == Some(b"\0SYNTHETIC_USER\0SYNTHETIC_PASSWORD");
    }
    if mode == Mode::AggregateAfterData {
        io.write_all(format!("235 {}\r\n", "X".repeat(480)).as_bytes())
            .await
            .ok()?;
        let _ = command(&mut io, &seen).await?;
        io.write_all(format!("250 {}\r\n", "X".repeat(480)).as_bytes())
            .await
            .ok()?;
        let _ = command(&mut io, &seen).await?;
        io.write_all(format!("250 {}\r\n", "X".repeat(480)).as_bytes())
            .await
            .ok()?;
    } else {
        io.write_all(if mode == Mode::AuthReject {
            b"535 SYNTHETIC denied\r\n"
        } else {
            b"235 SYNTHETIC accepted\r\n"
        })
        .await
        .ok()?;
        let _mail = command(&mut io, &seen).await?;
        io.write_all(if mode == Mode::MailReject {
            b"550 SYNTHETIC denied\r\n"
        } else {
            b"250 SYNTHETIC accepted\r\n"
        })
        .await
        .ok()?;
        let _rcpt = command(&mut io, &seen).await?;
        io.write_all(if mode == Mode::RecipientReject {
            b"550 SYNTHETIC denied\r\n"
        } else {
            b"250 SYNTHETIC accepted\r\n"
        })
        .await
        .ok()?;
    }
    if command(&mut io, &seen).await? != b"DATA\r\n" {
        return None;
    }
    data_command.notify_one();
    if mode == Mode::DropBeforeData {
        return None;
    }
    if mode == Mode::StallBeforeData {
        let _ = io.read_u8().await;
        return None;
    }
    io.write_all(if mode == Mode::DataReject {
        b"250 SYNTHETIC wrong phase\r\n"
    } else {
        b"354 SYNTHETIC continue\r\n"
    })
    .await
    .ok()?;
    loop {
        let value = line(&mut io).await?;
        if value == b".\r\n" {
            seen.lock().unwrap().complete = true;
            data_complete.notify_one();
            break;
        }
        let mut record = seen.lock().unwrap();
        if record.data.len() + value.len() > 16384 {
            return None;
        }
        record.data.extend(value);
    }
    if mode == Mode::DropAfterData {
        return None;
    }
    if mode == Mode::StallAfterData {
        let _ = io.read_u8().await;
        return None;
    }
    if [Mode::FinalLong, Mode::AggregateAfterData].contains(&mode) {
        let size = if mode == Mode::FinalLong { 600 } else { 480 };
        io.write_all(format!("250 {}\r\n", "X".repeat(size)).as_bytes())
            .await
            .ok()?;
        let _ = command(&mut io, &seen).await;
        return None;
    }
    io.write_all(if mode == Mode::FinalReject {
        b"550 SYNTHETIC rejected\r\n"
    } else {
        b"250 SYNTHETIC accepted by local server\r\n"
    })
    .await
    .ok()?;
    // Sender must close locally, not submit QUIT/RSET, reconnect or detach IO.
    let _ = command(&mut io, &seen).await;
    Some(())
}
impl Server {
    async fn start(mode: Mode, name: &str) -> Self {
        let cert = rcgen::generate_simple_self_signed(vec![name.into()]).unwrap();
        let certificate = cert.cert.der().clone();
        let tls = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![certificate.clone()],
            PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der()).into(),
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Seen::default()));
        let data_command = Arc::new(Notify::new());
        let data_complete = Arc::new(Notify::new());
        let (s, d, c) = (seen.clone(), data_command.clone(), data_complete.clone());
        let task = tokio::spawn(async move {
            let _ = timeout(Duration::from_secs(3), async {
                let (tcp, _) = listener.accept().await.ok()?;
                let tls = TlsAcceptor::from(Arc::new(tls)).accept(tcp).await.ok()?;
                serve(tls, mode, s.clone(), d, c).await
            })
            .await;
            s.lock().unwrap().closed = true;
        });
        Self {
            address,
            certificate,
            seen,
            data_command,
            data_complete,
            task,
        }
    }
    async fn closed(&self) {
        timeout(Duration::from_secs(2), async {
            while !self.seen.lock().unwrap().closed {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    fn sender(&self) -> EmailSender {
        EmailSender::new(vec![self.certificate.clone()]).unwrap()
    }
}
struct LocalConnector {
    address: SocketAddr,
    calls: usize,
}
impl Connector for LocalConnector {
    async fn connect(&mut self, host: &str, port: u16) -> rustsimhub_forwarder::Result<TcpStream> {
        assert_eq!((host, port), ("localhost", 465));
        self.calls += 1;
        TcpStream::connect(self.address)
            .await
            .map_err(|_| Error::Storage)
    }
}
struct SyntheticFence {
    stages: Vec<EmailStage>,
    fingerprint: [u8; 32],
    deny: Option<EmailStage>,
    cancel: Option<(EmailStage, watch::Sender<bool>)>,
    block: Option<EmailStage>,
}
impl SyntheticFence {
    fn new(claim: &EmailClaim) -> Self {
        Self {
            stages: vec![],
            fingerprint: claim.fingerprint().unwrap(),
            deny: None,
            cancel: None,
            block: None,
        }
    }
}
impl EmailFence for SyntheticFence {
    async fn check(
        &mut self,
        stage: EmailStage,
        claim: &EmailClaim,
    ) -> rustsimhub_forwarder::Result<()> {
        if PHASES.get(self.stages.len()) != Some(&stage)
            || claim.fingerprint().unwrap() != self.fingerprint
        {
            return Err(Error::Forbidden);
        }
        self.stages.push(stage);
        assert_eq!(claim.line(), LineEpoch(2));
        assert_eq!(claim.generation(), 1);
        assert_eq!(claim.binding().owner_generation, 1);
        if let Some((at, tx)) = &self.cancel {
            if *at == stage {
                let _ = tx.send(true);
            }
        }
        if self.block == Some(stage) {
            std::thread::sleep(Duration::from_millis(150));
        }
        if self.deny == Some(stage) {
            Err(Error::Forbidden)
        } else {
            Ok(())
        }
    }
}
fn config(host: &str, to: &str, password: &str) -> rustsimhub_forwarder::Result<EmailConfig> {
    EmailConfig::new(
        Secret::new(host.into()),
        Secret::new("SYNTHETIC_USER".into()),
        Secret::new(password.into()),
        Secret::new("sender@example.invalid".into()),
        Secret::new(to.into()),
    )
}
fn message(body: &str) -> Message {
    Message {
        id: "00000000-0000-4000-8000-000000000003".into(),
        line: LineEpoch(2),
        sender: "SYNTHETIC_SENDER".into(),
        timestamp: "2026-10-30T12:00:00Z".into(),
        body: body.into(),
        duplicate_candidate: false,
        binary: false,
    }
}
fn claim(share: Share, body: &str) -> EmailClaim {
    let request = EmailRequest::prepare(
        config(
            "localhost",
            "recipient@example.invalid",
            "SYNTHETIC_PASSWORD",
        )
        .unwrap(),
        ID,
        &message(body),
        share,
    )
    .unwrap();
    EmailClaim::new(
        Binding {
            device_key: [1; 32],
            realm: ID.into(),
            owner_key: [2; 32],
            owner_generation: 1,
        },
        ID.into(),
        ATTEMPT.into(),
        1,
        request,
    )
    .unwrap()
}
fn parts(
    server: &Server,
    claim: &EmailClaim,
) -> (
    LocalConnector,
    SyntheticFence,
    watch::Sender<bool>,
    watch::Receiver<bool>,
) {
    let (tx, rx) = watch::channel(false);
    (
        LocalConnector {
            address: server.address,
            calls: 0,
        },
        SyntheticFence::new(claim),
        tx,
        rx,
    )
}

#[tokio::test]
async fn actual_tls_server_acceptance_has_seven_fresh_fences_and_no_second_attempt() {
    let server = Server::start(Mode::Success, "localhost").await;
    let request = claim(Share::Notice, "SYNTHETIC_BODY");
    let (mut connector, mut fence, _tx, rx) = parts(&server, &request);
    assert_eq!(
        server
            .sender()
            .deliver(&request, &mut connector, &mut fence, rx)
            .await,
        EmailOutcome::Confirmed { smtp_code: 250 }
    );
    server.closed().await;
    assert_eq!(fence.stages, PHASES);
    {
        let seen = server.seen.lock().unwrap();
        assert!(seen.auth_matches);
        assert!(seen.complete);
        assert_eq!(seen.commands, ["EHLO", "AUTH", "MAIL", "RCPT", "DATA"]);
    }
    assert_eq!(connector.calls, 1);
    let (_tx, rx) = watch::channel(false);
    assert_eq!(
        server
            .sender()
            .deliver(&request, &mut connector, &mut fence, rx)
            .await,
        EmailOutcome::NotSubmitted {
            reason: Failure::Denied
        }
    );
    assert_eq!(connector.calls, 1);
}
#[tokio::test]
async fn notice_masked_and_full_text_use_actual_mime_base64_without_header_or_command_injection() {
    for share in [Share::Notice, Share::MaskedText, Share::FullText] {
        let server = Server::start(Mode::Success, "localhost").await;
        let request = claim(
            share,
            "SYNTHETIC_PRIVATE_BODY code 123456\r\n.\r\nQUIT\r\n测试中文",
        );
        let (mut connector, mut fence, _tx, rx) = parts(&server, &request);
        assert!(matches!(
            server
                .sender()
                .deliver(&request, &mut connector, &mut fence, rx)
                .await,
            EmailOutcome::Confirmed { .. }
        ));
        server.closed().await;
        let seen = server.seen.lock().unwrap();
        let mime = std::str::from_utf8(&seen.data).unwrap();
        let (headers, body) = mime.split_once("\r\n\r\n").unwrap();
        assert!(!headers.contains("SYNTHETIC_PRIVATE_BODY"));
        assert!(!headers.contains("SYNTHETIC_SENDER"));
        let decoded =
            String::from_utf8(STANDARD.decode(body.replace("\r\n", "")).unwrap()).unwrap();
        assert_eq!(seen.commands.len(), 5);
        if share == Share::Notice {
            assert!(!decoded.contains("SYNTHETIC_PRIVATE_BODY"));
            assert!(!decoded.contains("123456"));
        }
        if share == Share::MaskedText {
            assert!(!decoded.contains("123456"));
            assert!(!decoded.contains("SYNTHETIC_SENDER"));
        }
        if share == Share::FullText {
            assert!(decoded.contains("123456"));
            assert!(decoded.contains("\r\n.\r\nQUIT\r\n"));
        }
    }
}
#[tokio::test]
async fn every_fence_can_deny_and_final_permission_loss_remains_unknown() {
    for stage in PHASES {
        let server = Server::start(Mode::Success, "localhost").await;
        let request = claim(Share::Notice, "SYNTHETIC");
        let (mut connector, mut fence, _tx, rx) = parts(&server, &request);
        fence.deny = Some(stage);
        let out = server
            .sender()
            .deliver(&request, &mut connector, &mut fence, rx)
            .await;
        assert_eq!(
            out,
            if stage == EmailStage::BeforeSubmit {
                EmailOutcome::Unknown {
                    reason: Failure::Denied,
                }
            } else {
                EmailOutcome::NotSubmitted {
                    reason: Failure::Denied,
                }
            }
        );
        if connector.calls != 0 {
            server.closed().await;
        }
        assert!(!server.seen.lock().unwrap().complete);
    }
}
#[tokio::test]
async fn revocation_inside_a_ready_fence_prevents_the_next_socket_write() {
    for stage in PHASES {
        let server = Server::start(Mode::Success, "localhost").await;
        let request = claim(Share::Notice, "SYNTHETIC");
        let (mut connector, mut fence, tx, rx) = parts(&server, &request);
        fence.cancel = Some((stage, tx));
        let out = server
            .sender()
            .deliver(&request, &mut connector, &mut fence, rx)
            .await;
        assert_eq!(
            out,
            if stage == EmailStage::BeforeSubmit {
                EmailOutcome::Unknown {
                    reason: Failure::Cancelled,
                }
            } else {
                EmailOutcome::NotSubmitted {
                    reason: Failure::Cancelled,
                }
            }
        );
        if connector.calls != 0 {
            server.closed().await;
        }
        let seen = server.seen.lock().unwrap();
        assert!(!seen.complete);
        if stage == EmailStage::BeforeAuthenticate {
            assert_eq!(seen.commands, ["EHLO"]);
        }
        if stage == EmailStage::BeforeData {
            assert!(!seen.commands.iter().any(|s| s == "DATA"));
        }
    }
}
#[tokio::test]
async fn off_cancelled_or_missing_supervisor_has_zero_connections_and_callbacks() {
    for closed in [false, true] {
        let server = Server::start(Mode::Success, "localhost").await;
        let request = claim(Share::Notice, "SYNTHETIC");
        let (mut connector, mut fence, tx, rx) = parts(&server, &request);
        if closed {
            drop(tx);
        } else {
            tx.send(true).unwrap();
        }
        assert_eq!(
            server
                .sender()
                .deliver(&request, &mut connector, &mut fence, rx)
                .await,
            EmailOutcome::NotSubmitted {
                reason: Failure::Cancelled
            }
        );
        assert_eq!(connector.calls, 0);
        assert!(fence.stages.is_empty());
    }
}
#[tokio::test]
async fn authentication_envelope_and_wrong_data_phase_rejections_are_not_submitted() {
    for mode in [
        Mode::AuthReject,
        Mode::MailReject,
        Mode::RecipientReject,
        Mode::DataReject,
        Mode::DropBeforeData,
        Mode::NoPlain,
    ] {
        let server = Server::start(mode, "localhost").await;
        let request = claim(Share::Notice, "SYNTHETIC");
        let (mut connector, mut fence, _tx, rx) = parts(&server, &request);
        assert!(matches!(
            server
                .sender()
                .deliver(&request, &mut connector, &mut fence, rx)
                .await,
            EmailOutcome::NotSubmitted { .. }
        ));
        server.closed().await;
        assert!(!server.seen.lock().unwrap().complete);
        assert_eq!(connector.calls, 1);
    }
}
#[tokio::test]
async fn post_data_disconnect_or_negative_ack_is_unknown_and_never_retried() {
    for mode in [
        Mode::DropAfterData,
        Mode::FinalReject,
        Mode::FinalLong,
        Mode::AggregateAfterData,
    ] {
        let server = Server::start(mode, "localhost").await;
        let request = claim(Share::Notice, "SYNTHETIC");
        let (mut connector, mut fence, _tx, rx) = parts(&server, &request);
        assert!(matches!(
            server
                .sender()
                .deliver(&request, &mut connector, &mut fence, rx)
                .await,
            EmailOutcome::Unknown { .. }
        ));
        server.closed().await;
        assert!(server.seen.lock().unwrap().complete);
        assert_eq!(connector.calls, 1);
    }
}

#[tokio::test]
async fn a_different_immutable_claim_is_denied_before_any_connection() {
    let server = Server::start(Mode::Success, "localhost").await;
    let original = claim(Share::FullText, "SYNTHETIC_ORIGINAL");
    let changed = claim(Share::FullText, "SYNTHETIC_CHANGED");
    let (mut connector, mut fence, _tx, rx) = parts(&server, &original);
    assert_eq!(
        server
            .sender()
            .deliver(&changed, &mut connector, &mut fence, rx)
            .await,
        EmailOutcome::NotSubmitted {
            reason: Failure::Denied
        }
    );
    assert_eq!(connector.calls, 0);
}
#[tokio::test]
async fn bounded_strict_replies_reject_malformed_multiline_early_and_oversized_input() {
    for mode in [
        Mode::LongLine,
        Mode::ManyLines,
        Mode::LargeAggregate,
        Mode::MixedCode,
        Mode::EarlyReplies,
        Mode::BareLf,
    ] {
        let server = Server::start(mode, "localhost").await;
        let request = claim(Share::Notice, "SYNTHETIC");
        let (mut connector, mut fence, _tx, rx) = parts(&server, &request);
        assert!(matches!(
            server
                .sender()
                .deliver(&request, &mut connector, &mut fence, rx)
                .await,
            EmailOutcome::NotSubmitted { .. }
        ));
        server.closed().await;
        assert!(!server.seen.lock().unwrap().auth_matches);
        assert_eq!(connector.calls, 1);
    }
}
#[tokio::test]
async fn certificate_and_hostname_verification_happen_before_credentials() {
    for wrong_host in [false, true] {
        let server = Server::start(
            Mode::Success,
            if wrong_host {
                "different.invalid"
            } else {
                "localhost"
            },
        )
        .await;
        let request = claim(Share::Notice, "SYNTHETIC");
        let (mut connector, mut fence, _tx, rx) = parts(&server, &request);
        let sender = if wrong_host {
            server.sender()
        } else {
            let other = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
            EmailSender::new(vec![other.cert.der().clone()]).unwrap()
        };
        assert_eq!(
            sender
                .deliver(&request, &mut connector, &mut fence, rx)
                .await,
            EmailOutcome::NotSubmitted {
                reason: Failure::Tls
            }
        );
        server.closed().await;
        assert!(server.seen.lock().unwrap().commands.is_empty());
    }
}
#[tokio::test]
async fn deadline_before_and_after_data_has_distinct_results_and_closes_real_tls_socket() {
    for mode in [Mode::StallBeforeData, Mode::StallAfterData] {
        let server = Server::start(mode, "localhost").await;
        let request = claim(Share::Notice, "SYNTHETIC");
        let (mut connector, mut fence, _tx, rx) = parts(&server, &request);
        let out = server
            .sender()
            .with_budget(Duration::from_millis(100))
            .unwrap()
            .deliver(&request, &mut connector, &mut fence, rx)
            .await;
        assert_eq!(
            out,
            if mode == Mode::StallAfterData {
                EmailOutcome::Unknown {
                    reason: Failure::Deadline,
                }
            } else {
                EmailOutcome::NotSubmitted {
                    reason: Failure::Deadline,
                }
            }
        );
        server.closed().await;
        assert_eq!(connector.calls, 1);
    }
}
#[tokio::test]
async fn supervisor_loss_during_io_closes_owned_socket_before_or_after_body() {
    for mode in [Mode::StallBeforeData, Mode::StallAfterData] {
        let server = Server::start(mode, "localhost").await;
        let request = claim(Share::Notice, "SYNTHETIC");
        let (mut connector, mut fence, tx, rx) = parts(&server, &request);
        let sender = server.sender();
        let work = sender.deliver(&request, &mut connector, &mut fence, rx);
        let revoke = async {
            if mode == Mode::StallAfterData {
                server.data_complete.notified().await;
            } else {
                server.data_command.notified().await;
            }
            drop(tx);
        };
        let (out, ()) = timeout(Duration::from_secs(2), async { tokio::join!(work, revoke) })
            .await
            .unwrap();
        assert_eq!(
            out,
            if mode == Mode::StallAfterData {
                EmailOutcome::Unknown {
                    reason: Failure::Cancelled,
                }
            } else {
                EmailOutcome::NotSubmitted {
                    reason: Failure::Cancelled,
                }
            }
        );
        server.closed().await;
    }
}
#[tokio::test]
async fn ready_fence_that_blocks_past_absolute_budget_cannot_submit_or_confirm() {
    for stage in [EmailStage::BeforeAuthenticate, EmailStage::BeforeSubmit] {
        let server = Server::start(Mode::Success, "localhost").await;
        let request = claim(Share::Notice, "SYNTHETIC");
        let (mut connector, mut fence, _tx, rx) = parts(&server, &request);
        fence.block = Some(stage);
        let out = server
            .sender()
            .with_budget(Duration::from_millis(100))
            .unwrap()
            .deliver(&request, &mut connector, &mut fence, rx)
            .await;
        assert_eq!(
            out,
            if stage == EmailStage::BeforeSubmit {
                EmailOutcome::Unknown {
                    reason: Failure::Deadline,
                }
            } else {
                EmailOutcome::NotSubmitted {
                    reason: Failure::Deadline,
                }
            }
        );
        server.closed().await;
        assert!(!server.seen.lock().unwrap().complete);
    }
}
#[test]
fn finite_config_rejects_injection_urls_multiple_recipients_and_unsupported_encodings() {
    for host in [
        "https://localhost/",
        "127.0.0.1",
        "a..b",
        "bad\r\nAUTH PLAIN x",
        "-bad.invalid",
        "host.invalid.",
    ] {
        assert!(config(host, "recipient@example.invalid", "SYNTHETIC_PASSWORD").is_err());
    }
    for to in [
        "bad",
        "a@example.invalid,b@example.invalid",
        "Display <a@example.invalid>",
        "a\r\nDATA@example.invalid",
        "a@127.0.0.1",
        "..a@example.invalid",
        "测试@example.invalid",
    ] {
        assert!(config("localhost", to, "SYNTHETIC_PASSWORD").is_err());
    }
    for password in ["", "SYNTHETIC\0PASSWORD", "中文密码"] {
        assert!(config("localhost", "a@example.invalid", password).is_err());
    }
    assert!(config("localhost", "a@example.invalid", &"x".repeat(257)).is_err());
    for mutate in ["timestamp", "oversize", "binary", "duplicate"] {
        let mut m = message("SYNTHETIC");
        match mutate {
            "timestamp" => m.timestamp = "2026\r\nBcc: other@example.invalid".into(),
            "oversize" => m.body = "x".repeat(4097),
            "binary" => m.binary = true,
            _ => m.duplicate_candidate = true,
        };
        assert!(EmailRequest::prepare(
            config("localhost", "a@example.invalid", "SYNTHETIC_PASSWORD").unwrap(),
            ID,
            &m,
            Share::Notice
        )
        .is_err());
    }
}
#[test]
fn immutable_fingerprint_changes_with_private_recipient_password_payload_and_claim_context() {
    let original = claim(Share::FullText, "SYNTHETIC_ONE")
        .fingerprint()
        .unwrap();
    assert_ne!(
        original,
        claim(Share::FullText, "SYNTHETIC_TWO")
            .fingerprint()
            .unwrap()
    );
    for (to, password, generation) in [
        ("other@example.invalid", "SYNTHETIC_PASSWORD", 1),
        ("recipient@example.invalid", "SYNTHETIC_OTHER_PASSWORD", 1),
        ("recipient@example.invalid", "SYNTHETIC_PASSWORD", 2),
    ] {
        let request = EmailRequest::prepare(
            config("localhost", to, password).unwrap(),
            ID,
            &message("SYNTHETIC_ONE"),
            Share::FullText,
        )
        .unwrap();
        let changed = EmailClaim::new(
            Binding {
                device_key: [1; 32],
                realm: ID.into(),
                owner_key: [2; 32],
                owner_generation: 1,
            },
            ID.into(),
            ATTEMPT.into(),
            generation,
            request,
        )
        .unwrap();
        assert_ne!(original, changed.fingerprint().unwrap());
    }
}
#[test]
fn constructor_never_starts_service_and_budget_cannot_be_extended() {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    assert!(EmailSender::new(vec![]).is_err());
    assert!(EmailSender::new(vec![cert.cert.der().clone()])
        .unwrap()
        .with_budget(Duration::ZERO)
        .is_err());
    assert!(EmailSender::new(vec![cert.cert.der().clone()])
        .unwrap()
        .with_budget(Duration::from_secs(11))
        .is_err());
}

#[tokio::test]
async fn ehlo_greeting_is_not_an_auth_plain_extension() {
    for mode in [
        Mode::GreetingPlainOnly,
        Mode::GreetingPlainThenLogin,
        Mode::GreetingPlainThenSize,
    ] {
        let server = Server::start(mode, "localhost").await;
        let request = claim(Share::Notice, "SYNTHETIC_BODY");
        let (mut connector, mut fence, _tx, rx) = parts(&server, &request);
        assert_eq!(
            server
                .sender()
                .deliver(&request, &mut connector, &mut fence, rx)
                .await,
            EmailOutcome::NotSubmitted {
                reason: Failure::Protocol
            }
        );
        server.closed().await;
        assert_eq!(
            fence.stages,
            [EmailStage::BeforeConnect, EmailStage::BeforeTls]
        );
        assert_eq!(connector.calls, 1);
        let seen = server.seen.lock().unwrap();
        assert_eq!(seen.commands, ["EHLO"]);
        assert!(!seen.auth_matches && seen.data.is_empty() && !seen.complete);
    }
}

#[tokio::test]
async fn actual_auth_extension_after_greeting_remains_usable() {
    let server = Server::start(Mode::PlainExtensionAfterGreeting, "localhost").await;
    let request = claim(Share::Notice, "SYNTHETIC_BODY");
    let (mut connector, mut fence, _tx, rx) = parts(&server, &request);
    assert_eq!(
        server
            .sender()
            .deliver(&request, &mut connector, &mut fence, rx)
            .await,
        EmailOutcome::Confirmed { smtp_code: 250 }
    );
    server.closed().await;
    assert_eq!(fence.stages, PHASES);
    let seen = server.seen.lock().unwrap();
    assert!(seen.auth_matches && seen.complete);
    assert_eq!(seen.commands, ["EHLO", "AUTH", "MAIL", "RCPT", "DATA"]);
}
