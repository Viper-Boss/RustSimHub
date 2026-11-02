// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
// Compile the actual production source, no copied/reimplemented TURN driver.
#[path = "../src/turn.rs"]
pub(crate) mod turn;
pub use rustsimhub_audio_alsa::codec;
#[path = "../src/remote.rs"]
pub(crate) mod remote;
#[path = "../src/rtc.rs"]
pub(crate) mod rtc;

use std::{
    fs::File,
    net::SocketAddr,
    path::Path,
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, UdpSocket},
};
use tokio_rustls::{
    rustls::{
        self,
        pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
    },
    TlsAcceptor,
};
use turn::{Driver, Error, Registration};
pub(crate) struct Server {
    child: Child,
    directory: tempfile::TempDir,
    pub(crate) address: SocketAddr,
}
impl Drop for Server {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let log = std::fs::read_to_string(self.directory.path().join("coturn.log"))
                .unwrap_or_default();
            for line in log
                .lines()
                .filter(|line| {
                    line.contains("ERROR")
                        || line.contains("ALLOCATE")
                        || line.contains("401")
                        || line.contains("438")
                })
                .take(20)
            {
                eprintln!("fixture Coturn: {line}");
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn openssl(dir: &Path, args: &[&str]) {
    let output = Command::new("openssl")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "synthetic certificate generation failed ({}): {}",
        args[0],
        String::from_utf8_lossy(&output.stderr)
    );
}
fn certificates() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    openssl(
        p,
        &[
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-subj",
            "/CN=private fixture root",
            "-keyout",
            "ca.key",
            "-out",
            "ca.pem",
            "-days",
            "2",
        ],
    );
    openssl(
        p,
        &[
            "req",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-subj",
            "/CN=turn.fixture.invalid",
            "-keyout",
            "server.key",
            "-out",
            "server.csr",
        ],
    );
    std::fs::write(p.join("extensions"),b"subjectAltName=DNS:turn.fixture.invalid\nextendedKeyUsage=serverAuth\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\n").unwrap();
    openssl(
        p,
        &[
            "x509",
            "-req",
            "-in",
            "server.csr",
            "-CA",
            "ca.pem",
            "-CAkey",
            "ca.key",
            "-CAcreateserial",
            "-out",
            "server.pem",
            "-days",
            "1",
            "-extfile",
            "extensions",
        ],
    );
    std::fs::write(p.join("index.txt"), b"").unwrap();
    std::fs::write(p.join("serial"), b"01\n").unwrap();
    std::fs::write(p.join("ca.conf"),b"[ca]\ndefault_ca=fixture\n[fixture]\ndatabase=index.txt\nserial=serial\nnew_certs_dir=.\ncertificate=ca.pem\nprivate_key=ca.key\ndefault_md=sha256\npolicy=sign\n[sign]\ncommonName=supplied\n[server]\nsubjectAltName=DNS:turn.fixture.invalid\nextendedKeyUsage=serverAuth\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\n").unwrap();
    openssl(
        p,
        &[
            "ca",
            "-batch",
            "-config",
            "ca.conf",
            "-in",
            "server.csr",
            "-out",
            "expired.pem",
            "-notext",
            "-startdate",
            "20000101000000Z",
            "-enddate",
            "20000102000000Z",
            "-extensions",
            "server",
        ],
    );
    for name in ["ca", "server", "expired"] {
        openssl(
            p,
            &[
                "x509",
                "-in",
                &format!("{name}.pem"),
                "-outform",
                "DER",
                "-out",
                &format!("{name}.der"),
            ],
        );
    }
    openssl(
        p,
        &[
            "pkcs8",
            "-topk8",
            "-nocrypt",
            "-in",
            "server.key",
            "-outform",
            "DER",
            "-out",
            "server-key.der",
        ],
    );
    dir
}
fn roots(dir: &Path) -> rustls::RootCertStore {
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(CertificateDer::from(
            std::fs::read(dir.join("ca.der")).unwrap(),
        ))
        .unwrap();
    roots
}
fn reg(address: SocketAddr, dir: &Path) -> Registration {
    Registration::loopback(
        address,
        "turn.fixture.invalid",
        roots(dir),
        "fixture".into(),
        "synthetic-only".into(),
        Instant::now() + Duration::from_secs(600),
    )
    .unwrap()
}
impl Server {
    pub(crate) async fn start() -> Self {
        assert_eq!(
            std::env::var("RUSTSIMHUB_PRIVATE_TURN_FIXTURE").as_deref(),
            Ok("1"),
            "run only through explicit network-none fixture"
        );
        let directory = certificates();
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = probe.local_addr().unwrap();
        drop(probe);
        let logfile = File::create(directory.path().join("coturn.log")).unwrap();
        let child = Command::new("turnserver")
            .args([
                "-n",
                "--no-cli",
                "--no-tcp",
                "--no-udp",
                "--no-dtls",
                "--no-tcp-relay",
                "--no-tlsv1",
                "--no-tlsv1_1",
                "--listening-ip=127.0.0.1",
                "--relay-ip=127.0.0.1",
                "--relay-threads=0",
                "--max-allocate-lifetime=600",
                "--min-port=41000",
                "--max-port=41031",
                "--lt-cred-mech",
                "--realm=fixture.invalid",
                "--user=fixture:synthetic-only",
                "--stale-nonce=1",
                "--verbose",
                "--fingerprint",
                "--allow-loopback-peers",
                "--no-multicast-peers",
                "--simple-log",
                "--log-file=stdout",
            ])
            .arg(format!(
                "--userdb={}",
                directory.path().join("turn.db").display()
            ))
            .arg(format!("--tls-listening-port={}", address.port()))
            .arg(format!(
                "--pidfile={}",
                directory.path().join("turn.pid").display()
            ))
            .arg(format!(
                "--cert={}",
                directory.path().join("server.pem").display()
            ))
            .arg(format!(
                "--pkey={}",
                directory.path().join("server.key").display()
            ))
            .stdin(Stdio::null())
            .stdout(logfile.try_clone().unwrap())
            .stderr(logfile)
            .spawn()
            .expect("fixed Coturn executable required");
        let mut result = Self {
            child,
            directory,
            address,
        };
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                assert!(
                    result.child.try_wait().unwrap().is_none(),
                    "Coturn exited; private fixture log retained until test exits"
                );
                if tokio::net::TcpStream::connect(address).await.is_ok() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        result
    }
    pub(crate) fn ca_der(&self) -> Vec<u8> {
        std::fs::read(self.directory.path().join("ca.der")).unwrap()
    }
    pub(crate) fn registration(&self) -> Registration {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(CertificateDer::from(self.ca_der())).unwrap();
        Registration::loopback(
            self.address,
            "turn.fixture.invalid",
            roots,
            "fixture".into(),
            "synthetic-only".into(),
            Instant::now() + Duration::from_secs(600),
        )
        .unwrap()
    }
}
async fn datagram(driver: &mut Driver) -> Result<Vec<u8>, Error> {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(data) = driver.step(Duration::from_millis(10)).await? {
                return Ok(data);
            }
        }
    })
    .await
    .map_err(|_| Error::Deadline)?
}
#[tokio::test]
async fn actual_coturn_tls_allocation_permission_channel_and_odd_datagram_roundtrip() {
    let server = Server::start().await;
    let mut driver = Driver::allocate(server.registration()).await.unwrap();
    let relay = driver.allocation().address;
    assert!(relay.ip().is_loopback() && relay.port() >= 41000);
    assert_eq!(driver.send(b"no permission").await, Err(Error::Peer));
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    driver.bind_peer(peer.local_addr().unwrap()).await.unwrap();
    assert_eq!(driver.peer(), Some(peer.local_addr().unwrap()));
    driver.ready().unwrap();
    let payload = b"synthetic-odd-123";
    driver.send(payload).await.unwrap();
    let mut buffer = [0; 1501];
    let (n, source) = tokio::time::timeout(Duration::from_secs(2), peer.recv_from(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(source, relay);
    assert_eq!(&buffer[..n], payload);
    peer.send_to(&buffer[..n], source).await.unwrap();
    assert_eq!(datagram(&mut driver).await.unwrap(), payload);
    driver.close().await; // Deliberately no release_verified claim.
}
#[tokio::test]
async fn actual_coturn_wrong_credentials_never_produce_an_allocation() {
    let server = Server::start().await;
    let r = Registration::loopback(
        server.address,
        "turn.fixture.invalid",
        roots(server.directory.path()),
        "fixture".into(),
        "wrong-synthetic".into(),
        Instant::now() + Duration::from_secs(600),
    )
    .unwrap();
    assert!(Driver::allocate(r).await.is_err());
}
#[tokio::test]
async fn actual_coturn_wrong_ca_and_hostname_are_rejected_by_normal_tls() {
    let server = Server::start().await;
    let foreign = certificates();
    for (name, trust) in [
        ("turn.fixture.invalid", roots(foreign.path())),
        ("wrong.fixture.invalid", roots(server.directory.path())),
    ] {
        let r = Registration::loopback(
            server.address,
            name,
            trust,
            "fixture".into(),
            "synthetic-only".into(),
            Instant::now() + Duration::from_secs(600),
        )
        .unwrap();
        assert!(matches!(Driver::allocate(r).await, Err(Error::Certificate)));
    }
}
#[tokio::test]
async fn actual_coturn_unexpected_same_ip_peer_and_oversize_channel_close_the_driver() {
    let server = Server::start().await;
    for oversized in [false, true] {
        let mut driver = Driver::allocate(server.registration()).await.unwrap();
        let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        driver.bind_peer(peer.local_addr().unwrap()).await.unwrap();
        let other = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let bytes = vec![0x5a; if oversized { 1501 } else { 20 }];
        (if oversized { &peer } else { &other })
            .send_to(&bytes, driver.allocation().address)
            .await
            .unwrap();
        assert_eq!(
            datagram(&mut driver).await.unwrap_err(),
            if oversized {
                Error::Protocol
            } else {
                Error::Peer
            }
        );
        assert_eq!(driver.ready(), Err(Error::Closed));
    }
}
#[tokio::test]
async fn cancellation_of_actual_incoming_io_cannot_reuse_the_same_allocation() {
    let server = Server::start().await;
    let mut driver = Driver::allocate(server.registration()).await.unwrap();
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    driver.bind_peer(peer.local_addr().unwrap()).await.unwrap();
    assert!(tokio::time::timeout(
        Duration::from_millis(30),
        driver.step(Duration::from_secs(2))
    )
    .await
    .is_err());
    assert_eq!(driver.ready(), Err(Error::Closed));
    assert_eq!(driver.send(b"no replay").await, Err(Error::Closed));
}
fn acceptor(dir: &Path, expired: bool) -> TlsAcceptor {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let cert = CertificateDer::from(
        std::fs::read(dir.join(if expired { "expired.der" } else { "server.der" })).unwrap(),
    );
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
        std::fs::read(dir.join("server-key.der")).unwrap(),
    ));
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .unwrap();
    TlsAcceptor::from(Arc::new(config))
}
#[tokio::test]
async fn normal_tls_rejects_expired_certificate_without_a_custom_verifier() {
    let dir = certificates();
    let tls = acceptor(dir.path(), true);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (s, _) = listener.accept().await.unwrap();
        assert!(tls.accept(s).await.is_err());
    });
    assert!(matches!(
        Driver::allocate(reg(address, dir.path())).await,
        Err(Error::Certificate)
    ));
    server.await.unwrap();
}
#[tokio::test]
async fn actual_tls_partial_and_oversize_frames_fail_before_upstream_buffer_growth() {
    let dir = certificates();
    for (prefix, hold, expected) in [
        (vec![0, 1, 255, 252], false, Error::Bounds),
        (vec![0, 1], false, Error::Transport),
        (vec![0, 1], true, Error::Deadline),
        (vec![0; 20], false, Error::Protocol),
    ] {
        let tls = acceptor(dir.path(), false);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (s, _) = listener.accept().await.unwrap();
            let mut s = tls.accept(s).await.unwrap();
            let mut header = [0; 4];
            s.read_exact(&mut header).await.unwrap();
            let n = usize::from(u16::from_be_bytes([header[2], header[3]]));
            let mut request = vec![0; n + 16];
            s.read_exact(&mut request).await.unwrap();
            s.write_all(&prefix).await.unwrap();
            s.flush().await.unwrap();
            if hold {
                tokio::time::sleep(Duration::from_millis(700)).await;
            }
        });
        assert!(matches!(Driver::allocate(reg(address,dir.path())).await,Err(e) if e==expected));
        server.await.unwrap();
    }
}
#[tokio::test]
async fn cancelled_tls_handshake_closes_actual_socket_without_detached_allocation() {
    let dir = certificates();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut scratch = [0; 4096];
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if socket.read(&mut scratch).await.unwrap() == 0 {
                    break;
                }
            }
        })
        .await
        .unwrap();
    });
    assert!(tokio::time::timeout(
        Duration::from_millis(40),
        Driver::allocate(reg(address, dir.path()))
    )
    .await
    .is_err());
    server.await.unwrap();
}
#[test]
fn production_registration_never_accepts_test_loopback_private_or_arbitrary_names() {
    let dir = certificates();
    for ip in [
        "127.0.0.1",
        "10.0.0.1",
        "169.254.169.254",
        "168.63.129.16",
        "100.64.0.1",
        "192.0.2.1",
        "198.51.100.1",
        "203.0.113.1",
        "224.0.0.1",
        "0.0.0.0",
    ] {
        let address = format!("{ip}:5349").parse().unwrap();
        assert!(Registration::from_installed(
            address,
            "turn.fixture.invalid",
            roots(dir.path()),
            "fixture".into(),
            "synthetic-only".into(),
            Instant::now() + Duration::from_secs(600)
        )
        .is_err());
    }
}

pub(crate) fn fingerprint(sdp: &str) -> [u8; 32] {
    let value = sdp
        .lines()
        .find_map(|line| line.strip_prefix("a=fingerprint:"))
        .unwrap();
    let fp: str0m::crypto::Fingerprint = value.parse().unwrap();
    fp.bytes.as_slice().try_into().unwrap()
}
pub(crate) fn permit(
    key: [u8; 32],
) -> (
    rustsimhub_media::Authority,
    rustsimhub_media::Context,
    rustsimhub_media::Permit,
) {
    let now = rustsimhub_media::local::now_ms();
    let context = rustsimhub_media::Context {
        boot: "a".repeat(64),
        backend: "synthetic-normal-core".into(),
        line: 1,
        call: "c".repeat(36),
        call_revision: 1,
        lease_generation: 1,
        principal: "p".repeat(36),
        principal_generation: 1,
        peer_key: key,
        lease_until_ms: now + 30_000,
        active: true,
        media_scope: true,
        control_scope: true,
    };
    let mut authority = rustsimhub_media::Authority::new();
    let ticket = authority.issue(context.clone(), now).unwrap();
    let permit = authority.consume(ticket, &context, &key, now).unwrap();
    (authority, context, permit)
}
async fn pair(server: &Server) -> (remote::Session, remote::Session) {
    let (mut a, offer) = remote::Session::offer(server.registration()).await.unwrap();
    assert!(a.verified_peer().is_none());
    let (mut b, answer) =
        remote::Session::answer(server.registration(), &offer, fingerprint(&offer))
            .await
            .unwrap();
    assert!(b.verified_peer().is_none());
    a.accept_answer(&answer, fingerprint(&answer))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while a.verified_peer().is_none() || b.verified_peer().is_none() {
            a.service().await.unwrap();
            b.service().await.unwrap();
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("actual selected relay and DTLS must both finish");
    let (alocal, aremote) = a.verified_peer().unwrap().selected_path();
    let (blocal, bremote) = b.verified_peer().unwrap().selected_path();
    assert_eq!((alocal, aremote), (bremote, blocal));
    assert_ne!(alocal, blocal);
    assert_eq!(a.counters(), (0, 0, 0));
    assert_eq!(b.counters(), (0, 0, 0));
    (a, b)
}
#[tokio::test]
async fn actual_forced_relay_dtls_opus_both_directions_and_original_permit_revocation() {
    let server = Server::start().await;
    let (mut a, mut b) = pair(&server).await;
    let akey = a.verified_peer().unwrap().fingerprint();
    let bkey = b.verified_peer().unwrap().fingerprint();
    let (mut aa, ac, ap) = permit(akey);
    let (mut ba, bc, bp) = permit(bkey);
    a.authorize(ap.clone(), akey).unwrap();
    b.authorize(bp, bkey).unwrap();
    let mut arx = 0;
    let mut brx = 0;
    for sequence in 0..12 {
        let now = rustsimhub_media::local::now_ms();
        aa.refresh(Some(&ac), now).unwrap();
        ba.refresh(Some(&bc), now).unwrap();
        let samples = std::array::from_fn(|i| ((i as f64 * 0.17).sin() * 2000.0) as i16);
        let frame = rustsimhub_media::Frame {
            sequence,
            at_ms: now,
            samples,
        };
        a.send(&frame).await.unwrap();
        b.send(&frame).await.unwrap();
        let until = tokio::time::Instant::now() + Duration::from_millis(20);
        while tokio::time::Instant::now() < until {
            a.service().await.unwrap();
            b.service().await.unwrap();
            if let Some(frame) = a.receive().unwrap() {
                assert!(frame.samples.iter().any(|x| *x != 0));
                arx += 1;
            }
            if let Some(frame) = b.receive().unwrap() {
                assert!(frame.samples.iter().any(|x| *x != 0));
                brx += 1;
            }
        }
    }
    assert!(
        arx >= 4 && brx >= 4,
        "actual decoded relay audio in both directions"
    );
    assert!(a.counters().0 >= 12 && b.counters().0 >= 12);
    let sent = a.counters().0;
    aa.revoke();
    assert!(ap.check(rustsimhub_media::local::now_ms()).is_err());
    let frame = rustsimhub_media::Frame {
        sequence: 13,
        at_ms: rustsimhub_media::local::now_ms(),
        samples: [1000; rustsimhub_media::SAMPLES],
    };
    assert!(a.send(&frame).await.is_err());
    assert_eq!(a.counters().0, sent);
    assert!(a.verified_peer().is_none());
    a.close().await;
    b.close().await;
}
#[tokio::test]
async fn relay_dtls_without_core_permit_cannot_encode_any_speech_or_accept_wrong_ticket_peer() {
    let server = Server::start().await;
    let (mut a, b) = pair(&server).await;
    let frame = rustsimhub_media::Frame {
        sequence: 0,
        at_ms: rustsimhub_media::local::now_ms(),
        samples: [1000; rustsimhub_media::SAMPLES],
    };
    assert!(a.send(&frame).await.is_err());
    assert_eq!(a.counters().0, 0);
    a.close().await;
    b.close().await;
    let (mut a, b) = pair(&server).await;
    let (_authority, _context, permit) = permit([7; 32]);
    assert!(a.authorize(permit, [7; 32]).is_err());
    assert_eq!(a.counters().0, 0);
    a.close().await;
    b.close().await;
}
#[tokio::test]
async fn relay_signaling_pin_and_host_fallback_are_rejected_before_an_allocation() {
    let server = Server::start().await;
    let (a, offer) = remote::Session::offer(server.registration()).await.unwrap();
    assert!(
        remote::Session::answer(server.registration(), &offer, [7; 32])
            .await
            .is_err()
    );
    let host = offer.replace("typ relay", "typ host");
    assert!(
        remote::Session::answer(server.registration(), &host, fingerprint(&offer))
            .await
            .is_err()
    );
    a.close().await;
}

#[tokio::test]
async fn actual_coturn_stale_nonce_and_refresh_survive_protocol_timer_advancement() {
    let server = Server::start().await;
    let mut driver = Driver::allocate(server.registration()).await.unwrap();
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    driver.bind_peer(peer.local_addr().unwrap()).await.unwrap();
    // Only the library's protocol timer advances. Actual TLS/nonce server,
    // credential deadline and Core heartbeat clocks are never substituted.
    tokio::time::sleep(Duration::from_millis(2100)).await;
    for _ in 0..35 {
        driver.origin = driver.origin.checked_sub(Duration::from_secs(30)).unwrap();
        let end = tokio::time::Instant::now() + Duration::from_millis(60);
        while tokio::time::Instant::now() < end {
            driver.step(Duration::from_millis(5)).await.unwrap();
        }
        driver.ready().unwrap();
    }
    driver.send(b"after actual refresh").await.unwrap();
    let mut bytes = [0; 64];
    let (n, source) = tokio::time::timeout(Duration::from_secs(1), peer.recv_from(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&bytes[..n], b"after actual refresh");
    assert_eq!(source, driver.allocation().address);
    let log = std::fs::read_to_string(server.directory.path().join("coturn.log")).unwrap();
    assert!(
        log.contains("438"),
        "real server stale-nonce response required"
    );
    assert!(
        log.lines()
            .any(|line| line.contains("REFRESH") && line.contains("success")),
        "real server verified refresh required"
    );
    driver.close().await;
}
#[tokio::test]
async fn actual_server_eof_and_credential_deadline_cannot_reconnect_or_extend_lifetime() {
    let mut server = Server::start().await;
    let mut driver = Driver::allocate(server.registration()).await.unwrap();
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    driver.bind_peer(peer.local_addr().unwrap()).await.unwrap();
    server.child.kill().unwrap();
    server.child.wait().unwrap();
    assert!(datagram(&mut driver).await.is_err());
    assert_eq!(driver.ready(), Err(Error::Closed));
    let server = Server::start().await;
    let r = Registration::loopback(
        server.address,
        "turn.fixture.invalid",
        roots(server.directory.path()),
        "fixture".into(),
        "synthetic-only".into(),
        Instant::now() + Duration::from_secs(1),
    )
    .unwrap();
    let mut driver = Driver::allocate(r).await.unwrap();
    driver.bind_peer(peer.local_addr().unwrap()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(driver.ready(), Err(Error::Deadline));
    assert_eq!(driver.send(b"expired").await, Err(Error::Closed));
}

#[tokio::test]
async fn actual_early_peer_datagrams_during_permission_and_channel_binding_never_mint_ready() {
    let server = Server::start().await;
    let mut driver = Driver::allocate(server.registration()).await.unwrap();
    let relay = driver.allocation().address;
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let address = peer.local_addr().unwrap();
    let sending = tokio::spawn(async move {
        for _ in 0..100 {
            peer.send_to(b"early-synthetic-control", relay)
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    });
    let result = driver.bind_peer(address).await;
    sending.abort();
    let _ = sending.await;
    result.expect(
        "early data cannot replace actual permission/channel acknowledgements or break binding",
    );
    driver.ready().unwrap();
    driver.close().await;
}

#[tokio::test]
async fn expired_protocol_channel_is_closed_instead_of_late_renewal_or_reconnect() {
    let server = Server::start().await;
    let mut driver = Driver::allocate(server.registration()).await.unwrap();
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    driver.bind_peer(peer.local_addr().unwrap()).await.unwrap();
    driver.origin = driver.origin.checked_sub(Duration::from_secs(601)).unwrap();
    assert!(driver.step(Duration::from_millis(10)).await.is_err());
    assert_eq!(driver.ready(), Err(Error::Closed));
    assert_eq!(
        driver.send(b"expired old generation").await,
        Err(Error::Closed)
    );
}

#[tokio::test]
async fn peer_permission_cannot_target_an_address_outside_the_actual_registered_relay() {
    let server = Server::start().await;
    let mut driver = Driver::allocate(server.registration()).await.unwrap();
    // A public address is not proof that it belongs to this TURN allocation.
    // No packet is sent to that address: this is only a permission request test.
    assert_eq!(
        driver.bind_peer("8.8.8.8:40000".parse().unwrap()).await,
        Err(Error::Peer)
    );
    assert!(driver.ready().is_err());
    driver.close().await;
}
