// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::authenticated::{handle, Providers};
use crate::linux::{read_frame, write_frame};
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_auth::Identity;
use rustsimhub_domain::LineEpoch;
use rustsimhub_policy::{Kind, Scope};
use rustsimhub_storage::{IncomingSms, LineObservation, LineStatus, Store};
use rustsimhub_wire::{AuthRequest, Response, AUTH_PROTOCOL};
use std::os::unix::fs::{symlink, PermissionsExt};
use tokio::net::UnixStream;

pub(super) const REQUEST: &str = "11111111-1111-4111-8111-111111111111";
pub(super) const CHANGE: &str = "22222222-2222-4222-8222-222222222222";
const TARGET: &str = "33333333-3333-4333-8333-333333333333";
const RULE: &str = "44444444-4444-4444-8444-444444444444";
fn dir() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    d
}
fn login(a: &mut Auth, key: &SigningKey) -> String {
    let c = a.login_challenge(key.verifying_key().to_bytes()).unwrap();
    a.login_finish(&c.id, &key.sign(&c.transcript).to_bytes())
        .unwrap()
        .expose()
        .to_owned()
}
pub(super) struct Fixture {
    pub(super) state: tempfile::TempDir,
    _identity: tempfile::TempDir,
    pub(super) store: Store,
    pub(super) auth: Arc<Mutex<Auth>>,
    pub(super) control: Controller,
    token: String,
    line: LineEpoch,
}
impl Fixture {
    pub(super) fn new() -> Self {
        let state = dir();
        let identity = dir();
        let store = Store::open(state.path()).unwrap();
        let mut a = Auth::new(Identity::open(identity.path()).unwrap(), store.handle()).unwrap();
        let key = SigningKey::from_bytes(&[126; 32]);
        let c = a
            .bootstrap_challenge(key.verifying_key().to_bytes())
            .unwrap();
        a.bootstrap_finish(&c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap();
        let token = login(&mut a, &key);
        let line = store
            .handle()
            .observe_line(LineObservation {
                backend: "synthetic modem".into(),
                fingerprint: Some([19; 32]),
                status: LineStatus::Ready,
            })
            .unwrap()
            .line;
        let control = Controller::new(state.path(), store.handle());
        Self {
            state,
            _identity: identity,
            store,
            auth: Arc::new(Mutex::new(a)),
            control,
            token,
            line,
        }
    }
    pub(super) async fn request(&self, command: AuthCommand) -> Value {
        rpc(self.auth.clone(), Some(self.control.clone()), command).await
    }
    pub(super) fn configure(&self, expected: u64, request: &str, enabled: bool) -> AuthCommand {
        AuthCommand::ForwardingConfigure {
            token: self.token.clone(),
            expected_generation: expected,
            request_id: request.into(),
            policy: policy(enabled),
        }
    }
    pub(super) fn incoming(&self, name: &str) {
        self.store
            .handle()
            .receive_sms(IncomingSms {
                line: self.line,
                source: name.into(),
                sender: "synthetic sender".into(),
                timestamp: "synthetic time".into(),
                text: format!("synthetic private body {name}"),
                data: vec![],
            })
            .unwrap();
    }
}
async fn rpc(auth: Arc<Mutex<Auth>>, control: Option<Controller>, command: AuthCommand) -> Value {
    let (mut client, mut server) = UnixStream::pair().unwrap();
    let task = tokio::spawn(async move {
        handle(
            &mut server,
            auth,
            None,
            None,
            None,
            Providers {
                forwarding: control,
                ..Providers::default()
            },
        )
        .await
        .unwrap();
    });
    let bytes = serde_json::to_vec(&AuthRequest {
        protocol: AUTH_PROTOCOL,
        id: "forwarding-test".into(),
        command,
    })
    .unwrap();
    write_frame(&mut client, &bytes).await.unwrap();
    let response: Response =
        serde_json::from_slice(&read_frame(&mut client).await.unwrap()).unwrap();
    task.await.unwrap();
    assert_eq!(response.protocol, AUTH_PROTOCOL);
    assert_eq!(response.id, "forwarding-test");
    response.result
}
fn policy(enabled: bool) -> Value {
    json!({"enabled":enabled,"targets":[{"id":TARGET,"destination":{"kind":"webhook",
        "url":"https://example.invalid/path-secret?secret=synthetic-query-secret",
        "bearer":"synthetic-bearer-secret"}}],
        "rules":[{"id":RULE,"targets":[TARGET],"sender_equals":null,"body_contains":null,
            "line":null,"share":"notice"}]})
}

#[tokio::test]
async fn actual_owner_default_off_reads_do_not_create_files_or_claim_process_stopped() {
    let f = Fixture::new();
    let path = f.state.path().join("forwarding");
    assert!(!path.exists());
    for command in [
        AuthCommand::ForwardingStatus {
            token: f.token.clone(),
        },
        AuthCommand::ForwardingRules {
            token: f.token.clone(),
        },
        AuthCommand::ForwardingReceipts {
            token: f.token.clone(),
            after: "0".into(),
            limit: 32,
        },
    ] {
        let result = f.request(command).await;
        assert!(result.get("error").is_none());
    }
    let status = f
        .request(AuthCommand::ForwardingStatus {
            token: f.token.clone(),
        })
        .await;
    assert_eq!(status["enabled"], false);
    assert_eq!(status["configured"], false);
    assert_eq!(status["generation"], 0);
    assert_eq!(status["delivery_api"], false);
    assert_eq!(status["process_state_verified"], false);
    assert!(!path.exists());
    assert_eq!(
        rpc(f.auth.clone(), None, f.configure(0, REQUEST, true)).await["error"],
        "denied"
    );
    assert!(!path.exists());
}

#[tokio::test]
async fn actual_core_ipc_persists_rules_redacts_secrets_and_retries_without_skipping_new_sms() {
    let f = Fixture::new();
    f.incoming("before-enable");
    let baseline = f.store.handle().inbox_event_head().unwrap();
    assert_eq!(
        f.request(f.configure(0, REQUEST, true)).await["generation"],
        1
    );
    f.incoming("after-enable");
    // A real new inbox event arrived between an applied configure and a lost
    // reply retry. Replaying that intent must neither fail nor skip the event.
    assert_eq!(
        f.request(f.configure(0, REQUEST, true)).await["generation"],
        1
    );
    {
        let mut state = f.control.0.lock().unwrap();
        let q = state.queue.as_mut().unwrap();
        let cursor = q.event_cursor().unwrap();
        assert_eq!(cursor.db_epoch, baseline.db_epoch);
        assert_eq!(cursor.seq, baseline.seq);
        assert_eq!(
            q.capture_current(&f.auth.lock().unwrap(), &f.store.handle(), 1)
                .unwrap(),
            1
        );
    }
    let rules = f
        .request(AuthCommand::ForwardingRules {
            token: f.token.clone(),
        })
        .await;
    let encoded = rules.to_string();
    for secret in [
        "path-secret",
        "synthetic-query-secret",
        "synthetic-bearer-secret",
        "synthetic private body",
    ] {
        assert!(!encoded.contains(secret));
    }
    assert_eq!(rules["targets"][0]["kind"], "webhook");
    assert_eq!(rules["targets"][0]["credentials_redacted"], true);
    let page = f
        .request(AuthCommand::ForwardingReceipts {
            token: f.token.clone(),
            after: "0".into(),
            limit: 32,
        })
        .await;
    assert_eq!(page["receipts"].as_array().unwrap().len(), 1);
    assert_eq!(page["receipts"][0]["state"], "queued");
    assert!(!page.to_string().contains("synthetic"));
    assert_eq!(
        f.request(f.configure(0, REQUEST, false)).await["error"],
        "denied"
    );
    assert_eq!(
        f.request(f.configure(0, CHANGE, true)).await["error"],
        "denied"
    );
    assert_eq!(
        f.request(AuthCommand::ForwardingStatus {
            token: f.token.clone()
        })
        .await["generation"],
        1
    );
}

#[tokio::test]
async fn disabling_via_actual_ipc_revokes_inflight_and_restart_preserves_unknown_and_credentials() {
    let f = Fixture::new();
    f.request(f.configure(0, REQUEST, true)).await;
    f.incoming("synthetic one");
    let claim = {
        let a = f.auth.lock().unwrap();
        let mut state = f.control.0.lock().unwrap();
        let q = state.queue.as_mut().unwrap();
        q.capture_current(&a, &f.store.handle(), 1).unwrap();
        q.claim_current(&a, &f.store.handle(), 1).unwrap().unwrap()
    };
    f.incoming("synthetic two");
    {
        let a = f.auth.lock().unwrap();
        let mut state = f.control.0.lock().unwrap();
        state
            .queue
            .as_mut()
            .unwrap()
            .capture_current(&a, &f.store.handle(), 1)
            .unwrap();
    }
    let disabled = f.request(f.configure(1, CHANGE, false)).await;
    assert_eq!(disabled["generation"], 2);
    assert_eq!(disabled["enabled"], false);
    assert_eq!(disabled["queued"], 0);
    assert_eq!(disabled["in_flight"], 0);
    assert_eq!(disabled["unknown"], 1);
    assert_eq!(disabled["process_state_verified"], false);
    assert!(f
        .control
        .0
        .lock()
        .unwrap()
        .queue
        .as_mut()
        .unwrap()
        .finish(
            &Binding::from_owner(&mut f.auth.lock().unwrap(), &f.token).unwrap(),
            &claim,
            rustsimhub_forwarder::State::Confirmed
        )
        .is_err());
    let pruned = f
        .request(AuthCommand::ForwardingPrune {
            token: f.token.clone(),
            generation: 2,
            through: i64::MAX.to_string(),
        })
        .await;
    assert_eq!(pruned["pruned"], 1);
    f.control.0.lock().unwrap().queue = None;
    let next = Controller::new(f.state.path(), f.store.handle());
    let status = rpc(
        f.auth.clone(),
        Some(next.clone()),
        AuthCommand::ForwardingStatus {
            token: f.token.clone(),
        },
    )
    .await;
    assert_eq!(status["generation"], 2);
    assert_eq!(status["unknown"], 1);
    let rules = rpc(
        f.auth.clone(),
        Some(next),
        AuthCommand::ForwardingRules {
            token: f.token.clone(),
        },
    )
    .await;
    assert_eq!(rules["targets"].as_array().unwrap().len(), 1);
    assert!(!rules.to_string().contains("secret"));
}

#[tokio::test]
async fn even_privileged_signed_paired_client_cannot_read_or_configure_forwarding() {
    let f = Fixture::new();
    let token = {
        let mut a = f.auth.lock().unwrap();
        let key = SigningKey::from_bytes(&[127; 32]);
        let scopes = [Scope::SmsRead, Scope::SystemManage, Scope::ClientsManage];
        a.open_pairing(&f.token).unwrap();
        let c = a
            .pairing_challenge(key.verifying_key().to_bytes(), Kind::Client, &scopes)
            .unwrap();
        let id = a
            .pairing_finish(&c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap();
        a.approve(
            &f.token,
            &id,
            key.verifying_key().to_bytes(),
            Kind::Client,
            &scopes,
        )
        .unwrap();
        login(&mut a, &key)
    };
    for token in [token, "0".repeat(64)] {
        for command in [
            AuthCommand::SetForwardingEnabled {
                token: token.clone(),
                expected_generation: 0,
                request_id: REQUEST.into(),
                enabled: true,
            },
            AuthCommand::ForwardingStatus {
                token: token.clone(),
            },
            AuthCommand::ForwardingRules {
                token: token.clone(),
            },
            AuthCommand::ForwardingConfigure {
                token: token.clone(),
                expected_generation: 0,
                request_id: REQUEST.into(),
                policy: policy(true),
            },
            AuthCommand::ForwardingReceipts {
                token: token.clone(),
                after: "0".into(),
                limit: 32,
            },
            AuthCommand::ForwardingPrune {
                token,
                generation: 1,
                through: "1".into(),
            },
        ] {
            assert_eq!(f.request(command).await["error"], "denied");
        }
    }
    assert!(!f.state.path().join("forwarding").exists());
}

#[tokio::test]
async fn stale_or_invalid_config_and_unsafe_path_cannot_create_or_adopt_outbox() {
    let f = Fixture::new();
    assert_eq!(
        f.request(f.configure(7, REQUEST, true)).await["error"],
        "denied"
    );
    let mut bad = policy(true);
    bad["targets"][0]["destination"]["url"] = "http://example.invalid/unsafe".into();
    assert_eq!(
        f.request(AuthCommand::ForwardingConfigure {
            token: f.token.clone(),
            expected_generation: 0,
            request_id: REQUEST.into(),
            policy: bad
        })
        .await["error"],
        "denied"
    );
    assert!(!f.state.path().join("forwarding").exists());
    let foreign = dir();
    symlink(foreign.path(), f.state.path().join("forwarding")).unwrap();
    assert_eq!(
        f.request(f.configure(0, REQUEST, true)).await["error"],
        "denied"
    );
    assert_eq!(fs::read_dir(foreign.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn actual_owner_generation_change_never_adopts_previous_outbox_even_after_reopen() {
    let f = Fixture::new();
    f.request(f.configure(0, REQUEST, true)).await;
    let db = rusqlite::Connection::open(f.state.path().join("store.sqlite3")).unwrap();
    db.execute(
        "UPDATE principals SET generation=generation+1 WHERE kind='owner'",
        [],
    )
    .unwrap();
    // New real login at the new generation (old bearer is no longer valid).
    let token = login(
        &mut f.auth.lock().unwrap(),
        &SigningKey::from_bytes(&[126; 32]),
    );
    assert_eq!(
        f.request(AuthCommand::ForwardingStatus {
            token: token.clone()
        })
        .await["error"],
        "denied"
    );
    f.control.0.lock().unwrap().queue = None;
    let control = Controller::new(f.state.path(), f.store.handle());
    assert_eq!(
        rpc(
            f.auth.clone(),
            Some(control),
            AuthCommand::ForwardingConfigure {
                token,
                expected_generation: 0,
                request_id: CHANGE.into(),
                policy: policy(false)
            }
        )
        .await["error"],
        "denied"
    );
    let q = rusqlite::Connection::open(f.state.path().join("forwarding/outbox.sqlite3")).unwrap();
    assert_eq!(
        q.query_row("SELECT generation FROM meta", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn sqlite_abort_during_actual_configure_is_not_reported_as_applied_and_retry_is_safe() {
    let f = Fixture::new();
    f.request(f.configure(0, REQUEST, true)).await;
    let q = rusqlite::Connection::open(f.state.path().join("forwarding/outbox.sqlite3")).unwrap();
    q.execute_batch("CREATE TRIGGER injected_failure BEFORE UPDATE ON meta BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;").unwrap();
    assert_eq!(
        f.request(f.configure(1, CHANGE, false)).await["error"],
        "denied"
    );
    assert_eq!(
        f.request(AuthCommand::ForwardingStatus {
            token: f.token.clone()
        })
        .await["generation"],
        1
    );
    q.execute_batch("DROP TRIGGER injected_failure;").unwrap();
    assert_eq!(
        f.request(f.configure(1, CHANGE, false)).await["generation"],
        2
    );
}

#[tokio::test]
async fn master_switch_preserves_secret_targets_and_only_captures_messages_after_reenable() {
    let f = Fixture::new();
    let switch = |expected, request: &str, enabled| AuthCommand::SetForwardingEnabled {
        token: f.token.clone(),
        expected_generation: expected,
        request_id: request.into(),
        enabled,
    };
    assert_eq!(f.request(switch(0, REQUEST, true)).await["error"], "denied");
    assert!(!f.state.path().join("forwarding").exists());
    f.request(f.configure(0, REQUEST, true)).await;
    assert_eq!(f.request(switch(1, CHANGE, false)).await["generation"], 2);
    f.incoming("while disabled");
    let reenable = "55555555-5555-4555-8555-555555555555";
    assert_eq!(f.request(switch(2, reenable, true)).await["generation"], 3);
    let baseline = f.store.handle().inbox_event_head().unwrap();
    f.incoming("after reenable");
    assert_eq!(f.request(switch(2, reenable, true)).await["generation"], 3);
    let a = f.auth.lock().unwrap();
    let mut m = f.control.0.lock().unwrap();
    let q = m.queue.as_mut().unwrap();
    assert_eq!(q.event_cursor().unwrap().seq, baseline.seq);
    assert_eq!(q.capture_current(&a, &f.store.handle(), 3).unwrap(), 1);
    let claim = q.claim_current(&a, &f.store.handle(), 3).unwrap().unwrap();
    assert_eq!(
        claim.request.bearer.unwrap().expose(),
        "synthetic-bearer-secret"
    );
}

#[test]
fn management_wire_rejects_overflow_noncanonical_sequence_unknown_fields_and_missing_consent() {
    let good = json!({"protocol":2,"id":"id","command":{"op":"forwarding_configure",
        "token":"0".repeat(64),"expected_generation":0,"request_id":REQUEST,"policy":policy(true)}});
    assert!(rustsimhub_wire::parse_auth_request(&serde_json::to_vec(&good).unwrap()).is_ok());
    for (key, value) in [
        ("request_id", json!("A".repeat(36))),
        ("expected_generation", json!(u64::MAX)),
        ("policy", json!([])),
        ("owner", json!(true)),
    ] {
        let mut bad = good.clone();
        bad["command"][key] = value;
        assert!(rustsimhub_wire::parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    for after in ["-1", "+1", "01", "9223372036854775808"] {
        let request = json!({"protocol":2,"id":"id","command":{"op":"forwarding_receipts",
            "token":"0".repeat(64),"after":after,"limit":32}});
        assert!(
            rustsimhub_wire::parse_auth_request(&serde_json::to_vec(&request).unwrap()).is_err()
        );
    }
}
