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
            rustsimhub_policy::mask(&[Scope::CallsRead, Scope::CallsControl]).unwrap(),
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
    w.start -= Duration::from_secs(31);
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
