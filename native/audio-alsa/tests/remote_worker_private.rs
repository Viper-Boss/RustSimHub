// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
// Same production modules and independent TLS server as the SDK fixture. The
// SDK fixture's tests are intentionally also compiled; report duplicates apart.
#[path = "turn_private_tls.rs"]
mod tls;
pub use rustsimhub_audio_alsa::UfiDuplex;
pub use tls::codec;
use tls::{remote, rtc, turn};
#[path = "../src/control_socket.rs"]
mod remote_control_socket;
#[path = "../src/remote_worker/mod.rs"]
pub mod remote_worker;
#[path = "../src/trusted.rs"]
mod service_identity;
use rustsimhub_media::{
    local::now_ms,
    protocol,
    remote_worker::{self as wire, Client, Command},
    worker::Phase,
    Authority, Context, Direction, IoError, Pcm, Permit, Ticket,
};
use std::{
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::oneshot,
};
struct PcmState {
    opened: AtomicUsize,
    closed: AtomicUsize,
    writes: AtomicUsize,
    reads: AtomicUsize,
    blocked: AtomicBool,
    entered: AtomicBool,
}
impl PcmState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            opened: AtomicUsize::new(0),
            closed: AtomicUsize::new(0),
            writes: AtomicUsize::new(0),
            reads: AtomicUsize::new(0),
            blocked: AtomicBool::new(false),
            entered: AtomicBool::new(false),
        })
    }
}
struct ExternalPcm(Arc<PcmState>);
impl Pcm for ExternalPcm {
    fn read(&mut self, samples: &mut [i16]) -> Result<usize, IoError> {
        self.0.entered.store(true, Ordering::Release);
        while self.0.blocked.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(2));
        }
        self.0.reads.fetch_add(1, Ordering::AcqRel);
        samples.fill(123);
        Ok(samples.len())
    }
    fn write(&mut self, samples: &[i16]) -> Result<usize, IoError> {
        self.0.writes.fetch_add(1, Ordering::AcqRel);
        Ok(samples.len())
    }
    fn recover(&mut self, _: Direction) -> Result<(), IoError> {
        Ok(())
    }
    fn close(&mut self) {
        self.0.closed.fetch_add(1, Ordering::AcqRel);
    }
}
// External normal Core endpoint uses the actual Authority and protocol. This
// isn't a replacement business controller or proof of the full Core daemon.
struct CoreState {
    authority: Authority,
    context: Context,
    deny: bool,
    observed: Option<Permit>,
    redeemed: usize,
    pulses: usize,
}
struct Core {
    directory: tempfile::TempDir,
    state: Arc<Mutex<CoreState>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Core {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Core {
    async fn start(key: [u8; 32]) -> (Self, protocol::Credentials) {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("media.sock");
        let listener = UnixListener::bind(socket).unwrap();
        let context = Context {
            boot: "a".repeat(64),
            backend: "external-fixture".into(),
            line: 1,
            call: "c".repeat(36),
            call_revision: 1,
            lease_generation: 1,
            principal: "p".repeat(36),
            principal_generation: 1,
            peer_key: key,
            lease_until_ms: now_ms() + 30_000,
            active: true,
            media_scope: true,
            control_scope: true,
        };
        let mut authority = Authority::new();
        let ticket = authority.issue(context.clone(), now_ms()).unwrap();
        let credentials = protocol::Credentials {
            ticket: ticket.into_bytes(),
            peer_key: key,
            boot: context.boot.clone(),
        };
        let state = Arc::new(Mutex::new(CoreState {
            authority,
            context,
            deny: false,
            observed: None,
            redeemed: 0,
            pulses: 0,
        }));
        let shared = state.clone();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let expected_media =
                if std::env::var("RUSTSIMHUB_PRIVATE_WORKER_ROLE").as_deref() == Ok("core") {
                    984
                } else {
                    rustix::process::geteuid().as_raw()
                };
            assert_eq!(stream.peer_cred().unwrap().uid(), expected_media);
            let mut expected = 1;
            while let Ok(n) = stream.read_u32().await {
                if n == 0 || n as usize > protocol::MAX_FRAME {
                    break;
                }
                let mut bytes = vec![0; n as usize];
                if stream.read_exact(&mut bytes).await.is_err() {
                    break;
                }
                let Ok(request) = protocol::parse(&bytes) else {
                    break;
                };
                let reply = {
                    let mut s = shared.lock().unwrap();
                    if request.id != expected || s.deny {
                        s.authority.revoke();
                        break;
                    }
                    expected += 1;
                    let context = s.context.clone();
                    let permit = match request.command {
                        protocol::Command::Redeem { ticket, peer_key } => {
                            s.redeemed += 1;
                            match s.authority.consume(
                                Ticket::from_bytes(ticket),
                                &context,
                                &peer_key,
                                now_ms(),
                            ) {
                                Ok(p) => {
                                    s.observed = Some(p.clone());
                                    p
                                }
                                Err(_) => break,
                            }
                        }
                        protocol::Command::Pulse {} => {
                            s.pulses += 1;
                            if s.authority.refresh(Some(&context), now_ms()).is_err() {
                                break;
                            }
                            s.observed.as_ref().unwrap().clone()
                        }
                    };
                    protocol::Reply {
                        protocol: protocol::PROTOCOL,
                        id: request.id,
                        boot: context.boot,
                        until_ms: permit.deadline_ms(now_ms()).unwrap(),
                    }
                };
                let bytes = serde_json::to_vec(&reply).unwrap();
                if stream.write_u32(bytes.len() as u32).await.is_err()
                    || stream.write_all(&bytes).await.is_err()
                {
                    break;
                }
            }
            shared.lock().unwrap().authority.revoke();
        });
        (
            Self {
                directory,
                state,
                task,
            },
            credentials,
        )
    }
    fn socket(&self) -> PathBuf {
        self.directory.path().join("media.sock")
    }
}
struct Worker {
    directory: tempfile::TempDir,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}
impl Worker {
    async fn start(
        server: Arc<tls::Server>,
        core: &Core,
        pcm: Arc<PcmState>,
        observed: Arc<Mutex<Option<Permit>>>,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("remote.sock");
        let listener = UnixListener::bind(socket).unwrap();
        std::fs::set_permissions(
            directory.path().join("remote.sock"),
            std::fs::Permissions::from_mode(0o660),
        )
        .unwrap();
        let core_socket = core.socket();
        let uid = rustix::process::geteuid().as_raw();
        let (shutdown, stop) = oneshot::channel();
        let task = tokio::spawn(async move {
            remote_worker::serve_fixture(
                listener,
                core_socket,
                uid,
                uid,
                move |handle| {
                    if handle == "approved" {
                        Ok(server.registration())
                    } else {
                        Err(rustsimhub_media::worker::Failure::Denied)
                    }
                },
                move |permit| {
                    permit.check(now_ms()).unwrap();
                    *observed.lock().unwrap() = Some(permit.clone());
                    pcm.opened.fetch_add(1, Ordering::AcqRel);
                    Ok(Box::new(ExternalPcm(pcm.clone())))
                },
                async {
                    let _ = stop.await;
                },
            )
            .await
            .unwrap();
        });
        Self {
            directory,
            shutdown: Some(shutdown),
            task: Some(task),
        }
    }
    async fn finish(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        tokio::time::timeout(Duration::from_secs(3), self.task.take().unwrap())
            .await
            .unwrap()
            .unwrap();
    }
    fn socket(&self) -> PathBuf {
        self.directory.path().join("remote.sock")
    }
}
async fn until(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
async fn prepare(worker: &Worker, peer: &mut remote::Session, offer: String) -> Client {
    let mut client = Client::connect(
        &worker.socket(),
        rustix::process::geteuid().as_raw(),
        "a".repeat(32),
    )
    .await
    .unwrap();
    let key = tls::fingerprint(&offer);
    let reply = client
        .exchange(Command::Prepare {
            session: "a".repeat(32),
            relay_handle: "approved".into(),
            expected_peer: key,
            offer,
        })
        .await
        .unwrap();
    let answer = reply.result.unwrap().answer.unwrap();
    peer.accept_answer(&answer, tls::fingerprint(&answer))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            peer.service().await.unwrap();
            let reply = client
                .exchange(Command::Inspect {
                    session: "a".repeat(32),
                })
                .await
                .unwrap()
                .result
                .unwrap();
            assert!(!reply.pcm_opened);
            if reply.phase == Phase::Verified && peer.verified_peer().is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    client
}
#[tokio::test]
async fn worker_real_turn_and_core_ipc_open_pcm_only_after_original_ticket_and_revoke_on_control_eof(
) {
    let server = Arc::new(tls::Server::start().await);
    let (mut peer, offer) = remote::Session::offer(server.registration()).await.unwrap();
    let (core, credentials) = Core::start(tls::fingerprint(&offer)).await;
    let pcm = PcmState::new();
    let observed = Arc::new(Mutex::new(None));
    let worker = Worker::start(server, &core, pcm.clone(), observed.clone()).await;
    let mut client = prepare(&worker, &mut peer, offer).await;
    assert_eq!(pcm.opened.load(Ordering::Acquire), 0);
    client
        .exchange(Command::Authorize {
            session: "a".repeat(32),
            credentials,
        })
        .await
        .unwrap()
        .result
        .unwrap();
    until(|| pcm.reads.load(Ordering::Acquire) > 2 && pcm.writes.load(Ordering::Acquire) > 2).await;
    assert_eq!(core.state.lock().unwrap().redeemed, 1);
    assert!(observed
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .check(now_ms())
        .is_ok());
    drop(client);
    until(|| {
        observed
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .check(now_ms())
            .is_err()
    })
    .await;
    until(|| pcm.closed.load(Ordering::Acquire) == 1).await;
    assert!(core
        .state
        .lock()
        .unwrap()
        .observed
        .as_ref()
        .unwrap()
        .check(now_ms())
        .is_err());
    worker.finish().await;
    peer.close().await;
}
struct Unblock(Arc<PcmState>);
impl Drop for Unblock {
    fn drop(&mut self) {
        self.0.blocked.store(false, Ordering::Release);
    }
}
async fn exchange_raw(stream: &mut UnixStream, id: u64, command: Command) -> wire::Reply {
    let session = command.session().to_owned();
    let bytes = serde_json::to_vec(&wire::Request {
        protocol: wire::PROTOCOL,
        id,
        command,
    })
    .unwrap();
    stream.write_u32(bytes.len() as u32).await.unwrap();
    stream.write_all(&bytes).await.unwrap();
    let n = stream.read_u32().await.unwrap() as usize;
    assert!(n <= wire::MAX_FRAME);
    let mut bytes = vec![0; n];
    stream.read_exact(&mut bytes).await.unwrap();
    wire::parse_reply(&bytes, id, &session).unwrap()
}
#[tokio::test]
async fn worker_late_pcm_read_holds_exclusive_slot_after_eof_and_never_writes_late_speech() {
    let server = Arc::new(tls::Server::start().await);
    let (mut peer, offer) = remote::Session::offer(server.registration()).await.unwrap();
    let (core, credentials) = Core::start(tls::fingerprint(&offer)).await;
    let pcm = PcmState::new();
    pcm.blocked.store(true, Ordering::Release);
    let _unblock = Unblock(pcm.clone());
    let observed = Arc::new(Mutex::new(None));
    let worker = Worker::start(server, &core, pcm.clone(), observed.clone()).await;
    let mut client = prepare(&worker, &mut peer, offer).await;
    client
        .exchange(Command::Authorize {
            session: "a".repeat(32),
            credentials,
        })
        .await
        .unwrap()
        .result
        .unwrap();
    until(|| pcm.entered.load(Ordering::Acquire)).await;
    drop(client);
    until(|| {
        observed
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .check(now_ms())
            .is_err()
    })
    .await;
    assert_eq!(pcm.closed.load(Ordering::Acquire), 0);
    // A second actual same-UID connection has no authority over the original
    // owner, even while the external read is late after revocation.
    let mut intruder = UnixStream::connect(worker.socket()).await.unwrap();
    let request = serde_json::to_vec(&wire::Request {
        protocol: 2,
        id: 1,
        command: Command::Stop {
            session: "a".repeat(32),
        },
    })
    .unwrap();
    let _ = intruder.write_u32(request.len() as u32).await;
    let _ = intruder.write_all(&request).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(200), intruder.read_u32())
            .await
            .unwrap()
            .is_err()
    );
    assert_eq!(pcm.closed.load(Ordering::Acquire), 0);
    pcm.blocked.store(false, Ordering::Release);
    until(|| pcm.closed.load(Ordering::Acquire) == 1).await;
    assert_eq!(pcm.writes.load(Ordering::Acquire), 0);
    assert_eq!(pcm.opened.load(Ordering::Acquire), 1);
    worker.finish().await;
    peer.close().await;
}
#[tokio::test]
async fn worker_original_core_binding_revocations_all_close_the_actual_pcm() {
    let server = Arc::new(tls::Server::start().await);
    for case in 0..13 {
        let (mut peer, offer) = remote::Session::offer(server.registration()).await.unwrap();
        let (core, credentials) = Core::start(tls::fingerprint(&offer)).await;
        let pcm = PcmState::new();
        let observed = Arc::new(Mutex::new(None));
        let worker = Worker::start(server.clone(), &core, pcm.clone(), observed.clone()).await;
        let mut client = prepare(&worker, &mut peer, offer).await;
        client
            .exchange(Command::Authorize {
                session: "a".repeat(32),
                credentials,
            })
            .await
            .unwrap()
            .result
            .unwrap();
        until(|| pcm.opened.load(Ordering::Acquire) == 1 && pcm.reads.load(Ordering::Acquire) > 0)
            .await;
        {
            let mut s = core.state.lock().unwrap();
            match case {
                0 => s.context.boot = "b".repeat(64),
                1 => s.context.line += 1,
                2 => s.context.call = "d".repeat(36),
                3 => s.context.call_revision += 1,
                4 => s.context.lease_generation += 1,
                5 => s.context.principal = "q".repeat(36),
                6 => s.context.principal_generation += 1,
                7 => s.context.peer_key = [8; 32],
                8 => s.context.media_scope = false,
                9 => s.context.control_scope = false,
                10 => s.context.active = false,
                11 => s.context.lease_until_ms = now_ms(),
                12 => s.deny = true,
                _ => unreachable!(),
            }
        }
        let started = Instant::now();
        until(|| {
            observed
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .check(now_ms())
                .is_err()
        })
        .await;
        assert!(
            started.elapsed() < Duration::from_millis(900),
            "original 1s Permit budget: case {case}"
        );
        until(|| pcm.closed.load(Ordering::Acquire) == 1).await;
        let writes = pcm.writes.load(Ordering::Acquire);
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(
            pcm.writes.load(Ordering::Acquire),
            writes,
            "no late write for binding case {case}"
        );
        assert_eq!(pcm.opened.load(Ordering::Acquire), 1);
        drop(client);
        worker.finish().await;
        peer.close().await;
    }
}
#[tokio::test]
async fn worker_wrong_ticket_boot_or_verified_peer_never_opens_pcm() {
    let server = Arc::new(tls::Server::start().await);
    for case in 0..3 {
        let (mut peer, offer) = remote::Session::offer(server.registration()).await.unwrap();
        let (core, mut credentials) = Core::start(tls::fingerprint(&offer)).await;
        let pcm = PcmState::new();
        let observed = Arc::new(Mutex::new(None));
        let worker = Worker::start(server.clone(), &core, pcm.clone(), observed).await;
        let mut client = prepare(&worker, &mut peer, offer).await;
        match case {
            0 => credentials.ticket = [0; 32],
            1 => credentials.boot = "b".repeat(64),
            2 => credentials.peer_key = [9; 32],
            _ => unreachable!(),
        }
        assert!(client
            .exchange(Command::Authorize {
                session: "a".repeat(32),
                credentials
            })
            .await
            .unwrap()
            .result
            .is_err());
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert_eq!(pcm.opened.load(Ordering::Acquire), 0);
        assert_eq!(
            core.state.lock().unwrap().redeemed,
            if case == 2 { 0 } else { 1 }
        );
        drop(client);
        worker.finish().await;
        peer.close().await;
    }
}
#[tokio::test]
async fn worker_rejects_call_ops_oversize_unknown_endpoint_and_invalid_sdp() {
    let server = Arc::new(tls::Server::start().await);
    let (core, _credentials) = Core::start([7; 32]).await;
    let pcm = PcmState::new();
    let observed = Arc::new(Mutex::new(None));
    let worker = Worker::start(server, &core, pcm.clone(), observed).await;
    for bytes in [
        b"{\"protocol\":2,\"id\":1,\"command\":{\"op\":\"stop_all\",\"session\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}}".to_vec(),
        serde_json::to_vec(&serde_json::json!({"protocol":2,"id":1,"command":{"op":"prepare","session":"a".repeat(32),"relay_handle":"approved","expected_peer":vec![7;32],"offer":"not SDP","server":"127.0.0.1:1234"}})).unwrap(),
        vec![b' ';wire::MAX_FRAME+1],
    ] {
        let mut raw=UnixStream::connect(worker.socket()).await.unwrap();
        let _=raw.write_u32(bytes.len() as u32).await;
        for chunk in bytes.chunks(7) { if raw.write_all(chunk).await.is_err() {break;} }
        assert!(tokio::time::timeout(Duration::from_millis(300),raw.read_u32()).await.unwrap().is_err());
    }
    // A valid, fragmented Prepare obtains no authority from its session string;
    // malformed SDP is rejected, without touching the normal Core endpoint.
    let mut raw = UnixStream::connect(worker.socket()).await.unwrap();
    let result = exchange_raw(
        &mut raw,
        1,
        Command::Prepare {
            session: "a".repeat(32),
            relay_handle: "approved".into(),
            expected_peer: [7; 32],
            offer: "not SDP".into(),
        },
    )
    .await;
    assert!(result.result.is_err());
    assert_eq!(core.state.lock().unwrap().redeemed, 0);
    assert_eq!(pcm.opened.load(Ordering::Acquire), 0);
    drop(raw);
    worker.finish().await;
}
#[test]
fn worker_production_entry_rejects_the_fixture_uid_without_using_its_mock_factories() {
    assert_ne!(rustix::process::geteuid().as_raw(), 984);
    assert!(remote_worker::run(b"{}").is_err());
    assert!(!service_identity::core_uid(u32::MAX));
}
#[tokio::test]
async fn worker_actual_relay_opus_both_directions_and_stop_only_local_media() {
    let server = Arc::new(tls::Server::start().await);
    let (mut peer, offer) = remote::Session::offer(server.registration()).await.unwrap();
    let (core, credentials) = Core::start(tls::fingerprint(&offer)).await;
    let pcm = PcmState::new();
    let observed = Arc::new(Mutex::new(None));
    let worker = Worker::start(server, &core, pcm.clone(), observed.clone()).await;
    let mut client = prepare(&worker, &mut peer, offer).await;
    let key = peer.verified_peer().unwrap().fingerprint();
    let (mut peer_authority, context, permit) = tls::permit(key);
    peer.authorize(permit, key).unwrap();
    client
        .exchange(Command::Authorize {
            session: "a".repeat(32),
            credentials,
        })
        .await
        .unwrap()
        .result
        .unwrap();
    let mut sequence = 0;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            peer_authority.refresh(Some(&context), now_ms()).unwrap();
            peer.service().await.unwrap();
            peer.send(&rustsimhub_media::Frame {
                samples: [456; 320],
                sequence,
                at_ms: now_ms(),
            })
            .await
            .unwrap();
            sequence += 1;
            let snapshot = client
                .exchange(Command::Inspect {
                    session: "a".repeat(32),
                })
                .await
                .unwrap()
                .result
                .unwrap();
            if snapshot.counters.sent > 0
                && snapshot.counters.received > 0
                && peer.receive().unwrap().is_some()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let snapshot = client
        .exchange(Command::Stop {
            session: "a".repeat(32),
        })
        .await
        .unwrap()
        .result
        .unwrap();
    assert!(matches!(snapshot.phase, Phase::Closing | Phase::Closed));
    assert!(snapshot.release == wire::Release::Unknown);
    assert!(observed
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .check(now_ms())
        .is_err());
    until(|| pcm.closed.load(Ordering::Acquire) == 1).await;
    // The external normal call/line/lease context is unchanged by local Stop.
    {
        let state = core.state.lock().unwrap();
        assert!(state.context.active);
        assert_eq!(state.context.call_revision, 1);
        assert_eq!(state.context.lease_generation, 1);
    }
    drop(client);
    worker.finish().await;
    peer.close().await;
}
#[tokio::test]
async fn worker_actual_same_connection_replayed_ids_and_cross_session_are_denied() {
    let server = Arc::new(tls::Server::start().await);
    for replay in [true, false] {
        let (mut peer, offer) = remote::Session::offer(server.registration()).await.unwrap();
        let (core, _credentials) = Core::start(tls::fingerprint(&offer)).await;
        let pcm = PcmState::new();
        let observed = Arc::new(Mutex::new(None));
        let worker = Worker::start(server.clone(), &core, pcm.clone(), observed).await;
        let mut raw = UnixStream::connect(worker.socket()).await.unwrap();
        let key = tls::fingerprint(&offer);
        let reply = exchange_raw(
            &mut raw,
            1,
            Command::Prepare {
                session: "a".repeat(32),
                relay_handle: "approved".into(),
                expected_peer: key,
                offer,
            },
        )
        .await;
        let answer = reply.result.unwrap().answer.unwrap();
        peer.accept_answer(&answer, tls::fingerprint(&answer))
            .await
            .unwrap();
        let bad = wire::Request {
            protocol: 2,
            id: if replay { 1 } else { 2 },
            command: Command::Inspect {
                session: if replay {
                    "a".repeat(32)
                } else {
                    "b".repeat(32)
                },
            },
        };
        let bytes = serde_json::to_vec(&bad).unwrap();
        raw.write_u32(bytes.len() as u32).await.unwrap();
        raw.write_all(&bytes).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(300), raw.read_u32())
                .await
                .unwrap()
                .is_err()
        );
        assert_eq!(core.state.lock().unwrap().redeemed, 0);
        assert_eq!(pcm.opened.load(Ordering::Acquire), 0);
        drop(raw);
        worker.finish().await;
        peer.close().await;
    }
}
fn publish(path: impl AsRef<std::path::Path>, value: serde_json::Value) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let path = path.as_ref();
    let next = path.with_extension("next");
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o640)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&next)
        .unwrap();
    file.write_all(&serde_json::to_vec(&value).unwrap())
        .unwrap();
    file.sync_all().unwrap();
    std::fs::rename(next, path).unwrap();
}
async fn fixture_file(path: &str) -> serde_json::Value {
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            if let Ok(bytes) = std::fs::read(path) {
                return serde_json::from_slice(&bytes).unwrap();
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}
fn no_capabilities() {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    assert!(status.lines().any(|l| l == "CapEff:\t0000000000000000"));
}
#[test]
#[ignore = "explicit disposable image, synthetic NSS and actual UIDs 984/985/986 only"]
fn worker_actual_uid_role() {
    no_capabilities();
    let role = std::env::var("RUSTSIMHUB_PRIVATE_WORKER_ROLE").unwrap();
    let uid = rustix::process::geteuid().as_raw();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        match role.as_str() {
            "core"=>{
                assert_eq!(uid,986);assert!(service_identity::service_uid("rustsimhub-core",uid));
                let key: [u8;32]=serde_json::from_value(fixture_file("/fixture/edge/offer.json").await["key"].clone()).unwrap();
                let (core,credentials)=Core::start(key).await;
                std::fs::set_permissions(core.directory.path(),std::fs::Permissions::from_mode(0o750)).unwrap();
                std::fs::set_permissions(core.socket(),std::fs::Permissions::from_mode(0o660)).unwrap();
                publish("/fixture/core/ready.json",serde_json::json!({"socket":core.socket(),"credentials":credentials}));
                fixture_file("/fixture/edge/revoke.json").await;
                core.state.lock().unwrap().context.call_revision+=1;
                until(||core.state.lock().unwrap().observed.as_ref().is_some_and(|p|p.check(now_ms()).is_err())).await;
                let state=core.state.lock().unwrap();assert_eq!(state.redeemed,1);assert!(state.pulses>0);
                publish("/fixture/core/done.json",serde_json::json!({"uid":uid,"redeemed":state.redeemed,"pulses":state.pulses,"normal_authority_revoked":true,"actual_media_peer_uid":984}));
            }
            "media"=>{
                assert_eq!(uid,984);assert!(service_identity::service_uid("rustsimhub-media",uid));
                assert!(service_identity::service_uid("rustsimhub-core",986));assert!(service_identity::service_uid("rustsimhub-edge",985));
                let registration=fixture_file("/fixture/edge/offer.json").await;
                let address:std::net::SocketAddr=serde_json::from_value(registration["address"].clone()).unwrap();
                let ca:Vec<u8>=serde_json::from_value(registration["ca_der"].clone()).unwrap();
                let mut roots=tokio_rustls::rustls::RootCertStore::empty();roots.add(tokio_rustls::rustls::pki_types::CertificateDer::from(ca)).unwrap();
                let core=fixture_file("/fixture/core/ready.json").await;
                let core_socket=PathBuf::from(core["socket"].as_str().unwrap());
                let endpoint=if std::env::var("RUSTSIMHUB_NORMAL_CORE_FIXTURE").as_deref()==Ok("1") {"/run/rustsimhub/media/remote.sock"}else{"/fixture/media/remote.sock"};
                let (listener,_guard)=remote_control_socket::bind(std::path::Path::new(endpoint)).unwrap();
                publish("/fixture/media/ready.json",serde_json::json!({"uid":uid}));
                let pcm=PcmState::new();let factory_pcm=pcm.clone();let observed=Arc::new(Mutex::new(None::<Permit>));let factory_observed=observed.clone();
                let deadline=Instant::now()+Duration::from_secs(25);
                remote_worker::serve_fixture(listener,core_socket,986,985,
                    move |handle| {
                        if handle!="approved" {return Err(rustsimhub_media::worker::Failure::Denied);}
                        turn::Registration::loopback(address,"turn.fixture.invalid",roots.clone(),"fixture".into(),"synthetic-only".into(),deadline).map_err(|_|rustsimhub_media::worker::Failure::Denied)
                    },
                    move |permit| {permit.check(now_ms()).unwrap();*factory_observed.lock().unwrap()=Some(permit.clone());factory_pcm.opened.fetch_add(1,Ordering::AcqRel);Ok(Box::new(ExternalPcm(factory_pcm.clone())))},
                    async {
                        tokio::time::timeout(Duration::from_secs(12),async {
                            loop {
                                if observed.lock().unwrap().as_ref().is_some_and(|p|p.check(now_ms()).is_err()) && pcm.closed.load(Ordering::Acquire)==1 {break;}
                                tokio::time::sleep(Duration::from_millis(5)).await;
                            }
                        }).await.unwrap();
                        assert_eq!(pcm.opened.load(Ordering::Acquire),1);assert!(pcm.writes.load(Ordering::Acquire)>0);
                        publish("/fixture/media/done.json",serde_json::json!({"uid":uid,"opened":pcm.opened.load(Ordering::Acquire),"closed":pcm.closed.load(Ordering::Acquire),"writes":pcm.writes.load(Ordering::Acquire),"mirror_revoked":true}));
                    }).await.unwrap();
            }
            "edge"=>{
                assert_eq!(uid,985);assert!(service_identity::service_uid("rustsimhub-edge",uid));
                let server=tls::Server::start().await;
                let (mut peer,offer)=remote::Session::offer(server.registration()).await.unwrap();
                let key=tls::fingerprint(&offer);
                publish("/fixture/edge/offer.json",serde_json::json!({"key":key,"address":server.address,"ca_der":server.ca_der()}));
                let core=fixture_file("/fixture/core/ready.json").await;
                fixture_file("/fixture/media/ready.json").await;
                fixture_file("/fixture/start-edge.json").await;
                let credentials:protocol::Credentials=serde_json::from_value(core["credentials"].clone()).unwrap();
                let mut client=Client::connect(std::path::Path::new("/fixture/media/remote.sock"),984,"a".repeat(32)).await.unwrap();
                let answer=client.exchange(Command::Prepare {session:"a".repeat(32),relay_handle:"approved".into(),expected_peer:key,offer}).await.unwrap().result.unwrap().answer.unwrap();
                peer.accept_answer(&answer,tls::fingerprint(&answer)).await.unwrap();
                tokio::time::timeout(Duration::from_secs(3),async {loop {
                    peer.service().await.unwrap();
                    let snapshot=client.exchange(Command::Inspect {session:"a".repeat(32)}).await.unwrap().result.unwrap();assert!(!snapshot.pcm_opened);
                    if snapshot.phase==Phase::Verified && peer.verified_peer().is_some() {break;}tokio::time::sleep(Duration::from_millis(2)).await;
                }}).await.unwrap();
                let peer_key=peer.verified_peer().unwrap().fingerprint();let (mut authority,context,permit)=tls::permit(peer_key);peer.authorize(permit,peer_key).unwrap();
                client.exchange(Command::Authorize {session:"a".repeat(32),credentials}).await.unwrap().result.unwrap();
                let mut sequence=0;
                tokio::time::timeout(Duration::from_secs(3),async {loop {
                    authority.refresh(Some(&context),now_ms()).unwrap();peer.service().await.unwrap();
                    peer.send(&rustsimhub_media::Frame {samples:[456;320],sequence,at_ms:now_ms()}).await.unwrap();sequence+=1;
                    let snapshot=client.exchange(Command::Inspect {session:"a".repeat(32)}).await.unwrap().result.unwrap();
                    if snapshot.counters.sent>0 && snapshot.counters.received>0 && peer.receive().unwrap().is_some() {break;}tokio::time::sleep(Duration::from_millis(20)).await;
                }}).await.unwrap();
                publish("/fixture/edge/revoke.json",serde_json::json!({"revoke":true}));
                let media=fixture_file("/fixture/media/done.json").await;let core=fixture_file("/fixture/core/done.json").await;
                assert_eq!(media["uid"],984);assert_eq!(core["uid"],986);
                publish("/fixture/edge/done.json",serde_json::json!({"uid":uid,"forced_relay_opus_both_directions":true,"media_closed":media["closed"],"actual_core_peer_uid":986}));
                drop(client);peer.close().await;
            }
            "denied"=>{
                assert_ne!(uid,985);
                let mut client=Client::connect(std::path::Path::new("/fixture/media/remote.sock"),984,"b".repeat(32)).await.unwrap();
                assert!(client.exchange(Command::Prepare {session:"b".repeat(32),relay_handle:"approved".into(),expected_peer:[7;32],offer:"not SDP".into()}).await.is_err());
            }
            _=>panic!("unknown isolated fixture role"),
        }
    });
}
#[tokio::test]
async fn worker_cancelled_prepare_closes_the_actual_pending_tls_fd_without_replay() {
    let server = tls::Server::start().await;
    let (peer, offer) = remote::Session::offer(server.registration()).await.unwrap();
    let key = tls::fingerprint(&offer);
    let blackhole = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = blackhole.local_addr().unwrap();
    let mut roots = tokio_rustls::rustls::RootCertStore::empty();
    roots
        .add(tokio_rustls::rustls::pki_types::CertificateDer::from(
            server.ca_der(),
        ))
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("remote.sock");
    let listener = UnixListener::bind(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o660)).unwrap();
    let (stop, stopped) = oneshot::channel();
    let uid = rustix::process::geteuid().as_raw();
    let factory_called = Arc::new(AtomicBool::new(false));
    let called = factory_called.clone();
    let host = tokio::spawn(async move {
        let deadline = Instant::now() + Duration::from_secs(10);
        remote_worker::serve_fixture(
            listener,
            PathBuf::from("/unused-core.sock"),
            uid,
            uid,
            move |_| {
                turn::Registration::loopback(
                    address,
                    "turn.fixture.invalid",
                    roots.clone(),
                    "fixture".into(),
                    "synthetic-only".into(),
                    deadline,
                )
                .map_err(|_| rustsimhub_media::worker::Failure::Denied)
            },
            move |_| {
                called.store(true, Ordering::Release);
                Err(rustsimhub_media::worker::Failure::Unavailable)
            },
            async {
                let _ = stopped.await;
            },
        )
        .await
        .unwrap();
    });
    let (hello, received) = oneshot::channel();
    let backend = tokio::spawn(async move {
        let (mut socket, _) = blackhole.accept().await.unwrap();
        let mut buffer = [0; 4096];
        assert!(socket.read(&mut buffer).await.unwrap() > 0);
        hello.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if socket.read(&mut buffer).await.unwrap() == 0 {
                    break;
                }
            }
        })
        .await
        .unwrap();
    });
    let mut client = Client::connect(&path, uid, "a".repeat(32)).await.unwrap();
    {
        let prepare = client.exchange(Command::Prepare {
            session: "a".repeat(32),
            relay_handle: "approved".into(),
            expected_peer: key,
            offer,
        });
        tokio::pin!(prepare);
        tokio::select! { result=&mut prepare=>panic!("pending ordinary TLS unexpectedly completed: {}",result.is_ok()), result=received=>{result.unwrap();} }
    }
    backend.await.unwrap();
    assert!(client
        .exchange(Command::Inspect {
            session: "a".repeat(32)
        })
        .await
        .is_err());
    assert!(!factory_called.load(Ordering::Acquire));
    stop.send(()).unwrap();
    host.await.unwrap();
    peer.close().await;
}
#[tokio::test]
async fn worker_shutdown_revokes_before_waiting_for_a_late_pcm_owner() {
    let server = Arc::new(tls::Server::start().await);
    let (mut peer, offer) = remote::Session::offer(server.registration()).await.unwrap();
    let (core, credentials) = Core::start(tls::fingerprint(&offer)).await;
    let pcm = PcmState::new();
    pcm.blocked.store(true, Ordering::Release);
    let _unblock = Unblock(pcm.clone());
    let observed = Arc::new(Mutex::new(None));
    let mut worker = Worker::start(server, &core, pcm.clone(), observed.clone()).await;
    let mut client = prepare(&worker, &mut peer, offer).await;
    client
        .exchange(Command::Authorize {
            session: "a".repeat(32),
            credentials,
        })
        .await
        .unwrap()
        .result
        .unwrap();
    until(|| pcm.entered.load(Ordering::Acquire)).await;
    worker.shutdown.take().unwrap().send(()).unwrap();
    until(|| {
        observed
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .check(now_ms())
            .is_err()
    })
    .await;
    assert!(!worker.task.as_ref().unwrap().is_finished());
    assert_eq!(pcm.closed.load(Ordering::Acquire), 0);
    pcm.blocked.store(false, Ordering::Release);
    worker.finish().await;
    assert_eq!(pcm.closed.load(Ordering::Acquire), 1);
    assert_eq!(pcm.writes.load(Ordering::Acquire), 0);
    drop(client);
    peer.close().await;
}

#[path = "normal_remote_host.rs"]
mod normal_host;
