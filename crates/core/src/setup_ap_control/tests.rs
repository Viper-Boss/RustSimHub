// SPDX-License-Identifier: AGPL-3.0-only
//! Real Auth and production filesystem/coordination, with an explicit Root-status emulator.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_auth::Identity;
use rustsimhub_storage::Store;
use std::{os::unix::fs::PermissionsExt, sync::atomic::AtomicUsize};
fn directory(mode: u32) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(mode)).unwrap();
    d
}
struct Fixture {
    runtime: Runtime,
    auth: Arc<Mutex<Auth>>,
    token: String,
    dirs: Vec<tempfile::TempDir>,
    _store: Store,
}
struct NoCalls {
    checks: AtomicUsize,
    fail: bool,
}
impl Idle for NoCalls {
    async fn check(&self, _token: &str) -> Result<()> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            Err(Error::Busy)
        } else {
            Ok(())
        }
    }
}
fn idle() -> NoCalls {
    NoCalls {
        checks: AtomicUsize::new(0),
        fail: false,
    }
}
fn fixture() -> Fixture {
    let data = directory(0o700);
    let identity = directory(0o700);
    let input = directory(0o2750);
    let output = directory(0o2750);
    let journal = directory(0o700);
    let store = Store::open(data.path()).unwrap();
    let mut auth = Auth::new(Identity::open(identity.path()).unwrap(), store.handle()).unwrap();
    let key = SigningKey::from_bytes(&[111; 32]);
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
        .unwrap()
        .expose()
        .to_owned();
    let uid = rustix::process::geteuid().as_raw();
    let gid = fs::metadata(input.path()).unwrap().gid();
    let runtime = Runtime::with_coordination(
        Directory::open(input.path(), uid, gid, 0o2750).unwrap(),
        Directory::open(output.path(), uid, gid, 0o2750).unwrap(),
        journal.path().into(),
        Arc::new(crate::coordination::Coordination::default()),
    )
    .unwrap();
    let a = Availability {
        format: 1,
        boot_id: boot().unwrap(),
        realm: auth.durable_realm().unwrap(),
        device_key_sha256: hash(&auth.public_key()),
        material_sha256: "d".repeat(64),
        sample_uptime_ms: now().unwrap(),
        tls_origin_ready: true,
        gate_ready: true,
    };
    runtime.output.publish(Slot::Availability, &a).unwrap();
    Fixture {
        runtime,
        auth: Arc::new(Mutex::new(auth)),
        token,
        dirs: vec![data, identity, input, output, journal],
        _store: store,
    }
}
fn hint(f: &Fixture) -> Hint {
    serde_json::from_slice(&f.runtime.input.read(Slot::Hint).unwrap().unwrap()).unwrap()
}
fn root_status(f: &Fixture, state: State, mutate: impl FnOnce(&mut Status)) {
    let h = hint(f);
    let mut s = Status {
        format: 1,
        binding: h.binding,
        state,
        sequence: h.sequence,
        sample_uptime_ms: now().unwrap(),
        rollback_verified: state == State::Closed,
        guard_released: state == State::Closed,
    };
    mutate(&mut s);
    f.runtime.output.publish(Slot::Status, &s).unwrap();
}
#[tokio::test]
async fn real_owner_start_is_durable_single_use_and_waits_for_root_consumption_before_heartbeat() {
    let f = fixture();
    let calls = idle();
    let first = f
        .runtime
        .start(f.auth.clone(), f.token.clone(), "one".into(), &calls)
        .await
        .unwrap();
    let initial = hint(&f);
    assert!(initial.action == Action::Start);
    assert_eq!(initial.sequence, 1);
    assert!(f.dirs[4].path().join("pending.json").is_file());
    let json = serde_json::to_string(&initial).unwrap();
    assert!(!json.contains(&f.token));
    assert!(!first.to_string().contains("nonce"));
    assert!(f
        .runtime
        .coordination
        .telephony(rustsimhub_domain::CallAction::Dial)
        .is_err());
    assert!(f
        .runtime
        .coordination
        .telephony(rustsimhub_domain::CallAction::Hangup)
        .unwrap()
        .is_none());
    let same = f
        .runtime
        .start(f.auth.clone(), f.token.clone(), "one".into(), &calls)
        .await
        .unwrap();
    assert_eq!(same["operation_id"], first["operation_id"]);
    assert_eq!(calls.checks.load(Ordering::SeqCst), 1);
    f.runtime.tick(f.auth.clone(), &calls).await.unwrap();
    assert!(hint(&f) == initial);
    root_status(&f, State::Starting, |_| {});
    f.runtime.tick(f.auth.clone(), &calls).await.unwrap();
    assert!(hint(&f).action == Action::KeepAlive);
    assert_eq!(hint(&f).sequence, 2);
    assert_eq!(
        hint(&f).binding.deadline_uptime_ms,
        initial.binding.deadline_uptime_ms
    );
    assert!(f
        .runtime
        .start(f.auth.clone(), f.token.clone(), "other".into(), &calls)
        .await
        .is_err());
    root_status(&f, State::Closed, |s| s.binding.nonce = "f".repeat(64));
    f.runtime.tick(f.auth.clone(), &calls).await.unwrap();
    assert!(f.runtime.coordination.network().is_err());
    root_status(&f, State::Closed, |_| {});
    f.runtime.tick(f.auth.clone(), &calls).await.unwrap();
    assert!(f.runtime.coordination.network().is_ok());
    assert!(!f.dirs[4].path().join("pending.json").exists());
    assert_eq!(
        f.runtime
            .status(f.auth.clone(), f.token.clone())
            .await
            .unwrap()["state"],
        "closed"
    );
}
#[tokio::test]
async fn delegated_or_missing_permissions_and_root_readiness_never_publish_a_start() {
    let f = fixture();
    let client = SigningKey::from_bytes(&[112; 32]);
    let token = {
        let mut a = f.auth.lock().unwrap();
        a.open_pairing(&f.token).unwrap();
        let scopes = [Scope::NetworkManage];
        let c = a
            .pairing_challenge(client.verifying_key().to_bytes(), Kind::Client, &scopes)
            .unwrap();
        let id = a
            .pairing_finish(&c.id, &client.sign(&c.transcript).to_bytes())
            .unwrap();
        a.approve(
            &f.token,
            &id,
            client.verifying_key().to_bytes(),
            Kind::Client,
            &scopes,
        )
        .unwrap();
        let c = a
            .login_challenge(client.verifying_key().to_bytes())
            .unwrap();
        a.login_finish(&c.id, &client.sign(&c.transcript).to_bytes())
            .unwrap()
            .expose()
            .to_owned()
    };
    assert!(f
        .runtime
        .start(f.auth.clone(), token, "no".into(), &idle())
        .await
        .is_err());
    assert!(f.runtime.input.read(Slot::Hint).unwrap().is_none());
    let mut a: Availability =
        serde_json::from_slice(&f.runtime.output.read(Slot::Availability).unwrap().unwrap())
            .unwrap();
    a.tls_origin_ready = false;
    f.runtime.output.publish(Slot::Availability, &a).unwrap();
    assert!(f
        .runtime
        .start(f.auth.clone(), f.token.clone(), "no-tls".into(), &idle())
        .await
        .is_err());
    assert!(!f.dirs[4].path().join("pending.json").exists());
    a.tls_origin_ready = true;
    f.runtime.output.publish(Slot::Availability, &a).unwrap();
    let busy = NoCalls {
        checks: AtomicUsize::new(0),
        fail: true,
    };
    assert!(f
        .runtime
        .start(f.auth.clone(), f.token.clone(), "busy".into(), &busy)
        .await
        .is_err());
    assert!(f.runtime.input.read(Slot::Hint).unwrap().is_none());
}
#[tokio::test]
async fn auth_instance_loss_sends_cancel_and_unknown_keeps_network_exclusion() {
    let f = fixture();
    f.runtime
        .start(f.auth.clone(), f.token.clone(), "loss".into(), &idle())
        .await
        .unwrap();
    root_status(&f, State::Ready, |_| {});
    *f.auth.lock().unwrap() =
        Auth::new(Identity::load(f.dirs[1].path()).unwrap(), f._store.handle()).unwrap();
    f.runtime.tick(f.auth.clone(), &idle()).await.unwrap();
    assert!(hint(&f).action == Action::Cancel);
    assert!(f.runtime.coordination.network().is_err());
    root_status(&f, State::Unknown, |_| {});
    f.runtime.tick(f.auth.clone(), &idle()).await.unwrap();
    assert!(f.runtime.coordination.network().is_err());
    root_status(&f, State::Closed, |_| {});
    f.runtime.tick(f.auth.clone(), &idle()).await.unwrap();
    assert!(f.runtime.coordination.network().is_ok());
}
#[tokio::test]
async fn a_late_ready_receipt_cannot_resume_a_cancelled_lease() {
    let f = fixture();
    let first = f
        .runtime
        .start(f.auth.clone(), f.token.clone(), "cancel".into(), &idle())
        .await
        .unwrap();
    root_status(&f, State::Ready, |_| {});
    f.runtime.tick(f.auth.clone(), &idle()).await.unwrap();
    f.runtime
        .cancel(
            f.auth.clone(),
            f.token.clone(),
            first["operation_id"].as_str().unwrap().into(),
        )
        .await
        .unwrap();
    let cancelled = hint(&f);
    assert!(cancelled.action == Action::Cancel);
    root_status(&f, State::Ready, |_| {});
    f.runtime.tick(f.auth.clone(), &idle()).await.unwrap();
    assert!(hint(&f) == cancelled);
    assert_eq!(
        f.runtime
            .status(f.auth.clone(), f.token.clone())
            .await
            .unwrap()["state"],
        "closing"
    );
    assert!(f.runtime.coordination.network().is_err());
    root_status(&f, State::Closed, |_| {});
    f.runtime.tick(f.auth.clone(), &idle()).await.unwrap();
    assert!(f.runtime.coordination.network().is_ok());
}
#[tokio::test]
async fn recovered_core_cancels_old_start_and_keeps_lock_until_exact_root_close() {
    let mut f = fixture();
    f.runtime
        .start(f.auth.clone(), f.token.clone(), "recovery".into(), &idle())
        .await
        .unwrap();
    let old = hint(&f);
    let input = f.runtime.input.clone();
    let output = f.runtime.output.clone();
    let journal = f.runtime.journal.clone();
    let coordination = f.runtime.coordination.clone();
    drop(f.runtime);
    f.runtime = Runtime::with_coordination(input, output, journal, coordination).unwrap();
    let current = hint(&f);
    assert!(current.action == Action::Cancel);
    assert!(current.binding == old.binding);
    assert!(f.runtime.coordination.network().is_err());
    let reused = f
        .runtime
        .start(f.auth.clone(), f.token.clone(), "recovery".into(), &idle())
        .await
        .unwrap();
    assert_eq!(reused["state"], "unknown");
    assert!(hint(&f).action == Action::Cancel);
    root_status(&f, State::Closed, |_| {});
    f.runtime.tick(f.auth.clone(), &idle()).await.unwrap();
    assert!(f.runtime.coordination.network().is_ok());
}
