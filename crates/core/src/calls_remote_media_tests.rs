// SPDX-License-Identifier: AGPL-3.0-only
//! Real normal Auth/Controller/dispatcher and media IPC. Only ModemManager is an
//! external private D-Bus backend; the unit worker below is a hostile transport
//! boundary fixture, not a TURN/DTLS/PCM success proof. The separate UID role
//! joins the actual native worker and independent TLS server.
use super::*;
#[path = "../../media/src/remote_signaling/mod.rs"]
pub mod actual_host;
use actual_host::{FixtureEndpoints, Host, Prepare};
use rustsimhub_media::{
    remote_worker::{self as wire, Command, Release, Snapshot},
    worker::{Counters, Phase},
};
use std::{os::unix::fs::PermissionsExt, path::PathBuf, sync::atomic::AtomicBool};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixListener,
};
struct Runtime {
    controller: Controller,
    worker: tokio::task::JoinHandle<()>,
    api: tokio::task::JoinHandle<()>,
    media: tokio::task::JoinHandle<Result<()>>,
    core: PathBuf,
    media_socket: PathBuf,
    token: String,
    second: String,
    lease: CallLease,
    store: rustsimhub_storage::Store,
    owner: Principal,
    data: FakeState,
    accepted: Arc<AtomicUsize>,
    hold: Arc<AtomicBool>,
    _directories: (tempfile::TempDir, tempfile::TempDir),
    _server: zbus::Connection,
    _guard: Box<dyn std::any::Any>,
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.controller.shutdown();
        self.api.abort();
        self.media.abort();
        self.worker.abort();
    }
}
impl Runtime {
    async fn start(expired: bool) -> Self {
        Self::start_on(expired, false).await
    }
    async fn start_on(expired: bool, normal_roles: bool) -> Self {
        let mut f = fixture(false).await;
        f.worker.media_enabled = true;
        let token = f.first.clone();
        let operation = prepare(
            &mut f.worker,
            &token,
            "normal-remote-host",
            CallAction::Dial,
            None,
            "10010",
        )
        .await;
        let receipt = submit(&mut f.worker, &token, &operation, None)
            .await
            .unwrap();
        let lease: CallLease = serde_json::from_value(receipt["lease"].clone()).unwrap();
        if expired {
            f.worker.advance_ms += 31_000;
        }
        let core = if normal_roles {
            PathBuf::from("/run/rustsimhub/core/core.sock")
        } else {
            f.d.path().join("core.sock")
        };
        let media_socket = if normal_roles {
            PathBuf::from("/run/rustsimhub/core/media.sock")
        } else {
            f.d.path().join("media.sock")
        };
        let (api_listener, api_guard) = crate::socket_runtime::SocketRuntime::bind(&core).unwrap();
        std::fs::set_permissions(&core, std::fs::Permissions::from_mode(0o660)).unwrap();
        let (media_listener, media_guard) =
            crate::media_ipc::bind(media_socket.to_str().unwrap()).unwrap();
        let auth = f.worker.auth.clone();
        let controller = f.controller.clone();
        let uid = rustix::process::geteuid().as_raw();
        let accepted = Arc::new(AtomicUsize::new(0));
        let api_uid = if normal_roles { 985 } else { uid };
        let media_uid = if normal_roles { 984 } else { uid };
        let observed = accepted.clone();
        let hold = Arc::new(AtomicBool::new(false));
        let blocked = hold.clone();
        let api = tokio::spawn(async move {
            let _api_guard = api_guard;
            while let Ok((mut stream, _)) = api_listener.accept().await {
                assert_eq!(stream.peer_cred().unwrap().uid(), api_uid);
                observed.fetch_add(1, Ordering::AcqRel);
                while blocked.load(Ordering::Acquire) {
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
                let _ = crate::authenticated::handle(
                    &mut stream,
                    auth.clone(),
                    None,
                    None,
                    Some(controller.clone()),
                    crate::authenticated::Providers::default(),
                )
                .await;
            }
        });
        let controller = f.controller.clone();
        let media = tokio::spawn(async move {
            let _guard = media_guard;
            crate::media_ipc::serve(media_listener, media_uid, controller).await
        });
        let Fixture {
            worker,
            controller,
            store,
            d,
            i,
            server,
            guard,
            data,
            first,
            second,
            owner,
        } = f;
        let worker = tokio::spawn(worker.run());
        Self {
            controller,
            worker,
            api,
            media,
            core,
            media_socket,
            token: first,
            second,
            lease,
            store,
            owner,
            data,
            accepted,
            hold,
            _directories: (d, i),
            _server: server,
            _guard: guard,
        }
    }
    fn request(&self) -> Prepare {
        Prepare {
            token: self.token.clone(),
            lease: self.lease.clone(),
            relay_handle: "approved".into(),
            expected_peer: [7; 32],
            offer: "external hostile boundary fixture".into(),
        }
    }
    fn endpoints(&self, worker: PathBuf) -> FixtureEndpoints {
        let uid = rustix::process::geteuid().as_raw();
        FixtureEndpoints {
            core: self.core.clone(),
            core_uid: uid,
            worker,
            worker_uid: uid,
        }
    }
}
struct WorkerBoundary {
    path: PathBuf,
    authorized: Arc<AtomicUsize>,
    disconnected: Arc<AtomicBool>,
    permit: Arc<Mutex<Option<rustsimhub_media::Permit>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for WorkerBoundary {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl WorkerBoundary {
    async fn start(runtime: &Runtime, bad_peer: bool) -> Self {
        let path = runtime.core.parent().unwrap().join("worker.sock");
        let listener = UnixListener::bind(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o660)).unwrap();
        let authorized = Arc::new(AtomicUsize::new(0));
        let grants = authorized.clone();
        let disconnected = Arc::new(AtomicBool::new(false));
        let ended = disconnected.clone();
        let permit = Arc::new(Mutex::new(None));
        let observed = permit.clone();
        let core = runtime.media_socket.clone();
        let uid = rustix::process::geteuid().as_raw();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut core_client = None;
            while let Ok(n) = stream.read_u32().await {
                assert!((1..=wire::MAX_FRAME).contains(&(n as usize)));
                let mut bytes = vec![0; n as usize];
                if stream.read_exact(&mut bytes).await.is_err() {
                    break;
                }
                let request = wire::parse(&bytes).unwrap();
                let session = request.command.session().to_owned();
                let mut answer = None;
                let phase = match request.command {
                    Command::Prepare { .. } => {
                        answer = Some("external answer".into());
                        Phase::Handshaking
                    }
                    Command::Inspect { .. } => {
                        if core_client.is_some() {
                            Phase::Active
                        } else {
                            Phase::Verified
                        }
                    }
                    Command::Authorize { credentials, .. } => {
                        grants.fetch_add(1, Ordering::AcqRel);
                        let client =
                            rustsimhub_media::local::Client::redeem(&core, uid, credentials)
                                .await
                                .unwrap();
                        *observed.lock().unwrap() = Some(client.permit());
                        core_client = Some(client);
                        Phase::Active
                    }
                    Command::Stop { .. } => {
                        core_client = None;
                        Phase::Closed
                    }
                };
                let peer_key = (!matches!(phase, Phase::Handshaking | Phase::Closed))
                    .then_some(if bad_peer { [8; 32] } else { [7; 32] });
                let snapshot = Snapshot {
                    session,
                    phase,
                    peer_key,
                    answer,
                    pcm_opened: phase == Phase::Active,
                    counters: Counters::default(),
                    failure: None,
                    release: if phase == Phase::Closed {
                        Release::Unknown
                    } else {
                        Release::NotRequested
                    },
                };
                let response = serde_json::to_vec(&wire::Reply {
                    protocol: 2,
                    id: request.id,
                    result: Ok(snapshot),
                })
                .unwrap();
                if stream.write_u32(response.len() as u32).await.is_err()
                    || stream.write_all(&response).await.is_err()
                {
                    break;
                }
                if phase == Phase::Closed {
                    break;
                }
            }
            drop(core_client);
            ended.store(true, Ordering::Release);
        });
        Self {
            path,
            authorized,
            disconnected,
            permit,
            task,
        }
    }
}
async fn until(mut check: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while !check() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn actual_normal_core_denies_missing_scope_bad_lease_or_expiry_before_media_connect() {
    for case in 0..7 {
        let runtime = Runtime::start(case == 6).await;
        if rustix::process::geteuid().as_raw() != 985 {
            assert!(Host::prepare(runtime.request()).await.is_err());
        }
        let path = runtime.core.parent().unwrap().join("untouched.sock");
        let listener = UnixListener::bind(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o660)).unwrap();
        let mut request = runtime.request();
        match case {
            0 => request.token = "0".repeat(64),
            1 => request.token = runtime.second.clone(),
            2 => request.lease.generation += 1,
            3 => request.lease.boot = "b".repeat(64),
            4 => request.lease.line.0 += 1,
            5 => request.lease.call_id = "x".repeat(36),
            6 => {}
            _ => unreachable!(),
        }
        assert!(
            actual_host::prepare_fixture(request, runtime.endpoints(path))
                .await
                .is_err()
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(15), listener.accept())
                .await
                .is_err()
        );
        assert_eq!(runtime.data.creates.load(Ordering::Acquire), 1);
        assert_eq!(runtime.data.starts.load(Ordering::Acquire), 1);
    }
}
#[tokio::test]
async fn actual_normal_dispatcher_issues_original_ticket_only_after_verified_peer_and_revokes_retained_host(
) {
    let runtime = Runtime::start(false).await;
    let worker = WorkerBoundary::start(&runtime, false).await;
    let (mut host, answer) =
        actual_host::prepare_fixture(runtime.request(), runtime.endpoints(worker.path.clone()))
            .await
            .unwrap();
    assert_eq!(answer, "external answer");
    assert_eq!(worker.authorized.load(Ordering::Acquire), 0);
    assert!(worker.permit.lock().unwrap().is_none());
    assert_eq!(host.poll().await.unwrap().phase, Phase::Verified);
    let status = host.activate().await.unwrap();
    assert_eq!(status.phase, Phase::Active);
    let json = serde_json::to_string(&status).unwrap();
    for secret in ["ticket", "token", "peer_key", "binding", "relay_handle"] {
        assert!(!json.contains(secret));
    }
    assert_eq!(worker.authorized.load(Ordering::Acquire), 1);
    assert!(worker
        .permit
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .check(rustsimhub_media::local::now_ms())
        .is_ok());
    runtime
        .store
        .handle()
        .revoke_client(
            runtime.owner.clone(),
            SigningKey::from_bytes(&[72; 32]).verifying_key().to_bytes(),
        )
        .unwrap();
    assert!(host.poll().await.is_err());
    until(|| worker.disconnected.load(Ordering::Acquire)).await;
    assert!(worker
        .permit
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .check(rustsimhub_media::local::now_ms())
        .is_err());
    assert!(host.activate().await.is_err());
    assert_eq!(worker.authorized.load(Ordering::Acquire), 1);
}
#[tokio::test]
async fn actual_worker_peer_mismatch_never_receives_a_core_ticket() {
    let runtime = Runtime::start(false).await;
    let worker = WorkerBoundary::start(&runtime, true).await;
    let (mut host, _) =
        actual_host::prepare_fixture(runtime.request(), runtime.endpoints(worker.path.clone()))
            .await
            .unwrap();
    assert!(host.activate().await.is_err());
    until(|| worker.disconnected.load(Ordering::Acquire)).await;
    assert_eq!(worker.authorized.load(Ordering::Acquire), 0);
    assert!(worker.permit.lock().unwrap().is_none());
}
#[tokio::test]
async fn cancelling_host_during_normal_core_observation_closes_retained_worker_before_its_own_exchange(
) {
    let runtime = Runtime::start(false).await;
    let worker = WorkerBoundary::start(&runtime, false).await;
    let (mut host, _) =
        actual_host::prepare_fixture(runtime.request(), runtime.endpoints(worker.path.clone()))
            .await
            .unwrap();
    host.activate().await.unwrap();
    let accepted = runtime.accepted.load(Ordering::Acquire);
    runtime.hold.store(true, Ordering::Release);
    {
        let poll = host.poll();
        tokio::pin!(poll);
        tokio::select! {result=&mut poll=>panic!("held normal Core unexpectedly returned {}",result.is_ok()),_=until(||runtime.accepted.load(Ordering::Acquire)>accepted)=>{}}
    }
    until(|| worker.disconnected.load(Ordering::Acquire)).await;
    assert!(worker
        .permit
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .check(rustsimhub_media::local::now_ms())
        .is_err());
    runtime.hold.store(false, Ordering::Release);
    assert!(host.poll().await.is_err());
}
#[tokio::test]
async fn local_media_stop_does_not_create_another_call_or_renew_the_original_lease() {
    let runtime = Runtime::start(false).await;
    let worker = WorkerBoundary::start(&runtime, false).await;
    let (mut host, _) =
        actual_host::prepare_fixture(runtime.request(), runtime.endpoints(worker.path.clone()))
            .await
            .unwrap();
    host.activate().await.unwrap();
    let status = host.close().await.unwrap();
    assert_eq!(status.phase, Phase::Closed);
    assert!(status.release == Release::Unknown);
    assert_eq!(runtime.data.creates.load(Ordering::Acquire), 1);
    assert_eq!(runtime.data.starts.load(Ordering::Acquire), 1);
    assert_eq!(runtime.data.hangups.load(Ordering::Acquire), 0);
    let checked = runtime
        .controller
        .request(AuthCommand::CheckMediaLease {
            token: runtime.token.clone(),
            lease: runtime.lease.clone(),
        })
        .await
        .unwrap();
    assert_eq!(checked["media_lease_valid"], true);
}

fn publish(path: &str, value: serde_json::Value) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::path::Path::new(path);
    let next = file.with_extension("next");
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o640)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(&next)
        .unwrap();
    out.write_all(&serde_json::to_vec(&value).unwrap()).unwrap();
    out.sync_all().unwrap();
    std::fs::rename(next, file).unwrap();
}
async fn shared_file(path: &str) -> serde_json::Value {
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
#[tokio::test]
#[ignore = "explicit disposable normal Core/worker/Edge UID fixture, no device or host accounts"]
async fn normal_core_actual_uid_role() {
    assert_eq!(rustix::process::getuid().as_raw(), 986);
    assert_eq!(rustix::process::geteuid().as_raw(), 986);
    assert_eq!(
        std::env::var("RUSTSIMHUB_NORMAL_CORE_FIXTURE").as_deref(),
        Ok("1")
    );
    assert!(std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .any(|s| s == "CapEff:\t0000000000000000"));
    let mut runtime = Runtime::start_on(false, true).await;
    assert_eq!(runtime.data.creates.load(Ordering::Acquire), 1);
    assert_eq!(runtime.data.starts.load(Ordering::Acquire), 1);
    publish(
        "/fixture/core/ready.json",
        json!({"socket":runtime.media_socket,"normal_socket":runtime.core,"token":runtime.token,
        "denied_token":runtime.second,"lease":runtime.lease,"normal_auth_and_calls_controller":true}),
    );
    // Only the actual original store's owner authority performs revocation.
    // The fake backend is the private ModemManager D-Bus peer, never Core Auth,
    // media Authority, an invented Context, or a copied call/lease controller.
    shared_file("/fixture/edge/revoke.json").await;
    runtime
        .store
        .handle()
        .revoke_client(
            runtime.owner.clone(),
            SigningKey::from_bytes(&[72; 32]).verifying_key().to_bytes(),
        )
        .unwrap();
    assert!(runtime
        .controller
        .request(AuthCommand::CheckMediaLease {
            token: runtime.token.clone(),
            lease: runtime.lease.clone()
        })
        .await
        .is_err());
    shared_file("/fixture/media/done.json").await;
    publish(
        "/fixture/core/done.json",
        json!({"uid":986,"actual_media_peer_uid":984,"normal_auth_and_calls_controller":true,
        "normal_dispatcher":true,"original_owner_revocation":true,"normal_media_lease_rejected":true,
        "fake_modem_creates":runtime.data.creates.load(Ordering::Acquire),"fake_modem_starts":runtime.data.starts.load(Ordering::Acquire),
        "call_lease_renewed_by_host":false}),
    );
    runtime.controller.shutdown();
    tokio::time::timeout(Duration::from_secs(2), &mut runtime.worker)
        .await
        .unwrap()
        .unwrap();
}
