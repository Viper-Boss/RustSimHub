// SPDX-License-Identifier: AGPL-3.0-only
//! One persistent signaling worker; verified DTLS and live Core permission precede ALSA.
use rustsimhub_audio_alsa::rtc::Session;
use rustsimhub_media::{
    local::{self, Client},
    protocol::Credentials,
    worker::{self as wire, Command, Counters, Failure, Phase, Snapshot},
    Pump,
};
use serde::Deserialize;
use std::{
    collections::VecDeque,
    net::{IpAddr, UdpSocket},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    task::JoinSet,
    time::timeout,
};
type Result<T> = std::result::Result<T, Failure>;
const CORE_UID: u32 = 986;
const EDGE_UID: u32 = 985;
const MEDIA_UID: u32 = 984;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    control_socket: PathBuf,
    core_socket: PathBuf,
    bind_ip: IpAddr,
}
struct Shared {
    phase: Phase,
    peer: Option<[u8; 32]>,
    pcm_opened: bool,
    failure: Option<Failure>,
    counters: Counters,
}
impl Shared {
    fn snapshot(&self, id: &str, answer: Option<String>) -> Snapshot {
        Snapshot {
            session: id.to_owned(),
            phase: self.phase,
            peer_key: self.peer,
            answer,
            pcm_opened: self.pcm_opened,
            failure: self.failure,
            counters: self.counters.clone(),
        }
    }
}
struct Active {
    id: String,
    peer_ip: IpAddr,
    offer: String,
    answer: String,
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    authorization: mpsc::SyncSender<Credentials>,
    thread: Option<JoinHandle<()>>,
}
impl Active {
    fn stop(&self) {
        self.stop.store(true, Ordering::Release);
        if let Ok(mut shared) = self.shared.lock() {
            if shared.phase != Phase::Closed {
                shared.phase = Phase::Closing;
            }
        }
    }
    fn snapshot(&self, answer: bool) -> Result<Snapshot> {
        self.shared
            .lock()
            .map(|s| s.snapshot(&self.id, answer.then(|| self.answer.clone())))
            .map_err(|_| Failure::Unavailable)
    }
    fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }
}
impl Drop for Active {
    fn drop(&mut self) {
        self.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
struct Manager {
    active: Option<Active>,
    used: VecDeque<String>,
    core_socket: PathBuf,
    bind_ip: IpAddr,
}
impl Manager {
    fn new(core_socket: PathBuf, bind_ip: IpAddr) -> Self {
        Self {
            active: None,
            used: VecDeque::new(),
            core_socket,
            bind_ip,
        }
    }
    fn prepare(&mut self, id: String, peer_ip: IpAddr, offer: String) -> Result<Snapshot> {
        if let Some(active) = &self.active {
            if active.id == id {
                if active.peer_ip != peer_ip || active.offer != offer {
                    return Err(Failure::Denied);
                }
                return active.snapshot(true);
            }
            if !active.finished() {
                return Err(Failure::Busy);
            }
        }
        if self.used.contains(&id) {
            return Err(Failure::Denied);
        }
        self.active = None; // joins a terminal thread before another can own the PCM.
        let socket = UdpSocket::bind((self.bind_ip, 0)).map_err(|_| Failure::Unavailable)?;
        let (session, answer) =
            Session::answer(&socket, peer_ip, &offer).map_err(|_| Failure::BadRequest)?;
        let shared = Arc::new(Mutex::new(Shared {
            phase: Phase::Handshaking,
            peer: None,
            pcm_opened: false,
            failure: None,
            counters: Counters::default(),
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (authorization, receiver) = mpsc::sync_channel(1);
        let worker_shared = shared.clone();
        let worker_stop = stop.clone();
        let core_socket = self.core_socket.clone();
        let handle = thread::Builder::new()
            .name("simhub-media-session".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    session_loop(
                        session,
                        socket,
                        core_socket,
                        receiver,
                        &worker_stop,
                        &worker_shared,
                    )
                }));
                if let Ok(mut s) = worker_shared.lock() {
                    s.phase = Phase::Closed;
                    s.failure = match result {
                        Ok(Ok(())) => None,
                        Ok(Err(e)) => Some(e),
                        Err(_) => Some(Failure::Unavailable),
                    };
                }
            })
            .map_err(|_| Failure::Unavailable)?;
        let active = Active {
            id: id.clone(),
            peer_ip,
            offer,
            answer,
            shared,
            stop,
            authorization,
            thread: Some(handle),
        };
        let result = active.snapshot(true);
        self.active = Some(active);
        self.used.push_back(id);
        if self.used.len() > 32 {
            self.used.pop_front();
        }
        result
    }
    fn active(&self, id: &str) -> Result<&Active> {
        self.active
            .as_ref()
            .filter(|a| a.id == id)
            .ok_or(Failure::Closed)
    }
    fn process(&mut self, command: Command) -> Result<Snapshot> {
        match command {
            Command::Prepare {
                session,
                peer_ip,
                offer,
            } => self.prepare(session, peer_ip, offer),
            Command::Inspect { session } => self.active(&session)?.snapshot(false),
            Command::Stop { session } => {
                let active = self.active(&session)?;
                active.stop();
                active.snapshot(false)
            }
            Command::Authorize {
                session,
                credentials,
            } => {
                let active = self.active(&session)?;
                let accepted = {
                    let mut s = active.shared.lock().map_err(|_| Failure::Unavailable)?;
                    if s.phase == Phase::Verified
                        && s.peer == Some(credentials.peer_key)
                        && !active.stop.load(Ordering::Acquire)
                    {
                        s.phase = Phase::Authorizing;
                        true
                    } else {
                        false
                    }
                };
                if !accepted {
                    active.stop();
                    return Err(Failure::Denied);
                }
                if active.authorization.try_send(credentials).is_err() {
                    active.stop();
                    return Err(Failure::Closed);
                }
                active.snapshot(false)
            }
        }
    }
}

fn stopped(stop: &AtomicBool) -> Result<()> {
    if stop.load(Ordering::Acquire) {
        Err(Failure::Closed)
    } else {
        Ok(())
    }
}
fn pcm_failure(error: rustsimhub_media::Error) -> Failure {
    match error {
        rustsimhub_media::Error::Denied | rustsimhub_media::Error::Expired => Failure::Denied,
        _ => Failure::Unavailable,
    }
}
fn finish_audio(result: Result<()>, requested_stop: bool) -> Result<()> {
    match result {
        Err(Failure::Closed | Failure::Denied) if requested_stop => Ok(()),
        other => other,
    }
}
fn session_loop(
    mut session: Session,
    socket: UdpSocket,
    core_socket: PathBuf,
    authorization: mpsc::Receiver<Credentials>,
    stop: &Arc<AtomicBool>,
    shared: &Arc<Mutex<Shared>>,
) -> Result<()> {
    let handshake_end = Instant::now() + Duration::from_secs(15);
    while session.verified_peer().is_none() {
        stopped(stop)?;
        if Instant::now() >= handshake_end {
            return Err(Failure::Timeout);
        }
        session
            .service(&socket, local::now_ms())
            .map_err(|_| Failure::Closed)?;
        thread::sleep(Duration::from_millis(2));
    }
    let peer = session.verified_peer().ok_or(Failure::Denied)?;
    {
        let mut s = shared.lock().map_err(|_| Failure::Unavailable)?;
        s.peer = Some(peer);
        if s.phase == Phase::Handshaking {
            s.phase = Phase::Verified;
        }
    }
    let end = Instant::now() + Duration::from_secs(10);
    let credentials = loop {
        stopped(stop)?;
        if Instant::now() >= end {
            return Err(Failure::Timeout);
        }
        session
            .service(&socket, local::now_ms())
            .map_err(|_| Failure::Closed)?;
        match authorization.try_recv() {
            Ok(c) => break c,
            Err(mpsc::TryRecvError::Empty) => thread::sleep(Duration::from_millis(2)),
            Err(_) => return Err(Failure::Closed),
        }
    };
    if credentials.peer_key != peer {
        return Err(Failure::Denied);
    }
    stopped(stop)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| Failure::Unavailable)?;
    runtime.block_on(async {
        let mut client=Client::redeem(&core_socket,CORE_UID,credentials).await.map_err(|_|Failure::Denied)?;
        stopped(stop)?;
        let permit=client.permit();
        session.authorize(permit.clone(),peer,local::now_ms()).map_err(|_|Failure::Denied)?;
        let audio_stop=stop.clone();let audio_shared=shared.clone();
        let audio=thread::Builder::new().name("simhub-pcm".into()).spawn(move|| {
            stopped(&audio_stop)?;
            let device=rustsimhub_audio_alsa::UfiDuplex::open(&permit,local::now_ms).map_err(|_|Failure::Unavailable)?;
            let mut pump=Pump::new(device,permit,local::now_ms()).map_err(|_|Failure::Denied)?;
            {let mut s=audio_shared.lock().map_err(|_|Failure::Unavailable)?;s.pcm_opened=true;if !audio_stop.load(Ordering::Acquire){s.phase=Phase::Active;}}
            let mut next_stats=Instant::now();
            let result=(|| {
                loop {
                    stopped(&audio_stop)?;
                    session.service(&socket,local::now_ms()).map_err(|_|Failure::Closed)?;
                    pump.step(local::now_ms()).map_err(pcm_failure)?;
                    while let Some(frame)=pump.capture(local::now_ms()).map_err(pcm_failure)? {
                        session.send(&socket,&frame,local::now_ms()).map_err(|_|Failure::Closed)?;
                    }
                    while let Some(frame)=session.receive(local::now_ms()).map_err(|_|Failure::Closed)? {
                        pump.enqueue(frame,local::now_ms()).map_err(pcm_failure)?;
                    }
                    if Instant::now()>=next_stats {update_stats(&audio_shared,&session,pump.stats());next_stats=Instant::now()+Duration::from_millis(100);}
                    thread::sleep(Duration::from_millis(2));
                }
            })();
            pump.close();session.close();update_stats(&audio_shared,&session,pump.stats());result
        }).map_err(|_|Failure::Unavailable)?;
        let mut pulse=tokio::time::interval(Duration::from_millis(250));pulse.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut failure=None;
        while !audio.is_finished(){
            tokio::select! {
                _=tokio::time::sleep(Duration::from_millis(20))=>{if stop.load(Ordering::Acquire){break;}},
                _=pulse.tick()=>{if client.pulse().await.is_err(){failure=Some(Failure::Denied);break;}}
            }
        }
        let requested_stop=stop.load(Ordering::Acquire);
        drop(client); // invalidates the PCM and codec permits before the join.
        stop.store(true,Ordering::Release);
        let result=audio.join().map_err(|_|Failure::Unavailable)?;
        if let Some(f)=failure {return Err(f);}
        finish_audio(result,requested_stop)
    })
}
fn update_stats(shared: &Arc<Mutex<Shared>>, session: &Session, stats: rustsimhub_media::Stats) {
    if let Ok(mut s) = shared.lock() {
        s.counters = Counters {
            sent: session.sent_frames,
            received: session.received_frames,
            captured: stats.captured_frames,
            played: stats.played_frames,
            dropped: stats.dropped_frames + session.dropped_frames,
            xruns: stats.xruns,
        };
    }
}

async fn handle(mut stream: UnixStream, manager: Arc<Mutex<Manager>>) -> Result<()> {
    let request = timeout(Duration::from_secs(1), async {
        let length = stream.read_u32().await.map_err(|_| Failure::Closed)? as usize;
        if length == 0 || length > wire::MAX_FRAME {
            return Err(Failure::BadRequest);
        }
        let mut body = vec![0; length];
        stream
            .read_exact(&mut body)
            .await
            .map_err(|_| Failure::Closed)?;
        wire::parse(&body).map_err(|_| Failure::BadRequest)
    })
    .await
    .map_err(|_| Failure::Timeout)??;
    let result = manager
        .lock()
        .map_err(|_| Failure::Unavailable)?
        .process(request.command);
    let bytes = serde_json::to_vec(&wire::Reply {
        protocol: wire::PROTOCOL,
        id: request.id,
        result,
    })
    .map_err(|_| Failure::Unavailable)?;
    if bytes.len() > wire::MAX_FRAME {
        return Err(Failure::Unavailable);
    }
    timeout(Duration::from_secs(1), async {
        stream
            .write_u32(bytes.len() as u32)
            .await
            .map_err(|_| Failure::Closed)?;
        stream.write_all(&bytes).await.map_err(|_| Failure::Closed)
    })
    .await
    .map_err(|_| Failure::Timeout)?
}
async fn serve(
    listener: UnixListener,
    manager: Arc<Mutex<Manager>>,
    allowed_uid: u32,
) -> Result<()> {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| Failure::Unavailable)?;
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .map_err(|_| Failure::Unavailable)?;
    let mut tasks = JoinSet::new();
    loop {
        let accepted = tokio::select! {
            _=term.recv()=>break,_=interrupt.recv()=>break,
            _=tasks.join_next(),if !tasks.is_empty()=>continue,
            result=listener.accept()=>result,
        };
        let (stream, _) = accepted.map_err(|_| Failure::Unavailable)?;
        if stream.peer_cred().map(|c| c.uid()).ok() != Some(allowed_uid) || tasks.len() >= 4 {
            continue;
        }
        let manager = manager.clone();
        tasks.spawn(async move {
            let _ = handle(stream, manager).await;
        });
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    if let Ok(mut manager) = manager.lock() {
        manager.active = None;
    }
    Ok(())
}
pub fn run(path: &str) -> std::result::Result<(), &'static str> {
    if rustix::process::geteuid().as_raw() != MEDIA_UID
        || !crate::trusted::service_uid("rustsimhub-media", MEDIA_UID)
        || !crate::trusted::service_uid("rustsimhub-edge", EDGE_UID)
        || !crate::trusted::service_uid("rustsimhub-core", CORE_UID)
    {
        return Err("fixed service identities required");
    }
    let config: Config = serde_json::from_slice(&crate::rtc_lan::read(Path::new(path), 4096)?)
        .map_err(|_| "invalid private worker configuration")?;
    if !wire::local_ip(config.bind_ip)
        || [&config.control_socket, &config.core_socket]
            .iter()
            .any(|p| {
                !p.is_absolute()
                    || p.to_str().is_none_or(|s| s.len() > 100)
                    || p.components().any(|c| {
                        matches!(
                            c,
                            std::path::Component::ParentDir | std::path::Component::CurDir
                        )
                    })
            })
    {
        return Err("explicit local worker endpoints required");
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "worker runtime unavailable")?;
    runtime.block_on(async {
        let (listener, _guard) = crate::control_socket::bind(&config.control_socket)?;
        let manager = Arc::new(Mutex::new(Manager::new(config.core_socket, config.bind_ip)));
        let result = serve(listener, manager.clone(), EDGE_UID).await;
        // Even an accept/runtime failure waits for all PCM/UDP owners to finish.
        if let Ok(mut m) = manager.lock() {
            m.active = None;
        }
        result.map_err(|_| "worker service failed")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pcm_faults_and_unexpected_transport_close_are_not_successful_stop() {
        assert_eq!(
            finish_audio(Err(Failure::Closed), false),
            Err(Failure::Closed)
        );
        assert_eq!(
            finish_audio(Err(Failure::Denied), false),
            Err(Failure::Denied)
        );
        assert!(finish_audio(Err(Failure::Closed), true).is_ok());
        assert!(finish_audio(Err(Failure::Denied), true).is_ok());
        assert_eq!(
            finish_audio(Err(Failure::Unavailable), true),
            Err(Failure::Unavailable)
        );
        assert_eq!(
            pcm_failure(rustsimhub_media::Error::XrunLimit),
            Failure::Unavailable
        );
        assert_eq!(
            pcm_failure(rustsimhub_media::Error::Device),
            Failure::Unavailable
        );
        assert_eq!(
            pcm_failure(rustsimhub_media::Error::Expired),
            Failure::Denied
        );
    }
    #[tokio::test]
    async fn incomplete_control_frame_expires_without_creating_a_session() {
        let (_client, server) = UnixStream::pair().unwrap();
        let manager = Arc::new(Mutex::new(Manager::new(
            PathBuf::from("/absent/core.sock"),
            "127.0.0.1".parse().unwrap(),
        )));
        assert!(matches!(
            handle(server, manager.clone()).await,
            Err(Failure::Timeout)
        ));
        assert!(manager.lock().unwrap().active.is_none());
    }
    fn id(n: char) -> String {
        n.to_string().repeat(32)
    }
    fn credentials(peer: [u8; 32]) -> Credentials {
        Credentials {
            ticket: [9; 32],
            peer_key: peer,
            boot: "a".repeat(64),
        }
    }
    fn verify_peer(manager: &Manager, peer: &mut Session, socket: &UdpSocket) -> [u8; 32] {
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            peer.service(socket, local::now_ms()).unwrap();
            let snapshot = manager.active(&id('a')).unwrap().snapshot(false).unwrap();
            assert!(!snapshot.pcm_opened);
            if snapshot.phase == Phase::Verified {
                return snapshot.peer_key.unwrap();
            }
            assert!(Instant::now() < end);
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn await_closed(manager: &Manager) -> Snapshot {
        let end = Instant::now() + Duration::from_secs(2);
        loop {
            let snapshot = manager.active(&id('a')).unwrap().snapshot(false).unwrap();
            if snapshot.phase == Phase::Closed {
                return snapshot;
            }
            assert!(Instant::now() < end);
            thread::sleep(Duration::from_millis(2));
        }
    }
    #[test]
    fn repeated_prepare_cannot_replace_live_session_or_restart_stopped_one() {
        let ip = "127.0.0.1".parse().unwrap();
        let socket = UdpSocket::bind((ip, 0)).unwrap();
        let (_, offer, _) = Session::offer(&socket, ip).unwrap();
        let mut manager = Manager::new(PathBuf::from("/absent/core.sock"), ip);
        let first = manager.prepare(id('a'), ip, offer.clone()).unwrap();
        assert_eq!(first.phase, Phase::Handshaking);
        assert!(!first.pcm_opened);
        let repeated = manager.prepare(id('a'), ip, offer.clone()).unwrap();
        assert_eq!(first.answer, repeated.answer);
        assert!(matches!(
            manager.prepare(id('b'), ip, offer.clone()),
            Err(Failure::Busy)
        ));
        assert!(matches!(
            manager.prepare(id('a'), ip, offer.clone() + "changed"),
            Err(Failure::Denied)
        ));
        manager.process(Command::Stop { session: id('a') }).unwrap();
        assert!(!await_closed(&manager).pcm_opened);
        assert_eq!(
            manager.prepare(id('a'), ip, offer).unwrap().phase,
            Phase::Closed
        );
    }
    #[test]
    fn premature_authorization_cannot_open_pcm_or_reuse_a_session() {
        let ip = "127.0.0.1".parse().unwrap();
        let socket = UdpSocket::bind((ip, 0)).unwrap();
        let (_, offer, _) = Session::offer(&socket, ip).unwrap();
        let mut manager = Manager::new(PathBuf::from("/absent/core.sock"), ip);
        manager.prepare(id('a'), ip, offer).unwrap();
        assert!(matches!(
            manager.process(Command::Authorize {
                session: id('a'),
                credentials: credentials([0; 32])
            }),
            Err(Failure::Denied)
        ));
        assert!(!await_closed(&manager).pcm_opened);
    }
    #[test]
    fn real_dtls_does_not_open_pcm_and_substituted_ticket_stops_before_core() {
        let ip = "127.0.0.1".parse().unwrap();
        let socket = UdpSocket::bind((ip, 0)).unwrap();
        let (mut peer, offer, pending) = Session::offer(&socket, ip).unwrap();
        let mut manager = Manager::new(PathBuf::from("/absent/core.sock"), ip);
        let answer = manager.prepare(id('a'), ip, offer).unwrap().answer.unwrap();
        peer.accept_answer(&socket, pending, &answer).unwrap();
        let actual = verify_peer(&manager, &mut peer, &socket);
        assert_ne!(actual, [0; 32]);
        assert!(matches!(
            manager.process(Command::Authorize {
                session: id('a'),
                credentials: credentials([0; 32])
            }),
            Err(Failure::Denied)
        ));
        assert!(!await_closed(&manager).pcm_opened);
    }
    #[test]
    fn verified_peer_still_requires_actual_core_identity_and_consumed_permission() {
        let ip = "127.0.0.1".parse().unwrap();
        let socket = UdpSocket::bind((ip, 0)).unwrap();
        let (mut peer, offer, pending) = Session::offer(&socket, ip).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let core = dir.path().join("core.sock");
        let _fake = std::os::unix::net::UnixListener::bind(&core).unwrap();
        let mut manager = Manager::new(core, ip);
        let answer = manager.prepare(id('a'), ip, offer).unwrap().answer.unwrap();
        peer.accept_answer(&socket, pending, &answer).unwrap();
        let actual = verify_peer(&manager, &mut peer, &socket);
        manager
            .process(Command::Authorize {
                session: id('a'),
                credentials: credentials(actual),
            })
            .unwrap();
        let closed = await_closed(&manager);
        assert!(!closed.pcm_opened);
        assert_eq!(closed.failure, Some(Failure::Denied));
    }
}
