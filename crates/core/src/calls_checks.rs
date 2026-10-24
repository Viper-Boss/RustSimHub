// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use std::sync::atomic::{AtomicI32, AtomicU8, AtomicUsize, Ordering};
#[derive(Clone)]
struct FakeState {
    paths: Arc<Mutex<Vec<OwnedObjectPath>>>,
    state: Arc<AtomicI32>,
    direction: Arc<AtomicI32>,
    number: Arc<Mutex<String>>,
    mode: Arc<AtomicU8>,
    creates: Arc<AtomicUsize>,
    starts: Arc<AtomicUsize>,
    accepts: Arc<AtomicUsize>,
    hangups: Arc<AtomicUsize>,
    tones: Arc<AtomicUsize>,
}
fn path() -> OwnedObjectPath {
    "/org/freedesktop/ModemManager1/Call/9000"
        .try_into()
        .unwrap()
}
struct FakeVoice(FakeState);

#[tokio::test]
async fn setup_ap_quiescence_requires_fresh_real_voice_catalogue_and_owner() {
    let mut f = fixture(false).await;
    let token = {
        let mut a = f.worker.auth.lock().unwrap();
        let k = SigningKey::from_bytes(&[71; 32]);
        let c = a.login_challenge(k.verifying_key().to_bytes()).unwrap();
        a.login_finish(&c.id, &k.sign(&c.transcript).to_bytes())
            .unwrap()
            .expose()
            .to_owned()
    };
    let request = || AuthCommand::CallStatus {
        token: token.clone(),
    };
    assert!(f.worker.setup_ap_idle(request()).await.is_ok());
    assert!(f
        .worker
        .setup_ap_idle(AuthCommand::CallStatus {
            token: f.first.clone()
        })
        .await
        .is_err());
    let binding = f.worker.bindings.clone();
    f.worker.bindings = watch::channel(None).1;
    assert!(f.worker.setup_ap_idle(request()).await.is_err());
    f.worker.bindings = binding;
    f.data.paths.lock().unwrap().push(path());
    f.data.state.store(3, Ordering::SeqCst);
    assert!(f.worker.setup_ap_idle(request()).await.is_err());
    assert_eq!(f.data.creates.load(Ordering::SeqCst), 0);
    assert_eq!(f.data.starts.load(Ordering::SeqCst), 0);
    assert_eq!(f.data.hangups.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn session_continuation_preserves_live_call_lease_and_media_binding() {
    let mut f = fixture(false).await;
    let token = f.first.clone();
    let id = prepare(
        &mut f.worker,
        &token,
        "continuation-dial",
        CallAction::Dial,
        None,
        "10010",
    )
    .await;
    let result = submit(&mut f.worker, &token, &id, None).await.unwrap();
    let lease: CallLease = serde_json::from_value(result["lease"].clone()).unwrap();
    f.worker.media_enabled = true;
    let check = || AuthCommand::CheckMediaLease {
        token: token.clone(),
        lease: lease.clone(),
    };
    let before = f.worker.process(check()).await.unwrap();
    let continued = {
        let mut auth = f.worker.auth.lock().unwrap();
        let c = auth.session_challenge(&token).unwrap();
        let key = SigningKey::from_bytes(&[72; 32]);
        auth.renew_session(&token, &c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap()
            .expose()
            .to_owned()
    };
    assert_eq!(before, f.worker.process(check()).await.unwrap());
    assert_eq!(
        before,
        f.worker
            .process(AuthCommand::CheckMediaLease {
                token: continued.clone(),
                lease: lease.clone()
            })
            .await
            .unwrap()
    );
    let renewed = f
        .worker
        .process(AuthCommand::RenewCall {
            token: continued,
            lease: lease.clone(),
        })
        .await
        .unwrap();
    assert_eq!(renewed["lease"]["generation"], lease.generation);
    assert_eq!(renewed["lease"]["call_id"], lease.call_id);
    assert_eq!(f.data.creates.load(Ordering::SeqCst), 1);
    assert_eq!(f.data.starts.load(Ordering::SeqCst), 1);
    assert_eq!(f.data.hangups.load(Ordering::SeqCst), 0);
}
#[zbus::interface(name = "org.freedesktop.ModemManager1.Modem.Voice")]
impl FakeVoice {
    #[zbus(property)]
    fn emergency_only(&self) -> bool {
        false
    }
    fn list_calls(&self) -> Vec<OwnedObjectPath> {
        self.0.paths.lock().unwrap().clone()
    }
    #[zbus(property)]
    fn calls(&self) -> Vec<OwnedObjectPath> {
        self.list_calls()
    }
    fn create_call(
        &self,
        properties: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        self.0.creates.fetch_add(1, Ordering::SeqCst);
        assert_eq!(properties.len(), 1);
        *self.0.number.lock().unwrap() =
            <&str>::try_from(&properties["number"]).unwrap().to_owned();
        self.0.state.store(0, Ordering::SeqCst);
        self.0.direction.store(2, Ordering::SeqCst);
        *self.0.paths.lock().unwrap() = vec![path()];
        Ok(path())
    }
}
struct FakeCall(FakeState);
#[zbus::interface(name = "org.freedesktop.ModemManager1.Call")]
impl FakeCall {
    async fn start(&self) -> zbus::fdo::Result<()> {
        self.0.starts.fetch_add(1, Ordering::SeqCst);
        if self.0.mode.load(Ordering::SeqCst) == 1 {
            tokio::time::sleep(Duration::from_secs(3)).await;
            return Err(zbus::fdo::Error::Failed("synthetic".into()));
        }
        self.0.state.store(4, Ordering::SeqCst);
        Ok(())
    }
    fn accept(&self) {
        self.0.accepts.fetch_add(1, Ordering::SeqCst);
        self.0.state.store(4, Ordering::SeqCst);
    }
    fn hangup(&self) -> zbus::fdo::Result<()> {
        self.0.hangups.fetch_add(1, Ordering::SeqCst);
        if self.0.mode.load(Ordering::SeqCst) == 4 {
            return Err(zbus::fdo::Error::Failed("synthetic".into()));
        }
        self.0.state.store(7, Ordering::SeqCst);
        Ok(())
    }
    async fn send_dtmf(&self, _dtmf: &str) -> zbus::fdo::Result<()> {
        self.0.tones.fetch_add(1, Ordering::SeqCst);
        if self.0.mode.load(Ordering::SeqCst) == 2 {
            tokio::time::sleep(Duration::from_secs(3)).await;
            return Err(zbus::fdo::Error::Failed("synthetic".into()));
        }
        Ok(())
    }
    #[zbus(property)]
    fn state(&self) -> i32 {
        self.0.state.load(Ordering::SeqCst)
    }
    #[zbus(property)]
    fn state_reason(&self) -> i32 {
        0
    }
    #[zbus(property)]
    fn direction(&self) -> i32 {
        self.0.direction.load(Ordering::SeqCst)
    }
    #[zbus(property)]
    fn number(&self) -> String {
        self.0.number.lock().unwrap().clone()
    }
}
fn directory() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let d = tempfile::tempdir().unwrap();
    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    d
}
#[tokio::test]
async fn preflight_never_issues_a_ticket_and_is_fenced_to_current_owner_and_line() {
    let mut f = fixture(false).await;
    let check = |token: &str| AuthCommand::CheckMediaPreflight {
        token: token.into(),
    };
    assert!(f.worker.process(check(&f.first)).await.is_err());
    f.worker.media_enabled = true;
    let before = f.worker.process(check(&f.first)).await.unwrap();
    assert_eq!(before["media_preflight_valid"], true);
    assert_eq!(before["binding"].as_str().unwrap().len(), 64);
    assert!(before.get("ticket").is_none() && f.worker.media.owner.is_none());
    assert_eq!(f.data.creates.load(Ordering::SeqCst), 0);
    assert_eq!(f.data.starts.load(Ordering::SeqCst), 0);
    assert!(f.worker.process(check(&f.second)).await.is_err()); // no media.use
    let id = prepare(
        &mut f.worker,
        &f.first,
        "preflight-dial",
        CallAction::Dial,
        None,
        "10010",
    )
    .await;
    submit(&mut f.worker, &f.first, &id, None).await.unwrap();
    assert_eq!(f.worker.process(check(&f.first)).await.unwrap(), before);
    assert!(f.worker.process(check(&f.second)).await.is_err());
    f.worker.bindings = watch::channel(None).1;
    assert!(f.worker.process(check(&f.first)).await.is_err());
}
#[tokio::test]
async fn media_issue_checks_real_owner_scopes_lease_active_and_explicit_mode() {
    let mut f = fixture(false).await;
    let id = prepare(
        &mut f.worker,
        &f.first,
        "media-dial",
        CallAction::Dial,
        None,
        "+12025550123",
    )
    .await;
    let value = submit(&mut f.worker, &f.first, &id, None).await.unwrap();
    let lease: CallLease = serde_json::from_value(value["lease"].clone()).unwrap();
    let request = |token: &str, lease: &CallLease| AuthCommand::IssueMedia {
        token: token.into(),
        lease: lease.clone(),
        peer_key: [7; 32],
    };
    let check = |token: &str, lease: &CallLease| AuthCommand::CheckMediaLease {
        token: token.into(),
        lease: lease.clone(),
    };
    assert!(f.worker.process(check(&f.first, &lease)).await.is_err());
    assert!(f.worker.process(request(&f.first, &lease)).await.is_err());
    f.worker.media_enabled = true;
    assert!(f.worker.process(check(&f.second, &lease)).await.is_err());
    assert!(f.worker.process(request(&f.second, &lease)).await.is_err());
    let mut bad = lease.clone();
    bad.generation += 1;
    assert!(f.worker.process(check(&f.first, &bad)).await.is_err());
    assert!(f.worker.process(request(&f.first, &bad)).await.is_err());
    f.data.state.store(5, Ordering::SeqCst);
    assert!(f.worker.process(check(&f.first, &lease)).await.is_err());
    assert!(f.worker.process(request(&f.first, &lease)).await.is_err());
    f.data.state.store(4, Ordering::SeqCst);
    let before = f.worker.process(check(&f.first, &lease)).await.unwrap();
    assert_eq!(before["media_lease_valid"], true);
    assert_eq!(before["binding"].as_str().unwrap().len(), 64);
    assert_eq!(
        before,
        f.worker.process(check(&f.first, &lease)).await.unwrap()
    );
    let credential = f.worker.process(request(&f.first, &lease)).await.unwrap();
    assert_eq!(
        before,
        f.worker.process(check(&f.first, &lease)).await.unwrap()
    );
    assert!(credential["ticket"].is_array());
    assert!(f.worker.process(request(&f.first, &lease)).await.is_err());
    assert_eq!(f.data.creates.load(Ordering::SeqCst), 1);
    assert_eq!(f.data.starts.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn media_local_socket_redeems_once_and_revoke_or_disconnect_closes_permit() {
    let mut f = fixture(false).await;
    f.worker.media_enabled = true;
    let id = prepare(
        &mut f.worker,
        &f.first,
        "media-dial",
        CallAction::Dial,
        None,
        "+12025550123",
    )
    .await;
    let value = submit(&mut f.worker, &f.first, &id, None).await.unwrap();
    let lease: CallLease = serde_json::from_value(value["lease"].clone()).unwrap();
    let value = f
        .worker
        .process(AuthCommand::IssueMedia {
            token: f.first.clone(),
            lease,
            peer_key: [7; 32],
        })
        .await
        .unwrap();
    let credentials: rustsimhub_media::protocol::Credentials =
        serde_json::from_value(value.clone()).unwrap();
    let socket = f.d.path().join("media.sock");
    let (listener, runtime) = crate::media_ipc::bind(socket.to_str().unwrap()).unwrap();
    let uid = rustix::process::geteuid().as_raw();
    let server = tokio::spawn(crate::media_ipc::serve(listener, uid, f.controller.clone()));
    let worker = tokio::spawn(f.worker.run());
    let mut client = rustsimhub_media::local::Client::redeem(&socket, uid, credentials)
        .await
        .unwrap();
    let permit = client.permit();
    client.pulse().await.unwrap();
    assert!(permit.check(rustsimhub_media::local::now_ms()).is_ok());
    let replay = serde_json::from_value(value).unwrap();
    assert!(
        rustsimhub_media::local::Client::redeem(&socket, uid, replay)
            .await
            .is_err()
    );
    // A rejected second connection cannot close the first connection's lease.
    client.pulse().await.unwrap();
    let p = f
        .store
        .handle()
        .principal(SigningKey::from_bytes(&[72; 32]).verifying_key().to_bytes())
        .unwrap();
    // Revoke the actual media owner, independent of session token expiry.
    let first = match p {
        Some(p) => p,
        None => panic!("fixture key changed"),
    };
    f.store
        .handle()
        .revoke_client(f.owner.clone(), first.public_key)
        .unwrap();
    assert!(client.pulse().await.is_err());
    assert!(permit.check(rustsimhub_media::local::now_ms()).is_err());
    drop(client);
    f.controller.shutdown();
    worker.await.unwrap();
    server.abort();
    let _ = server.await;
    drop(runtime);
    assert!(!socket.exists());
}
#[tokio::test]
async fn media_local_socket_rejects_wrong_uid_and_missing_redeem() {
    let mut f = fixture(false).await;
    f.worker.media_enabled = true;
    let socket = f.d.path().join("media.sock");
    let uid = rustix::process::geteuid().as_raw();
    let (listener, runtime) = crate::media_ipc::bind(socket.to_str().unwrap()).unwrap();
    let server = tokio::spawn(crate::media_ipc::serve(
        listener,
        uid + 1,
        f.controller.clone(),
    ));
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
    let _ = stream.write_all(&[0, 0, 0, 1, b'!']).await;
    let mut byte = [0];
    assert!(matches!(stream.read(&mut byte).await, Ok(0) | Err(_)));
    server.abort();
    let _ = server.await;
    drop(runtime);
    let (listener, runtime) = crate::media_ipc::bind(socket.to_str().unwrap()).unwrap();
    let server = tokio::spawn(crate::media_ipc::serve(listener, uid, f.controller.clone()));
    for payload in [
        None,
        Some(br#"{"protocol":1,"id":1,"command":{"op":"pulse"}}"#.as_slice()),
    ] {
        let mut s = tokio::net::UnixStream::connect(&socket).await.unwrap();
        match payload {
            None => s.write_u32(u32::MAX).await.unwrap(),
            Some(p) => {
                s.write_u32(p.len() as u32).await.unwrap();
                s.write_all(p).await.unwrap();
            }
        }
        assert!(matches!(s.read(&mut byte).await, Ok(0) | Err(_)));
    }
    server.abort();
    let _ = server.await;
    drop(runtime);
}
struct Fixture {
    worker: Worker,
    controller: Controller,
    store: rustsimhub_storage::Store,
    d: tempfile::TempDir,
    i: tempfile::TempDir,
    server: zbus::Connection,
    guard: Box<dyn std::any::Any>,
    data: FakeState,
    first: String,
    second: String,
    owner: Principal,
}
#[tokio::test]
async fn live_status_excludes_persisted_history_and_unavailable_catalogue() {
    let mut f = fixture(true).await;
    let status = |token: &str| AuthCommand::CallStatus {
        token: token.into(),
    };
    assert!(f.worker.process(status(&"0".repeat(64))).await.is_err());
    let current = f.worker.process(status(&f.first)).await.unwrap();
    assert_eq!(current["available"], true);
    let calls = current["live_calls"].as_array().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["state"], "ringing_in");
    assert_eq!(calls[0]["availability"], "observed");
    assert!(calls[0].get("number").is_none());
    assert!(calls[0].get("source").is_none());
    let bindings = f.worker.bindings.clone();
    f.worker.bindings = watch::channel(None).1;
    let unavailable = f.worker.process(status(&f.first)).await.unwrap();
    assert_eq!(unavailable["available"], false);
    assert_eq!(unavailable["live_calls"], json!([]));
    assert_eq!(f.worker.live.len(), 1); // Stale memory is not a live authority.
    f.worker.bindings = bindings;
    f.data.paths.lock().unwrap().clear();
    let empty = f.worker.process(status(&f.first)).await.unwrap();
    assert_eq!(empty["live_calls"], json!([]));
    let historical = f.worker.store.calls(None, 16).unwrap();
    assert_eq!(historical.calls.len(), 1);
    assert_eq!(historical.calls[0].availability, "reconciling");
    f.worker.bindings = watch::channel(None).1;
    let unavailable = f.worker.process(status(&f.first)).await.unwrap();
    assert_eq!(unavailable["available"], false);
    assert_eq!(unavailable["live_calls"], json!([]));
    assert_eq!(f.data.accepts.load(Ordering::SeqCst), 0);
    assert_eq!(f.data.starts.load(Ordering::SeqCst), 0);
    assert_eq!(f.data.hangups.load(Ordering::SeqCst), 0);
    f.server.close().await.unwrap();
}
#[tokio::test]
async fn media_session_cannot_continue_after_hold_reuse_binding_loss_or_disconnect() {
    for cause in 0..4 {
        let mut f = fixture(false).await;
        f.worker.media_enabled = true;
        let id = prepare(
            &mut f.worker,
            &f.first,
            "media-dial",
            CallAction::Dial,
            None,
            "+12025550123",
        )
        .await;
        let value = submit(&mut f.worker, &f.first, &id, None).await.unwrap();
        let lease = serde_json::from_value(value["lease"].clone()).unwrap();
        let credentials = serde_json::from_value(
            f.worker
                .process(AuthCommand::IssueMedia {
                    token: f.first.clone(),
                    lease,
                    peer_key: [7; 32],
                })
                .await
                .unwrap(),
        )
        .unwrap();
        let socket = f.d.path().join("media.sock");
        let uid = rustix::process::geteuid().as_raw();
        let (listener, runtime) = crate::media_ipc::bind(socket.to_str().unwrap()).unwrap();
        let server = tokio::spawn(crate::media_ipc::serve(listener, uid, f.controller.clone()));
        let worker = tokio::spawn(f.worker.run());
        let mut client = rustsimhub_media::local::Client::redeem(&socket, uid, credentials)
            .await
            .unwrap();
        let permit = client.permit();
        match cause {
            0 => f.data.state.store(5, Ordering::SeqCst),
            1 => *f.data.number.lock().unwrap() = "+12025550199".into(),
            2 => f.guard = Box::new(()),
            _ => server.abort(),
        }
        assert!(client.pulse().await.is_err());
        assert!(permit.check(rustsimhub_media::local::now_ms()).is_err());
        drop(client);
        f.controller.shutdown();
        worker.await.unwrap();
        // Dropping the final listener future aborts its bounded connection tasks.
        server.abort();
        let _ = server.await;
        if cause == 1 {
            assert_eq!(f.data.hangups.load(Ordering::SeqCst), 0);
        }
        drop(runtime);
    }
}
async fn fixture(incoming: bool) -> Fixture {
    let (server, client, _) = crate::observer::checks::peers().await;
    let data = FakeState {
        paths: Arc::new(Mutex::new(if incoming { vec![path()] } else { vec![] })),
        state: Arc::new(AtomicI32::new(if incoming { 3 } else { 0 })),
        direction: Arc::new(AtomicI32::new(if incoming { 1 } else { 2 })),
        number: Arc::new(Mutex::new("+12025550123".into())),
        mode: Arc::new(AtomicU8::new(0)),
        creates: Arc::new(AtomicUsize::new(0)),
        starts: Arc::new(AtomicUsize::new(0)),
        accepts: Arc::new(AtomicUsize::new(0)),
        hangups: Arc::new(AtomicUsize::new(0)),
        tones: Arc::new(AtomicUsize::new(0)),
    };
    server
        .object_server()
        .at(
            "/org/freedesktop/ModemManager1/Modem/0",
            FakeVoice(data.clone()),
        )
        .await
        .unwrap();
    server
        .object_server()
        .at(path(), FakeCall(data.clone()))
        .await
        .unwrap();
    let d = directory();
    let i = directory();
    let store = rustsimhub_storage::Store::open(d.path()).unwrap();
    let h = store.handle();
    let mut a = Auth::new(
        rustsimhub_auth::Identity::open(i.path()).unwrap(),
        h.clone(),
    )
    .unwrap();
    let owner = SigningKey::from_bytes(&[71; 32]);
    let first = SigningKey::from_bytes(&[72; 32]);
    let second = SigningKey::from_bytes(&[73; 32]);
    let c = a
        .bootstrap_challenge(owner.verifying_key().to_bytes())
        .unwrap();
    a.bootstrap_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    let owner = h
        .principal(owner.verifying_key().to_bytes())
        .unwrap()
        .unwrap();
    let mut tokens = vec![];
    for key in [&first, &second] {
        h.approve_client(
            owner.clone(),
            key.verifying_key().to_bytes(),
            rustsimhub_policy::Kind::Client,
            rustsimhub_policy::mask(if key == &first {
                &[Scope::CallsRead, Scope::CallsControl, Scope::MediaUse]
            } else {
                &[Scope::CallsRead, Scope::CallsControl]
            })
            .unwrap(),
        )
        .unwrap();
        let c = a.login_challenge(key.verifying_key().to_bytes()).unwrap();
        tokens.push(
            a.login_finish(&c.id, &key.sign(&c.transcript).to_bytes())
                .unwrap()
                .expose()
                .to_owned(),
        );
    }
    let line = h
        .observe_line(rustsimhub_storage::LineObservation {
            backend: "synthetic-voice".into(),
            fingerprint: Some([9; 32]),
            status: rustsimhub_storage::LineStatus::Ready,
        })
        .unwrap()
        .line;
    let context = rustsimhub_storage::SendContext {
        line,
        backend: "synthetic-voice".into(),
        owner: ":1.20".into(),
        modem: "/org/freedesktop/ModemManager1/Modem/0".into(),
        fingerprint: [9; 32],
    };
    let (binding, watcher) = crate::observer::test_binding(client, context).await;
    let (publisher, bindings) = watch::channel(Some(binding));
    let (controller, worker) = channel(Arc::new(Mutex::new(a)), h, bindings, "a".repeat(64), true);
    Fixture {
        worker,
        controller,
        store,
        d,
        i,
        server,
        guard: Box::new((watcher, publisher)),
        data,
        first: tokens.remove(0),
        second: tokens.remove(0),
        owner,
    }
}
async fn prepare(
    w: &mut Worker,
    token: &str,
    key: &str,
    action: CallAction,
    call_id: Option<String>,
    payload: &str,
) -> String {
    let line = w.binding().unwrap().context.line.0;
    let value = w
        .process(AuthCommand::PrepareCall {
            token: token.into(),
            line_epoch: line,
            idempotency_key: key.into(),
            action,
            call_id,
            payload: payload.into(),
        })
        .await
        .unwrap();
    value["operation"]["id"].as_str().unwrap().to_owned()
}
async fn submit(w: &mut Worker, token: &str, id: &str, lease: Option<CallLease>) -> Result<Value> {
    w.process(AuthCommand::SubmitCall {
        token: token.into(),
        operation_id: id.into(),
        lease,
        urgent: false,
    })
    .await
}
#[tokio::test]
async fn prepared_cancellation_checks_owner_mode_and_never_calls_modem() {
    let mut f = fixture(false).await;
    let token = f.first.clone();
    let id = prepare(
        &mut f.worker,
        &token,
        "cancel",
        CallAction::Dial,
        None,
        "10010",
    )
    .await;
    assert!(f
        .worker
        .process(AuthCommand::CancelPreparedCall {
            token: f.second.clone(),
            operation_id: id.clone()
        })
        .await
        .is_err());
    let value = f
        .worker
        .process(AuthCommand::CancelPreparedCall {
            token: token.clone(),
            operation_id: id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(value["operation"]["state"], "cancelled");
    assert_eq!(
        submit(&mut f.worker, &token, &id, None).await.unwrap()["operation"]["state"],
        "cancelled"
    );
    let next = prepare(
        &mut f.worker,
        &token,
        "readonly-cancel",
        CallAction::Dial,
        None,
        "10010",
    )
    .await;
    f.worker.writes = false;
    assert!(f
        .worker
        .process(AuthCommand::CancelPreparedCall {
            token,
            operation_id: next
        })
        .await
        .is_err());
    assert_eq!(f.data.creates.load(Ordering::SeqCst), 0);
    assert_eq!(f.data.starts.load(Ordering::SeqCst), 0);
    assert_eq!(f.data.hangups.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn ringing_rejection_is_single_use_and_never_acquires_persistent_ownership() {
    for failed in [false, true] {
        let mut f = fixture(true).await;
        f.worker.refresh().await.unwrap();
        let target = f.worker.live.values().next().unwrap().summary.id.clone();
        let token = f.first.clone();
        let id = prepare(
            &mut f.worker,
            &token,
            "reject",
            CallAction::Hangup,
            Some(target),
            "",
        )
        .await;
        if failed {
            f.data.mode.store(4, Ordering::SeqCst);
        }
        let result = submit(&mut f.worker, &token, &id, None).await.unwrap();
        assert_eq!(
            result["operation"]["state"],
            if failed { "unknown" } else { "submitted" }
        );
        assert!(result["lease"].is_null() && f.worker.owned.is_none());
        submit(&mut f.worker, &token, &id, None).await.unwrap();
        f.worker.advance_ms += 31_000;
        f.worker.expire().await;
        assert_eq!(f.data.hangups.load(Ordering::SeqCst), 1);
        assert_eq!(f.data.accepts.load(Ordering::SeqCst), 0);
        assert_eq!(f.data.creates.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn ringing_rejection_cannot_take_an_answered_outgoing_or_other_owned_call() {
    for (state, direction) in [(4, 1), (3, 2), (6, 1), (7, 1)] {
        let mut f = fixture(true).await;
        f.worker.refresh().await.unwrap();
        let target = f.worker.live.values().next().unwrap().summary.id.clone();
        let token = f.first.clone();
        let id = prepare(
            &mut f.worker,
            &token,
            "changed-rejection",
            CallAction::Hangup,
            Some(target),
            "",
        )
        .await;
        f.data.state.store(state, Ordering::SeqCst);
        f.data.direction.store(direction, Ordering::SeqCst);
        assert!(submit(&mut f.worker, &token, &id, None).await.is_err());
        assert_eq!(f.data.hangups.load(Ordering::SeqCst), 0);
        assert!(f.worker.owned.is_none());
    }
    let mut f = fixture(true).await;
    f.worker.refresh().await.unwrap();
    let target = f.worker.live.values().next().unwrap().summary.id.clone();
    let first = f.first.clone();
    let second = f.second.clone();
    let rejection = prepare(
        &mut f.worker,
        &second,
        "rejection",
        CallAction::Hangup,
        Some(target.clone()),
        "",
    )
    .await;
    let answer = prepare(
        &mut f.worker,
        &first,
        "answer",
        CallAction::Accept,
        Some(target),
        "",
    )
    .await;
    submit(&mut f.worker, &first, &answer, None).await.unwrap();
    assert!(submit(&mut f.worker, &second, &rejection, None)
        .await
        .is_err());
    assert_eq!(f.data.hangups.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn replay_of_cancelled_hangup_cannot_revoke_current_media() {
    let mut f = fixture(false).await;
    let token = f.first.clone();
    let id = prepare(
        &mut f.worker,
        &token,
        "dial-for-cancelled-hangup",
        CallAction::Dial,
        None,
        "10010",
    )
    .await;
    let result = submit(&mut f.worker, &token, &id, None).await.unwrap();
    let lease: CallLease = serde_json::from_value(result["lease"].clone()).unwrap();
    let hangup = prepare(
        &mut f.worker,
        &token,
        "hangup-for-cancel",
        CallAction::Hangup,
        Some(lease.call_id.clone()),
        "",
    )
    .await;
    f.worker
        .process(AuthCommand::CancelPreparedCall {
            token: token.clone(),
            operation_id: hangup.clone(),
        })
        .await
        .unwrap();
    f.worker.media.owner = Some(
        f.worker
            .principal(&token, Scope::CallsControl)
            .await
            .unwrap(),
    );
    let result = submit(&mut f.worker, &token, &hangup, Some(lease))
        .await
        .unwrap();
    assert_eq!(result["operation"]["state"], "cancelled");
    assert!(f.worker.media.owner.is_some());
    assert_eq!(f.data.hangups.load(Ordering::SeqCst), 0);
    assert_eq!(f.data.state.load(Ordering::SeqCst), 4);
}
#[tokio::test]
async fn network_exclusion_denies_new_intents_and_submits_but_allows_cleanup() {
    let mut f = fixture(false).await;
    f.worker.coordination = Arc::new(crate::coordination::Coordination::default());
    let id = prepare(
        &mut f.worker,
        &f.first,
        "prepared-before-network",
        CallAction::Dial,
        None,
        "+12025550123",
    )
    .await;
    let guard = f.worker.coordination.network().unwrap();
    let line = f.worker.binding().unwrap().context.line.0;
    assert!(f
        .worker
        .process(AuthCommand::PrepareCall {
            token: f.first.clone(),
            line_epoch: line,
            idempotency_key: "denied-network".into(),
            action: CallAction::Dial,
            call_id: None,
            payload: "+12025550123".into()
        })
        .await
        .is_err());
    assert!(submit(&mut f.worker, &f.first, &id, None).await.is_err());
    assert_eq!(f.data.creates.load(Ordering::SeqCst), 0);
    let p = f
        .worker
        .principal(&f.first, Scope::CallsControl)
        .await
        .unwrap();
    assert_eq!(
        f.worker.result(id.clone(), p).await.unwrap()["operation"]["state"],
        "prepared"
    );
    drop(guard);
    let value = submit(&mut f.worker, &f.first, &id, None).await.unwrap();
    let lease: CallLease = serde_json::from_value(value["lease"].clone()).unwrap();
    let guard = f.worker.coordination.network().unwrap();
    let hangup = prepare(
        &mut f.worker,
        &f.first,
        "cleanup-during-network",
        CallAction::Hangup,
        Some(lease.call_id.clone()),
        "",
    )
    .await;
    assert_eq!(
        submit(&mut f.worker, &f.first, &hangup, Some(lease))
            .await
            .unwrap()["operation"]["state"],
        "submitted"
    );
    assert_eq!(f.data.hangups.load(Ordering::SeqCst), 1);
    drop(guard);
}
#[tokio::test]
async fn private_voice_dial_timeout_never_restarts_and_tone_timeout_never_repeats() {
    let Fixture {
        worker: mut w,
        controller: _c,
        store: _store,
        d: _d,
        i: _i,
        server,
        guard: _guard,
        data: f,
        first: token,
        second: _other,
        owner: _owner,
    } = fixture(false).await;
    f.mode.store(1, Ordering::SeqCst);
    let id = prepare(
        &mut w,
        &token,
        "dial",
        CallAction::Dial,
        None,
        "+12025550123",
    )
    .await;
    let value = submit(&mut w, &token, &id, None).await.unwrap();
    assert_eq!(value["operation"]["state"], "unknown");
    submit(&mut w, &token, &id, None).await.unwrap();
    assert_eq!(f.creates.load(Ordering::SeqCst), 1);
    assert_eq!(f.starts.load(Ordering::SeqCst), 1);
    f.state.store(4, Ordering::SeqCst);
    let value = w
        .process(AuthCommand::CallOperation {
            token: token.clone(),
            operation_id: id,
        })
        .await
        .unwrap();
    assert_eq!(value["operation"]["state"], "submitted");
    let lease: CallLease = serde_json::from_value(value["lease"].clone()).unwrap();
    f.mode.store(2, Ordering::SeqCst);
    let id = prepare(
        &mut w,
        &token,
        "tone",
        CallAction::Dtmf,
        Some(lease.call_id.clone()),
        "5",
    )
    .await;
    let value = submit(&mut w, &token, &id, Some(lease.clone()))
        .await
        .unwrap();
    assert_eq!(value["operation"]["state"], "unknown");
    submit(&mut w, &token, &id, Some(lease)).await.unwrap();
    assert_eq!(f.tones.load(Ordering::SeqCst), 1);
    server.close().await.unwrap();
}
#[tokio::test]
async fn private_voice_only_one_client_answers_and_revocation_cleans_up_only_owned_call() {
    let Fixture {
        worker: mut w,
        controller: _c,
        store,
        d: _d,
        i: _i,
        server,
        guard: _guard,
        data: f,
        first,
        second,
        owner,
    } = fixture(true).await;
    w.refresh().await.unwrap();
    let target = w.live.values().next().unwrap().summary.id.clone();
    let a = prepare(
        &mut w,
        &first,
        "answer-a",
        CallAction::Accept,
        Some(target.clone()),
        "",
    )
    .await;
    let b = prepare(
        &mut w,
        &second,
        "answer-b",
        CallAction::Accept,
        Some(target),
        "",
    )
    .await;
    let value = submit(&mut w, &first, &a, None).await.unwrap();
    assert_eq!(value["operation"]["state"], "submitted");
    assert!(submit(&mut w, &second, &b, None).await.is_err());
    assert_eq!(f.accepts.load(Ordering::SeqCst), 1);
    let lease: CallLease = serde_json::from_value(value["lease"].clone()).unwrap();
    assert!(w
        .process(AuthCommand::RenewCall {
            token: second,
            lease: lease.clone()
        })
        .await
        .is_err());
    let principal = w.principal(&first, Scope::CallsControl).await.unwrap();
    store
        .handle()
        .revoke_client(owner, principal.public_key)
        .unwrap();
    assert!(w
        .process(AuthCommand::RenewCall {
            token: first,
            lease
        })
        .await
        .is_err());
    w.expire().await;
    w.expire().await;
    assert_eq!(f.hangups.load(Ordering::SeqCst), 1);
    w.refresh().await.unwrap();
    assert!(w.owned.is_none());
    server.close().await.unwrap();
}
#[tokio::test]
async fn private_voice_expiry_does_not_let_failed_hangup_become_idle_or_new_dial() {
    let Fixture {
        worker: mut w,
        controller: _c,
        store: _store,
        d: _d,
        i: _i,
        server,
        guard: _guard,
        data: f,
        first: token,
        second: other,
        owner: _owner,
    } = fixture(false).await;
    let id = prepare(
        &mut w,
        &token,
        "dial",
        CallAction::Dial,
        None,
        "+12025550123",
    )
    .await;
    let value = submit(&mut w, &token, &id, None).await.unwrap();
    let lease: CallLease = serde_json::from_value(value["lease"].clone()).unwrap();
    w.advance_ms += 31_000;
    f.mode.store(4, Ordering::SeqCst);
    assert!(w
        .process(AuthCommand::RenewCall {
            token: token.clone(),
            lease
        })
        .await
        .is_err());
    w.expire().await;
    w.expire().await;
    assert_eq!(f.hangups.load(Ordering::SeqCst), 1);
    assert_eq!(f.state.load(Ordering::SeqCst), 4);
    let next = prepare(
        &mut w,
        &other,
        "other-dial",
        CallAction::Dial,
        None,
        "+12025550124",
    )
    .await;
    assert!(submit(&mut w, &other, &next, None).await.is_err());
    assert_eq!(f.creates.load(Ordering::SeqCst), 1);
    server.close().await.unwrap();
}
#[tokio::test]
async fn foreign_call_blocks_dial_and_read_only_mode_cannot_create_control_intents() {
    let Fixture {
        worker: mut w,
        controller: _c,
        store: _store,
        d: _d,
        i: _i,
        server,
        guard: _guard,
        data: f,
        first: token,
        second: _other,
        owner: _owner,
    } = fixture(true).await;
    w.refresh().await.unwrap();
    let id = prepare(
        &mut w,
        &token,
        "dial",
        CallAction::Dial,
        None,
        "+12025550123",
    )
    .await;
    assert!(submit(&mut w, &token, &id, None).await.is_err());
    assert_eq!(f.creates.load(Ordering::SeqCst), 0);
    assert_eq!(f.hangups.load(Ordering::SeqCst), 0);
    w.writes = false;
    let line = w.binding().unwrap().context.line.0;
    assert!(w
        .process(AuthCommand::PrepareCall {
            token,
            line_epoch: line,
            idempotency_key: "b".into(),
            action: CallAction::Dial,
            call_id: None,
            payload: "+12025550123".into()
        })
        .await
        .is_err());
    server.close().await.unwrap();
}
#[tokio::test]
async fn simultaneous_answer_requests_and_normal_shutdown_have_one_owner_and_one_cleanup() {
    let Fixture {
        worker: mut w,
        controller,
        store: _store,
        d: _d,
        i: _i,
        server,
        guard: _guard,
        data: f,
        first,
        second,
        owner: _owner,
    } = fixture(true).await;
    w.refresh().await.unwrap();
    let target = w.live.values().next().unwrap().summary.id.clone();
    let a = prepare(
        &mut w,
        &first,
        "a",
        CallAction::Accept,
        Some(target.clone()),
        "",
    )
    .await;
    let b = prepare(&mut w, &second, "b", CallAction::Accept, Some(target), "").await;
    let exercise = async {
        let (a, b) = tokio::join!(
            controller.request(AuthCommand::SubmitCall {
                token: first,
                operation_id: a,
                lease: None,
                urgent: false
            }),
            controller.request(AuthCommand::SubmitCall {
                token: second,
                operation_id: b,
                lease: None,
                urgent: false
            })
        );
        assert!(a.is_ok() ^ b.is_ok());
        assert_eq!(f.accepts.load(Ordering::SeqCst), 1);
        controller.shutdown();
    };
    tokio::time::timeout(Duration::from_secs(15), async {
        tokio::join!(w.run(), exercise);
    })
    .await
    .unwrap();
    assert_eq!(f.hangups.load(Ordering::SeqCst), 1);
    assert_eq!(f.state.load(Ordering::SeqCst), 7);
    server.close().await.unwrap();
}
