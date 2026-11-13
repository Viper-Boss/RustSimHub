// SPDX-License-Identifier: AGPL-3.0-only
//! Real Auth and Store, a synthetic owner and an app-side signer. No modem,
//! no socket; every key, code and message is synthetic.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_airsim::{base64url, canonical, decode_base64url, seal_registration, sign};
use rustsimhub_auth::Identity;
use rustsimhub_storage::{IncomingSms, LineObservation, Store};
use std::os::unix::fs::PermissionsExt;
use x25519_dalek::{PublicKey, StaticSecret};

const PEER: &str = "192.168.1.20";

/// A scripted modem behind the call controller's interface. It enforces the
/// same lease rules Core does, so the gateway cannot pass by skipping them.
#[derive(Default)]
struct FakeCalls {
    state: Mutex<Modem>,
}
#[derive(Default)]
struct Modem {
    live: Vec<(String, i64, i64)>, // id, direction, modem_state
    owned: Option<(String, u64)>,  // call id, lease generation
    operations: Vec<(
        String,
        rustsimhub_domain::CallAction,
        Option<String>,
        String,
    )>,
    log: Vec<String>,
    renewals: u32,
    submit_state: Option<&'static str>,
    fail: Option<&'static str>,
    refuse_renew: bool,
    unavailable: bool,
    next: u32,
}
impl FakeCalls {
    fn with<T>(&self, f: impl FnOnce(&mut Modem) -> T) -> T {
        f(&mut self.state.lock().unwrap())
    }
    fn ring(&self, id: &str) {
        self.with(|m| m.live.push((id.into(), 1, 3)));
    }
    fn set(&self, id: &str, modem_state: i64) {
        self.with(|m| {
            if modem_state == 7 {
                m.live.retain(|c| c.0 != id);
                if m.owned.as_ref().is_some_and(|o| o.0 == id) {
                    m.owned = None;
                }
            } else if let Some(call) = m.live.iter_mut().find(|c| c.0 == id) {
                call.2 = modem_state;
            }
        });
    }
    fn log(&self) -> Vec<String> {
        self.with(|m| std::mem::take(&mut m.log))
    }
}
fn fake_lease(call_id: &str, generation: u64) -> Value {
    json!({"call_id": call_id, "boot": "synthetic-boot", "line": 1, "generation": generation, "expires_in_seconds": 30})
}
impl CallPort for FakeCalls {
    fn request(
        &self,
        _: &tokio::runtime::Handle,
        command: AuthCommand,
    ) -> Result<Value, &'static str> {
        use rustsimhub_domain::CallAction as A;
        self.with(|m| match command {
            AuthCommand::CallStatus { .. } => Ok(json!({
                "available": !m.unavailable,
                "live_calls": m.live.iter().map(|(id, direction, state)| json!({
                    "id": id, "line": 1, "direction": direction, "modem_state": state,
                    "availability": "observed", "state": "synthetic",
                })).collect::<Vec<_>>(),
            })),
            AuthCommand::PrepareCall { idempotency_key, action, call_id, payload, .. } => {
                if let Some(code) = m.fail {
                    return Err(code);
                }
                if !m.operations.iter().any(|o| o.0 == idempotency_key) {
                    m.operations.push((idempotency_key.clone(), action, call_id.clone(), payload));
                }
                Ok(json!({"operation": {"id": idempotency_key, "state": "prepared", "call_id": call_id}, "lease": null}))
            }
            AuthCommand::SubmitCall { operation_id, lease, urgent, .. } => {
                let (_, action, call_id, payload) = m
                    .operations
                    .iter()
                    .find(|o| o.0 == operation_id)
                    .cloned()
                    .ok_or("unknown_operation")?;
                let held = lease.as_ref().map(|l| (l.call_id.clone(), l.generation));
                m.log.push(format!("{} {} lease={} urgent={urgent}", action.name(), call_id.as_deref().unwrap_or(&payload), held.is_some()));
                if let Some(state) = m.submit_state {
                    return Ok(json!({"operation": {"id": operation_id, "state": state, "call_id": call_id}, "lease": null}));
                }
                let id = match action {
                    A::Dial => {
                        if m.owned.is_some() { return Err("call_owned"); }
                        if !m.live.is_empty() { return Err("modem_busy"); }
                        m.next += 1;
                        let id = format!("call-{}", m.next);
                        m.live.push((id.clone(), 2, 1));
                        m.owned = Some((id.clone(), u64::from(m.next)));
                        id
                    }
                    A::Accept => {
                        let id = call_id.ok_or("invalid")?;
                        if m.owned.is_some() { return Err("call_owned"); }
                        let call = m.live.iter_mut().find(|c| c.0 == id && c.2 == 3).ok_or("call_unavailable")?;
                        call.2 = 4;
                        m.next += 1;
                        m.owned = Some((id.clone(), u64::from(m.next)));
                        id
                    }
                    A::Hangup => {
                        let id = call_id.ok_or("invalid")?;
                        let ringing = m.live.iter().any(|c| c.0 == id && c.1 == 1 && c.2 == 3);
                        // Core's rule: the holder's lease, or an unowned ringing incoming call.
                        if held != m.owned.clone().filter(|o| o.0 == id) || (held.is_none() && !ringing) {
                            return Err("lease_required");
                        }
                        m.live.retain(|c| c.0 != id);
                        m.owned = None;
                        id
                    }
                    A::Dtmf => {
                        let id = call_id.ok_or("invalid")?;
                        if held.is_none() || held != m.owned { return Err("lease_required"); }
                        id
                    }
                };
                let lease = m.owned.as_ref().filter(|o| o.0 == id).map(|o| fake_lease(&o.0, o.1));
                Ok(json!({"operation": {"id": operation_id, "state": "submitted", "call_id": id}, "lease": lease}))
            }
            AuthCommand::RenewCall { lease, .. } => {
                if m.refuse_renew || m.owned != Some((lease.call_id.clone(), lease.generation)) {
                    return Err("lease_expired");
                }
                m.renewals += 1;
                Ok(json!({"lease": fake_lease(&lease.call_id, lease.generation)}))
            }
            _ => Err("invalid_call_command"),
        })
    }
}

struct Fixture {
    _dirs: (tempfile::TempDir, tempfile::TempDir, tempfile::TempDir),
    store: Store,
    auth: Arc<Mutex<Auth>>,
    owner: String,
    state: PathBuf,
    controller: Controller,
    calls: Arc<FakeCalls>,
    secret: [u8; 32],
    counter: u32,
}
fn directory() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    d
}
fn login(auth: &mut Auth, key: &SigningKey) -> String {
    let c = auth
        .login_challenge(key.verifying_key().to_bytes())
        .unwrap();
    auth.login_finish(&c.id, &key.sign(&c.transcript).to_bytes())
        .unwrap()
        .expose()
        .to_owned()
}
fn fixture() -> Fixture {
    let (data, identity, state) = (directory(), directory(), directory());
    let store = Store::open(data.path()).unwrap();
    let mut auth = Auth::new(Identity::open(identity.path()).unwrap(), store.handle()).unwrap();
    let key = SigningKey::from_bytes(&[113; 32]);
    let c = auth
        .bootstrap_challenge(key.verifying_key().to_bytes())
        .unwrap();
    auth.bootstrap_finish(&c.id, &key.sign(&c.transcript).to_bytes())
        .unwrap();
    let owner = login(&mut auth, &key);
    store
        .handle()
        .observe_line(LineObservation {
            backend: "synthetic".into(),
            fingerprint: Some([1; 32]),
            status: LineStatus::Ready,
        })
        .unwrap();
    let path = state.path().to_owned();
    let calls = Arc::new(FakeCalls::default());
    Fixture {
        controller: Controller::open(&path, None, Some(calls.clone() as Arc<dyn CallPort>))
            .unwrap(),
        calls,
        state: path,
        _dirs: (data, identity, state),
        store,
        auth: Arc::new(Mutex::new(auth)),
        owner,
        secret: [0x42; 32],
        counter: 0,
    }
}
impl Fixture {
    async fn ask(&self, command: AuthCommand) -> Result<Value, Error> {
        self.controller.request(self.auth.clone(), command).await
    }
    async fn configure(&self, enabled: bool, pcm_rate: u32) -> Result<Value, Error> {
        self.ask(AuthCommand::AirsimConfigure {
            token: self.owner.clone(),
            enabled,
            pcm_rate,
        })
        .await
    }
    fn receive(&self, n: u32, text: &str) -> String {
        let h = self.store.handle();
        h.receive_sms(IncomingSms {
            line: h.line_epoch().unwrap(),
            source: format!("synthetic-source-{n}"),
            sender: "synthetic-sender".into(),
            timestamp: "2026-11-02T14:13:20+08:00".into(),
            text: text.into(),
            data: vec![],
        })
        .unwrap()
        .id
    }
    /// Enables the gateway and pairs one synthetic phone, as the app would.
    async fn pair(&mut self) -> Value {
        self.configure(true, 8_000).await.unwrap();
        let opened = self
            .ask(AuthCommand::AirsimOpenPairing {
                token: self.owner.clone(),
            })
            .await
            .unwrap();
        let code = opened["code"].as_str().unwrap().to_owned();
        let info = self.pairing("GET", "/pair/v1/info", "").await;
        assert_eq!(info["status"], 200, "{info}");
        let session = info["body"]["session_id"].as_str().unwrap().to_owned();
        let server = decode_base64url(info["body"]["server_public_key"].as_str().unwrap()).unwrap();
        let client = StaticSecret::from([5u8; 32]);
        let registration =
            json!({"vowlan_secret": base64url(&self.secret), "device_id": "synthetic-AB12"});
        let sealed = seal_registration(
            &client,
            &server,
            registration.to_string().as_bytes(),
            &session,
            &code,
            [3; 12],
        )
        .unwrap();
        let claim = json!({"version": 1, "session_id": session,
            "client_public_key": base64url(PublicKey::from(&client).as_bytes()),
            "sealed_registration": base64url(&sealed)});
        self.pairing("POST", "/pair/v1/claim", &claim.to_string())
            .await
    }
    async fn pairing(&self, method: &str, target: &str, body: &str) -> Value {
        self.ask(AuthCommand::AirsimPairing {
            method: method.into(),
            target: target.into(),
            body: body.into(),
            peer: PEER.into(),
        })
        .await
        .unwrap()
    }
    /// A request signed the way the app's VoWLANAuth does.
    async fn control(&mut self, method: &str, target: &str, body: &str) -> Value {
        self.counter += 1;
        let nonce = format!("nonce-{:06}", self.counter);
        let at = now_ms().div_euclid(1000);
        let signature = sign(
            &self.secret,
            &canonical(method, target, body.as_bytes(), at, &nonce),
        );
        self.ask(AuthCommand::AirsimControl {
            method: method.into(),
            target: target.into(),
            version: Some("1".into()),
            timestamp: Some(at.to_string()),
            nonce: Some(nonce),
            signature: Some(signature),
            body: body.into(),
            peer: PEER.into(),
        })
        .await
        .unwrap()
    }
    fn clients(&self) -> usize {
        self.auth
            .lock()
            .unwrap()
            .clients(&self.owner)
            .unwrap()
            .iter()
            .filter(|p| p.kind == Kind::Client && p.active)
            .count()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn management_needs_the_owner_and_the_gateway_starts_disabled() {
    let f = fixture();
    let status = f
        .ask(AuthCommand::AirsimStatus {
            token: f.owner.clone(),
        })
        .await
        .unwrap();
    assert_eq!(status["enabled"], false);
    assert_eq!(status["pcm_rate"], 8000);
    assert_eq!(status["pairings"], json!([]));
    assert_eq!(status["calls_supported"], true);
    assert_eq!(status["audio_supported"], false);
    assert_eq!(f.clients(), 0);
    assert!(
        !f.state.join("airsim.json").exists(),
        "reading status writes nothing"
    );

    let stranger = "0".repeat(64);
    for command in [
        AuthCommand::AirsimStatus {
            token: stranger.clone(),
        },
        AuthCommand::AirsimConfigure {
            token: stranger.clone(),
            enabled: true,
            pcm_rate: 8_000,
        },
        AuthCommand::AirsimOpenPairing {
            token: stranger.clone(),
        },
        AuthCommand::AirsimClosePairing {
            token: stranger.clone(),
        },
        AuthCommand::AirsimRevoke {
            token: stranger.clone(),
            pairing_id: "0".repeat(24),
        },
    ] {
        assert!(matches!(f.ask(command).await, Err(Error::Forbidden)));
    }
    assert!(
        matches!(
            f.ask(AuthCommand::AirsimOpenPairing {
                token: f.owner.clone()
            })
            .await,
            Err(Error::Forbidden)
        ),
        "no pairing window while disabled"
    );
    assert_eq!(f.pairing("GET", "/pair/v1/info", "").await["status"], 410);
    assert!(matches!(
        f.configure(true, 44_100).await,
        Err(Error::InvalidInput)
    ));
    assert_eq!(f.clients(), 0, "a refused setting creates no principal");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_paired_phone_reads_status_and_only_messages_received_after_pairing() {
    let mut f = fixture();
    f.receive(1, "before pairing");
    let claimed = f.pair().await;
    assert_eq!(claimed["status"], 200, "{claimed}");
    assert_eq!(claimed["body"]["control_port"], 7592);
    assert_eq!(
        f.clients(),
        1,
        "the gateway is one ordinary client the owner can see"
    );

    let file = f.state.join("airsim.json");
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let stored = std::fs::read_to_string(&file).unwrap();
    assert!(stored.contains("\"enabled\":true"));

    let health = f.control("GET", "/v1/health", "").await;
    assert_eq!(health["status"], 200, "{health}");
    assert_eq!(health["body"]["agent_kind"], "rustsimhub");
    assert_eq!(
        f.control("GET", "/api/health", "").await["body"]["cellular_state"],
        "registered"
    );
    assert_eq!(
        f.control("GET", "/api/sms", "").await["body"],
        json!([]),
        "older messages stay private"
    );

    let id = f.receive(2, "您的验证码是 482916");
    let list = f.control("GET", "/api/sms", "").await;
    assert_eq!(list["body"].as_array().unwrap().len(), 1, "{list}");
    assert_eq!(list["body"][0]["delivery_id"], id);
    assert_eq!(list["body"][0]["content"], "您的验证码是 482916");
    assert_eq!(list["body"][0]["code"], "482916");
    assert_eq!(list["body"][0]["timestamp"], "2026-11-02T06:13:20.000Z");
    let ack = json!({"ids": [id]}).to_string();
    assert_eq!(
        f.control("POST", "/api/sms/ack", &ack).await["body"]["acknowledged"],
        1
    );
    assert_eq!(f.control("GET", "/api/sms", "").await["body"], json!([]));

    assert_eq!(
        f.control("GET", "/api/calls/status", "").await["body"]["active"],
        Value::Null
    );
    let send = f
        .control(
            "POST",
            "/api/sms/send",
            r#"{"phone":"10010","message":"x"}"#,
        )
        .await;
    assert_eq!(
        send["status"], 503,
        "no sender in this fixture: nothing is claimed as sent"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disabling_revoking_and_owner_revocation_cut_the_phone_off() {
    let mut f = fixture();
    assert_eq!(f.pair().await["status"], 200);
    assert_eq!(f.control("GET", "/api/health", "").await["status"], 200);

    f.configure(false, 8_000).await.unwrap();
    assert_eq!(
        f.clients(),
        0,
        "disabling withdraws the gateway's authority"
    );
    assert_eq!(f.control("GET", "/api/health", "").await["status"], 503);
    assert_eq!(f.pairing("GET", "/pair/v1/info", "").await["status"], 410);

    let status = f.configure(true, 16_000).await.unwrap();
    assert_eq!(status["pcm_rate"], 16000);
    assert_eq!(
        status["pairings"].as_array().unwrap().len(),
        1,
        "the pairing survived the pause"
    );
    assert_eq!(f.clients(), 1);
    assert_eq!(f.control("GET", "/api/health", "").await["status"], 200);

    // The owner removes the gateway from the ordinary client list.
    let seed: Value =
        serde_json::from_str(&std::fs::read_to_string(f.state.join("airsim.json")).unwrap())
            .unwrap();
    let seed = <[u8; 32]>::try_from(&decode_base64url(seed["seed"].as_str().unwrap()).unwrap()[..])
        .unwrap();
    f.auth
        .lock()
        .unwrap()
        .revoke(&f.owner, Auth::gateway_public_key(&seed))
        .unwrap();
    assert_eq!(f.control("GET", "/api/health", "").await["status"], 401);
    let status = f
        .ask(AuthCommand::AirsimStatus {
            token: f.owner.clone(),
        })
        .await
        .unwrap();
    assert_eq!(
        status["pairings"],
        json!([]),
        "a phone whose authority is gone is unpaired"
    );

    let mut f = fixture();
    assert_eq!(f.pair().await["status"], 200);
    let status = f
        .ask(AuthCommand::AirsimStatus {
            token: f.owner.clone(),
        })
        .await
        .unwrap();
    let id = status["pairings"][0]["id"].as_str().unwrap().to_owned();
    assert!(!status.to_string().contains("secret"));
    let revoked = f
        .ask(AuthCommand::AirsimRevoke {
            token: f.owner.clone(),
            pairing_id: id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(revoked["revoked"], true);
    assert_eq!(f.control("GET", "/api/health", "").await["status"], 401);
    let again = f
        .ask(AuthCommand::AirsimRevoke {
            token: f.owner.clone(),
            pairing_id: id,
        })
        .await
        .unwrap();
    assert_eq!(again["revoked"], false);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pairings_survive_a_restart_and_damaged_state_is_left_alone() {
    let mut f = fixture();
    assert_eq!(f.pair().await["status"], 200);
    f.controller = Controller::open(&f.state, None, None).unwrap();
    assert_eq!(
        f.control("GET", "/v1/health", "").await["status"],
        200,
        "restart keeps the pairing"
    );
    let id = f.receive(7, "after restart");
    assert_eq!(
        f.control("GET", "/api/sms", "").await["body"][0]["delivery_id"],
        id
    );

    let file = f.state.join("airsim.json");
    for damage in ["not json", "{}", r#"{"format":2}"#] {
        std::fs::write(&file, damage).unwrap();
        assert!(
            matches!(Controller::open(&f.state, None, None), Err(Error::Storage)),
            "{damage}"
        );
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            damage,
            "never overwritten"
        );
    }
    std::fs::remove_file(&file).unwrap();
    std::os::unix::fs::symlink("/dev/null", &file).unwrap();
    assert!(
        matches!(Controller::open(&f.state, None, None), Err(Error::Storage)),
        "a link is not followed"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_held_poll_is_continued_by_ticket_until_something_changes() {
    let mut f = fixture();
    assert_eq!(f.pair().await["status"], 200);
    let first = f.control("GET", "/api/events", "").await;
    let revision = first["body"]["revision"].as_u64().unwrap();
    let held = f
        .control(
            "GET",
            &format!("/api/events?after={revision}&timeout_ms=30000"),
            "",
        )
        .await;
    assert_eq!(held["wait"], true, "{held}");
    let ticket = held["ticket"].as_str().unwrap().to_owned();
    let poll = |ticket: String| AuthCommand::AirsimPoll { ticket };
    assert_eq!(f.ask(poll(ticket.clone())).await.unwrap()["wait"], true);

    f.receive(3, "new message");
    let changed = f.ask(poll(ticket.clone())).await.unwrap();
    assert_eq!(changed["status"], 200, "{changed}");
    assert_eq!(changed["body"]["sms_pending"], 1);
    assert_ne!(changed["body"]["revision"].as_u64().unwrap(), revision);
    assert_eq!(
        f.ask(poll(ticket)).await.unwrap()["status"],
        410,
        "a ticket answers once"
    );
    assert_eq!(f.ask(poll("0".repeat(32))).await.unwrap()["status"], 410);

    let now = f
        .control("GET", "/api/events?after=1&timeout_ms=0", "")
        .await;
    assert_eq!(now["status"], 200);
}

#[test]
fn modem_timestamps_are_parsed_strictly_or_reported_unknown() {
    assert_eq!(sms_time_ms("2026-11-02T06:13:20Z"), Some(1_793_600_000_000));
    assert_eq!(sms_time_ms("2026-11-02T06:13:20"), Some(1_793_600_000_000));
    assert_eq!(
        sms_time_ms("2026-11-02T14:13:20+08:00"),
        Some(1_793_600_000_000)
    );
    assert_eq!(
        sms_time_ms("2026-11-01T22:13:20-08:00"),
        Some(1_793_600_000_000)
    );
    assert_eq!(sms_time_ms("2000-02-29T00:00:00Z"), Some(951_782_400_000));
    for bad in [
        "",
        "synthetic",
        "2026-13-02T06:13:20Z",
        "2026-11-02 06:13:20",
        "2026-11-02T25:13:20Z",
        "2026-11-02T06:13:20+0800",
        "2026-11-02T06:13:20+08:0x",
        "2026-11-0２T06:13:20Z",
    ] {
        assert_eq!(sms_time_ms(bad), None, "{bad:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_outgoing_call_is_dialled_followed_and_hung_up_with_the_gateways_lease() {
    let mut f = fixture();
    assert_eq!(f.pair().await["status"], 200);
    let dialled = f
        .control(
            "POST",
            "/api/calls/dial",
            r#"{"number":"100 10","operation_id":"op-1"}"#,
        )
        .await;
    assert_eq!(dialled["status"], 200, "{dialled}");
    assert_eq!(dialled["body"]["call_id"], "call-1");
    assert!(dialled["body"]["operation_proof"].is_string());
    assert_eq!(f.calls.log(), ["dial 10010 lease=false urgent=false"]);

    // The app repeats the same operation: Core's idempotency key is the same, nothing is dialled twice.
    let again = f
        .control(
            "POST",
            "/api/calls/dial",
            r#"{"number":"100 10","operation_id":"op-1"}"#,
        )
        .await;
    assert_eq!(f.calls.with(|m| m.operations.len()), 1, "{again}");

    let status = f.control("GET", "/api/calls/status", "").await;
    assert_eq!(status["body"]["active"]["state"], "dialing", "{status}");
    assert_eq!(status["body"]["active"]["number"], "10010");
    assert_eq!(status["body"]["active"]["direction"], "outgoing");
    f.calls.set("call-1", 2);
    assert_eq!(
        f.control("GET", "/api/calls/status", "").await["body"]["active"]["state"],
        "alerting"
    );
    f.calls.set("call-1", 4);
    assert_eq!(
        f.control("GET", "/api/calls/status", "").await["body"]["active"]["state"],
        "active"
    );
    assert!(
        f.calls.with(|m| m.renewals) >= 2,
        "a present phone keeps its call"
    );

    f.calls.log();
    assert_eq!(
        f.control("POST", "/api/calls/dtmf", r##"{"digit":"#"}"##)
            .await["status"],
        200
    );
    assert_eq!(
        f.control("POST", "/api/calls/hangup", "").await["status"],
        200
    );
    assert_eq!(
        f.calls.log(),
        [
            "dtmf call-1 lease=true urgent=false",
            "hangup call-1 lease=true urgent=true"
        ]
    );
    let ended = f.control("GET", "/api/calls/status", "").await;
    assert_eq!(ended["body"]["active"], Value::Null);
    assert_eq!(ended["body"]["history"][0]["id"], "call-1");
    assert_eq!(ended["body"]["history"][0]["state"], "ended");
    assert_eq!(ended["body"]["history"][0]["missed"], false);
    assert!(ended["body"]["history"][0]["ended_at"].is_string());
    assert_eq!(
        f.control("POST", "/api/calls/hangup", "").await["status"],
        200,
        "hanging up nothing is idempotent"
    );
    assert!(f.calls.log().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn incoming_calls_ring_and_can_be_answered_rejected_or_missed() {
    let mut f = fixture();
    assert_eq!(f.pair().await["status"], 200);
    let first = f.control("GET", "/api/events", "").await;
    let revision = first["body"]["revision"].as_u64().unwrap();

    f.calls.ring("in-1");
    let ringing = f
        .control(
            "GET",
            &format!("/api/events?after={revision}&timeout_ms=0"),
            "",
        )
        .await;
    assert_eq!(
        ringing["body"]["call"]["active"]["state"], "incoming",
        "{ringing}"
    );
    assert_eq!(ringing["body"]["call"]["active"]["direction"], "incoming");
    assert_ne!(ringing["body"]["revision"].as_u64().unwrap(), revision);

    assert_eq!(
        f.control("POST", "/api/calls/answer", r#"{"call_id":"in-1"}"#)
            .await["status"],
        200
    );
    assert_eq!(f.calls.log(), ["accept in-1 lease=false urgent=false"]);
    assert_eq!(
        f.control("GET", "/api/calls/status", "").await["body"]["active"]["state"],
        "active"
    );
    f.calls.set("in-1", 7);
    let after = f.control("GET", "/api/calls/status", "").await;
    assert_eq!(
        after["body"]["history"][0]["missed"], false,
        "an answered call is not missed"
    );

    f.calls.ring("in-2");
    f.control("GET", "/api/calls/status", "").await;
    assert_eq!(
        f.control("POST", "/api/calls/reject", "").await["body"]["rejected"],
        true
    );
    assert_eq!(
        f.calls.log(),
        ["hangup in-2 lease=false urgent=true"],
        "an unowned ringing call is refused without a lease"
    );

    f.calls.ring("in-3");
    f.control("GET", "/api/calls/status", "").await;
    f.calls.set("in-3", 7);
    let history = f.control("GET", "/api/calls/status", "").await["body"]["history"].clone();
    assert_eq!(history[0]["id"], "in-3");
    assert_eq!(
        history[0]["missed"], true,
        "rang and ended without an answer"
    );
    assert_eq!(history.as_array().unwrap().len(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn calls_the_gateway_does_not_control_and_uncertain_results_are_refused_honestly() {
    let mut f = fixture();
    assert_eq!(f.pair().await["status"], 200);

    // A call another client (the web page) dialled: visible, but not ours to end.
    f.calls.with(|m| {
        m.live.push(("web-1".into(), 2, 4));
        m.owned = Some(("web-1".into(), 99));
    });
    assert_eq!(
        f.control("GET", "/api/calls/status", "").await["body"]["active"]["id"],
        "web-1"
    );
    let hangup = f.control("POST", "/api/calls/hangup", "").await;
    assert_eq!(
        (hangup["status"].as_u64(), hangup["body"]["error"].as_str()),
        (Some(409), Some("这通电话不由本机控制"))
    );
    assert_eq!(
        f.control("POST", "/api/calls/dtmf", r#"{"digit":"1"}"#)
            .await["status"],
        409
    );
    assert!(f.calls.log().is_empty(), "nothing was sent to the modem");
    let busy = f
        .control("POST", "/api/calls/dial", r#"{"number":"10010"}"#)
        .await;
    assert_eq!(
        (busy["status"].as_u64(), busy["body"]["error"].as_str()),
        (Some(409), Some("线路正由其他设备控制"))
    );
    f.calls.set("web-1", 7);
    f.calls.log();

    for bad in [
        r#"{"number":"abc"}"#,
        r#"{"number":"12"}"#,
        r#"{"number":"+"}"#,
    ] {
        assert_eq!(
            f.control("POST", "/api/calls/dial", bad).await["status"],
            400,
            "{bad}"
        );
    }
    assert!(f.calls.log().is_empty());

    f.calls.with(|m| m.submit_state = Some("unknown"));
    let unknown = f
        .control("POST", "/api/calls/dial", r#"{"number":"10010"}"#)
        .await;
    assert_eq!(
        (
            unknown["status"].as_u64(),
            unknown["body"]["error"].as_str()
        ),
        (Some(502), Some("结果未确认，请勿重复操作"))
    );
    assert_eq!(
        f.calls.log().len(),
        1,
        "an uncertain dial is reported once and never retried"
    );
    f.calls.with(|m| m.submit_state = None);

    f.calls.with(|m| m.fail = Some("line_unavailable"));
    assert_eq!(
        f.control("POST", "/api/calls/dial", r#"{"number":"10010"}"#)
            .await["status"],
        503
    );
    f.calls.with(|m| m.fail = None);

    f.calls.with(|m| m.unavailable = true);
    assert_eq!(
        f.control("GET", "/api/calls/status", "").await["status"],
        503,
        "an unconfirmed catalogue is not an empty one"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_lease_ends_the_phones_control_and_another_phone_cannot_take_it() {
    let mut f = fixture();
    assert_eq!(f.pair().await["status"], 200);
    assert_eq!(
        f.control("POST", "/api/calls/dial", r#"{"number":"10010"}"#)
            .await["status"],
        200
    );

    // A second paired phone sees the call but may not hang it up.
    let first_secret = f.secret;
    f.secret = [0x55; 32];
    assert_eq!(f.pair().await["status"], 200);
    assert_eq!(
        f.control("GET", "/api/calls/status", "").await["body"]["active"]["id"],
        "call-1"
    );
    let other = f.control("POST", "/api/calls/hangup", "").await;
    assert_eq!(
        (other["status"].as_u64(), other["body"]["error"].as_str()),
        (Some(409), Some("通话由另一台手机控制"))
    );
    let renewals = f.calls.with(|m| m.renewals);
    f.control("GET", "/api/calls/status", "").await;
    assert_eq!(
        f.calls.with(|m| m.renewals),
        renewals,
        "only the controlling phone keeps the lease alive"
    );

    // Core stops honouring the lease (the phone was away too long).
    f.secret = first_secret;
    f.calls.with(|m| m.refuse_renew = true);
    f.control("GET", "/api/calls/status", "").await;
    f.calls.log();
    let lost = f.control("POST", "/api/calls/hangup", "").await;
    assert_eq!(
        (lost["status"].as_u64(), lost["body"]["error"].as_str()),
        (Some(409), Some("这通电话不由本机控制"))
    );
    assert!(
        f.calls.log().is_empty(),
        "no hangup is sent with a lease Core no longer honours"
    );
}

#[test]
fn dialled_numbers_are_normalised_or_refused() {
    assert_eq!(dial_number("100 10").as_deref(), Some("10010"));
    assert_eq!(
        dial_number("+86 (138) 0000-0000").as_deref(),
        Some("+8613800000000")
    );
    for bad in [
        "",
        "12",
        "+",
        "10010#",
        "1001０",
        "１００１０",
        "10+010",
        &"1".repeat(33),
    ] {
        assert_eq!(dial_number(bad), None, "{bad:?}");
    }
}
