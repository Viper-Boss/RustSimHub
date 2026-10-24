// SPDX-License-Identifier: AGPL-3.0-only
//! Real device LocalSetup/Auth, with explicit Root and idle fixtures; no radio is touched.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_auth::{Identity, LocalSetup};
use rustsimhub_storage::Store;
use std::os::unix::fs::PermissionsExt;
fn directory(mode: u32) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(mode)).unwrap();
    d
}
struct NoCalls {
    fail: bool,
}
fn idle() -> NoCalls {
    NoCalls { fail: false }
}
impl Idle for NoCalls {
    async fn check(&self, _token: &str) -> Result<()> {
        if self.fail {
            Err(Error::Busy)
        } else {
            Ok(())
        }
    }
    async fn device(&self, _setup: Arc<Mutex<LocalSetup>>) -> Result<()> {
        self.check("").await
    }
}
struct Fixture {
    runtime: Runtime,
    auth: Arc<Mutex<Auth>>,
    gate: Arc<Mutex<LocalSetup>>,
    dirs: Vec<tempfile::TempDir>,
    store: Store,
}
fn fixture() -> Fixture {
    let data = directory(0o700);
    let identity = directory(0o700);
    let setup = directory(0o700);
    let input = directory(0o2750);
    let output = directory(0o2750);
    let journal = directory(0o700);
    let store = Store::open(data.path()).unwrap();
    let auth = Auth::new(Identity::open(identity.path()).unwrap(), store.handle()).unwrap();
    LocalSetup::prepare_device_code(setup.path(), &auth).unwrap();
    let gate = LocalSetup::for_device(setup.path(), &auth)
        .unwrap()
        .unwrap();
    let uid = rustix::process::geteuid().as_raw();
    let gid = fs::metadata(input.path()).unwrap().gid();
    let runtime = Runtime::with_coordination(
        Directory::open(input.path(), uid, gid, 0o2750).unwrap(),
        Directory::open(output.path(), uid, gid, 0o2750).unwrap(),
        journal.path().into(),
        Arc::new(crate::coordination::Coordination::default()),
    )
    .unwrap();
    runtime
        .output
        .publish(
            Slot::Availability,
            &Availability {
                format: 1,
                boot_id: boot().unwrap(),
                realm: auth.durable_realm().unwrap(),
                device_key_sha256: hash(&auth.public_key()),
                material_sha256: "d".repeat(64),
                sample_uptime_ms: now().unwrap(),
                tls_origin_ready: true,
                gate_ready: true,
            },
        )
        .unwrap();
    Fixture {
        runtime,
        auth: Arc::new(Mutex::new(auth)),
        gate: Arc::new(Mutex::new(gate)),
        dirs: vec![data, identity, setup, input, output, journal],
        store,
    }
}
fn hint(f: &Fixture) -> Hint {
    serde_json::from_slice(&f.runtime.input.read(Slot::Hint).unwrap().unwrap()).unwrap()
}
fn status(f: &Fixture, state: State, mutate: impl FnOnce(&mut Status)) {
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
async fn tick(f: &Fixture) -> Result<()> {
    f.runtime
        .bootstrap
        .tick(f.auth.clone(), Some(f.gate.clone()), &idle())
        .await
}
fn claim(f: &Fixture) -> String {
    let code = rustsimhub_auth::read_private(&f.dirs[2].path().join("setup.code"), 32)
        .unwrap()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let key = SigningKey::from_bytes(&[117; 32]);
    let mut a = f.auth.lock().unwrap();
    let mut g = f.gate.lock().unwrap();
    let c = g
        .begin(&mut a, &code, key.verifying_key().to_bytes())
        .unwrap();
    g.finish(&mut a, &code, &c.id, &key.sign(&c.transcript).to_bytes())
        .unwrap();
    let c = a.login_challenge(key.verifying_key().to_bytes()).unwrap();
    a.login_finish(&c.id, &key.sign(&c.transcript).to_bytes())
        .unwrap()
        .expose()
        .to_owned()
}
#[tokio::test]
async fn real_device_window_starts_once_and_retains_original_deadline_after_signed_claim() {
    let f = fixture();
    tick(&f).await.unwrap();
    let start = hint(&f);
    assert!(start.action == Action::Start);
    assert!(start.binding.deadline_uptime_ms - start.binding.created_uptime_ms <= 300000);
    tick(&f).await.unwrap();
    assert!(hint(&f) == start);
    assert!(f.runtime.coordination.network().is_err());
    status(&f, State::Ready, |_| {});
    tick(&f).await.unwrap();
    assert!(hint(&f).action == Action::KeepAlive);
    let token = claim(&f);
    tick(&f).await.unwrap();
    let after = hint(&f);
    assert!(after.action == Action::KeepAlive);
    assert_eq!(
        after.binding.deadline_uptime_ms,
        start.binding.deadline_uptime_ms
    );
    assert_eq!(
        f.runtime
            .status(f.auth.clone(), token.clone())
            .await
            .unwrap()["first_setup"],
        true
    );
    assert!(f
        .runtime
        .start(f.auth.clone(), token, "competing-owner".into(), &idle())
        .await
        .is_err());
    status(&f, State::Closed, |_| {});
    tick(&f).await.unwrap();
    assert!(f.runtime.coordination.network().is_ok());
    tick(&f).await.unwrap();
    assert!(hint(&f).action == Action::KeepAlive); // No new Start after Closed.
}
#[tokio::test]
async fn explicit_load_missing_tls_or_busy_voice_never_produces_start() {
    let f = fixture();
    let explicit = Arc::new(Mutex::new(
        LocalSetup::load(&f.dirs[2].path().join("setup.code")).unwrap(),
    ));
    assert!(f
        .runtime
        .bootstrap
        .tick(f.auth.clone(), Some(explicit), &idle())
        .await
        .is_err());
    assert!(f.runtime.input.read(Slot::Hint).unwrap().is_none());
    let raw = f.runtime.output.read(Slot::Availability).unwrap().unwrap();
    let mut a: Availability = serde_json::from_slice(&raw).unwrap();
    a.tls_origin_ready = false;
    f.runtime.output.publish(Slot::Availability, &a).unwrap();
    assert!(tick(&f).await.is_err());
    assert!(f.runtime.input.read(Slot::Hint).unwrap().is_none());
    a.tls_origin_ready = true;
    f.runtime.output.publish(Slot::Availability, &a).unwrap();
    let busy = NoCalls { fail: true };
    assert!(f
        .runtime
        .bootstrap
        .tick(f.auth.clone(), Some(f.gate.clone()), &busy)
        .await
        .is_err());
    assert!(f.runtime.input.read(Slot::Hint).unwrap().is_none());
}
#[tokio::test]
async fn exhausted_setup_budget_cancels_and_unknown_never_frees_the_network_guard() {
    let f = fixture();
    tick(&f).await.unwrap();
    status(&f, State::Ready, |_| {});
    tick(&f).await.unwrap();
    {
        let mut a = f.auth.lock().unwrap();
        let mut g = f.gate.lock().unwrap();
        for _ in 0..16 {
            assert!(g.begin(&mut a, "bad", [8; 32]).is_err());
        }
    }
    tick(&f).await.unwrap();
    assert!(hint(&f).action == Action::Cancel);
    let cancelled = hint(&f);
    status(&f, State::Ready, |_| {});
    tick(&f).await.unwrap();
    assert!(hint(&f) == cancelled);
    status(&f, State::Unknown, |_| {});
    tick(&f).await.unwrap();
    assert!(f.runtime.coordination.network().is_err());
    status(&f, State::Closed, |s| s.binding.nonce = "f".repeat(64));
    tick(&f).await.unwrap();
    assert!(f.runtime.coordination.network().is_err());
    status(&f, State::Closed, |_| {});
    tick(&f).await.unwrap();
    assert!(f.runtime.coordination.network().is_ok());
}
#[tokio::test]
async fn claimed_before_start_or_stopped_runtime_never_auto_starts_an_ap() {
    let f = fixture();
    claim(&f);
    tick(&f).await.unwrap();
    assert!(f.runtime.input.read(Slot::Hint).unwrap().is_none());
    let f = fixture();
    f.runtime.bootstrap.stop();
    tick(&f).await.unwrap();
    assert!(f.runtime.input.read(Slot::Hint).unwrap().is_none());
}
#[tokio::test]
async fn original_auth_instance_loss_closes_without_reauthorizing_from_another_boot_context() {
    let f = fixture();
    tick(&f).await.unwrap();
    *f.auth.lock().unwrap() =
        Auth::new(Identity::load(f.dirs[1].path()).unwrap(), f.store.handle()).unwrap();
    tick(&f).await.unwrap();
    assert!(hint(&f).action == Action::Cancel);
    assert!(f.runtime.coordination.network().is_err());
}
#[tokio::test]
async fn restart_only_cancels_pending_bootstrap_and_waits_for_exact_root_close() {
    let mut f = fixture();
    tick(&f).await.unwrap();
    let start = hint(&f);
    let input = f.runtime.input.clone();
    let output = f.runtime.output.clone();
    let journal = f.runtime.journal.clone();
    let coordination = f.runtime.coordination.clone();
    drop(f.runtime);
    f.runtime = Runtime::with_coordination(input, output, journal, coordination).unwrap();
    let cancelled = hint(&f);
    assert!(cancelled.action == Action::Cancel && cancelled.binding == start.binding);
    tick(&f).await.unwrap();
    assert!(hint(&f) == cancelled);
    assert!(f.runtime.coordination.network().is_err());
    status(&f, State::Closed, |_| {});
    tick(&f).await.unwrap();
    assert!(f.runtime.coordination.network().is_ok());
}
