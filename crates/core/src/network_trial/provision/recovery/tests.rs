// SPDX-License-Identifier: AGPL-3.0-only
//! Actual original Core actor, locks, Auth/SQLite and decoder. Only the outside
//! A verifier and NM backend are simulated. No hardware/crypto/UID acceptance.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_auth::{
    recovery_capability::{
        RecoveryConnection, RecoveryProofContext, RecoveryRequestVerifierV1,
        VerifiedRecoveryRequest,
    },
    Token,
};
use rustsimhub_domain::CallAction;
use rustsimhub_storage::{RecoveryGrant, Store};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Weak,
    },
};
use tempfile::TempDir;
use tokio::sync::Notify;
mod status_tests;
mod control_order;
mod t5_tests;

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const BODY: &[u8] = br#"{"ssid":"SYNTHETIC_NATIVE","passphrase":"SYNTHETIC_NATIVE_PASSWORD"}"#;
type Plan = Arc<Mutex<(RecoveryAction, Vec<u8>)>>;
struct Adapter(Plan);
impl RecoveryRequestVerifierV1 for Adapter {
    fn verify_request(
        &self,
        c: &RecoveryProofContext,
        e: &[u8],
    ) -> Result<VerifiedRecoveryRequest> {
        let (action, body) = self.0.lock().unwrap().clone();
        VerifiedRecoveryRequest::from_verified_adapter(
            c.binding().clone(),
            vec![8; 16],
            e.try_into().map_err(|_| Error::InvalidInput)?,
            action,
            Zeroizing::new(body),
            Duration::from_secs(120),
        )
    }
}
struct Fixture {
    auth: Arc<Mutex<Auth>>,
    _store: Store,
    d: TempDir,
    _i: TempDir,
    owner: Token,
    grant: RecoveryGrant,
    plan: Plan,
}
fn dir() -> TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    d
}
fn id(n: u8) -> String {
    uuid::Uuid::from_bytes([n; 16]).to_string()
}
fn fixture(budget: u32) -> Fixture {
    let (d, i, b) = (dir(), dir(), dir());
    let s = Store::open(d.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let owner = SigningKey::from_bytes(&[81; 32]);
    let c = a
        .bootstrap_challenge(owner.verifying_key().to_bytes())
        .unwrap();
    a.bootstrap_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    drop(a);
    drop(s);
    let s = Store::migrate_recovery_grants(d.path(), b.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let c = a.login_challenge(owner.verifying_key().to_bytes()).unwrap();
    let token = a
        .login_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    let grant = a
        .grant_recovery_key(
            token.expose(),
            SigningKey::from_bytes(&[82; 32]).verifying_key().to_bytes(),
            budget,
            0,
            id(90),
        )
        .unwrap();
    drop(a);
    drop(s);
    let b = dir();
    Store::migrate_recovery_execution(d.path(), b.path()).unwrap();
    let s = Store::open_recovery_execution(d.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let c = a.login_challenge(owner.verifying_key().to_bytes()).unwrap();
    let token = a
        .login_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    let plan = Arc::new(Mutex::new((RecoveryAction::BeginWifi, BODY.to_vec())));
    a.set_recovery_verifier(Some(Arc::new(Adapter(plan.clone()))))
        .unwrap();
    Fixture {
        auth: Arc::new(Mutex::new(a)),
        _store: s,
        d,
        _i: i,
        owner: token,
        grant,
        plan,
    }
}
fn connect(f: &Fixture) -> Arc<RecoveryConnection> {
    Arc::new(
        f.auth
            .lock()
            .unwrap()
            .open_recovery_connection_from_trusted_transport([11; 32])
            .unwrap(),
    )
}
fn request(
    f: &Fixture,
    c: &RecoveryConnection,
    n: u8,
    action: RecoveryAction,
    body: &[u8],
) -> AuthorizedRecoveryRequest {
    *f.plan.lock().unwrap() = (action, body.to_vec());
    let p = f
        .auth
        .lock()
        .unwrap()
        .prepare_recovery_verification(c, f.grant.clone(), &[n; 16])
        .unwrap();
    let result = p.verify();
    f.auth
        .lock()
        .unwrap()
        .finish_recovery_verification(c, result)
        .unwrap()
}
fn write(
    f: &Fixture,
    c: &RecoveryConnection,
    n: u8,
    action: RecoveryAction,
    trial: Option<&str>,
) -> Box<CoreRecoveryWrite> {
    let body = trial
        .map(|id| format!("{{\"trial_id\":\"{id}\"}}").into_bytes())
        .unwrap_or_else(|| BODY.to_vec());
    let req = request(f, c, n, action, &body);
    let a = f.auth.lock().unwrap();
    match recovery_body::decode_v1(&a, req).unwrap() {
        DecodedRecoveryRequest::Write(w) => w,
        _ => panic!("write expected"),
    }
}
fn row(f: &Fixture, n: u8) -> Option<(String, Option<i64>, Option<i64>)> {
    use rusqlite::OptionalExtension;
    let c = rusqlite::Connection::open_with_flags(
        f.d.path().join("store.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    c.query_row("SELECT state,settings_saved,rollback_verified FROM recovery_execution_receipts WHERE request_id=?",[id(n)],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().unwrap()
}
struct Revoke {
    auth: Arc<Mutex<Auth>>,
    token: Zeroizing<String>,
    grant: RecoveryGrant,
}
struct Probe {
    effects: Mutex<Vec<&'static str>>,
    ready: AtomicBool,
    fail_commit: bool,
    fail_rollback: bool,
    hold_start: bool,
    hold_commit: bool,
    commit_entered: Notify,
    release_commit: Notify,
    invalidate: Option<Arc<RecoveryConnection>>,
    revoke: Option<Revoke>,
    trial: Mutex<Weak<CoreRecoveryTrial>>,
}
impl Probe {
    fn new() -> Self {
        Self {
            effects: Mutex::new(vec![]),
            ready: AtomicBool::new(true),
            fail_commit: false,
            fail_rollback: false,
            hold_start: false,
            hold_commit: false,
            commit_entered: Notify::new(),
            release_commit: Notify::new(),
            invalidate: None,
            revoke: None,
            trial: Mutex::new(Weak::new()),
        }
    }
}
struct Mock(Arc<Probe>);
impl ProvisionBackend for Mock {
    async fn start(&mut self, c: &Credentials, _: &str) -> Result<()> {
        assert_eq!(c.ssid, "SYNTHETIC_NATIVE");
        assert_eq!(&*c.passphrase, "SYNTHETIC_NATIVE_PASSWORD");
        self.0.effects.lock().unwrap().push("temporary");
        if self.0.hold_start {
            std::future::pending::<()>().await;
        }
        Ok(())
    }
    async fn ready(&self) -> Result<bool> {
        Ok(self.0.ready.load(Ordering::Acquire))
    }
    async fn commit(&mut self, _: &Credentials) -> Result<()> {
        self.0.effects.lock().unwrap().push("persist");
        self.0.commit_entered.notify_one();
        if self.0.hold_commit {
            self.0.release_commit.notified().await;
        }
        if let Some(c) = &self.0.invalidate {
            c.invalidate();
        }
        if let Some(r) = &self.0.revoke {
            let (auth, token, g) = (r.auth.clone(), r.token.clone(), r.grant.clone());
            tokio::task::spawn_blocking(move || {
                auth.lock().unwrap().revoke_recovery_key(
                    &token,
                    g.independent_public_key,
                    g.generation,
                    id(91),
                )
            })
            .await
            .unwrap()
            .unwrap();
        }
        if self.0.fail_commit {
            Err(Error::Io)
        } else {
            Ok(())
        }
    }
    async fn rollback(&mut self) -> Result<()> {
        self.0.effects.lock().unwrap().push("restore");
        if self.0.fail_rollback {
            Err(Error::Io)
        } else {
            Ok(())
        }
    }
}
async fn start(f: &Fixture, c: &RecoveryConnection, n: u8, p: Arc<Probe>) -> Result<RecoveryReply> {
    let w = write(f, c, n, RecoveryAction::BeginWifi, None);
    begin_with(f.auth.clone(), w, move |trial| {
        *p.trial.lock().unwrap() = Arc::downgrade(&trial);
        Mock(p)
    })
    .await
}
fn started(reply: RecoveryReply) -> String {
    match reply {
        RecoveryReply::Started {
            trial_id,
            remaining,
            ..
        } => {
            assert!(remaining <= DEADLINE);
            trial_id
        }
        _ => panic!("start expected"),
    }
}
async fn ready_state() -> Arc<CoreRecoveryTrial> {
    timeout(Duration::from_secs(5), async {
        loop {
            {
                let a = ACTIVE.lock().await;
                if let Some(s) = a.as_ref() {
                    if *s.state.lock().await == State::Ready {
                        if let TrialOwner::Recovery(t) = &s.owner {
                            return t.clone();
                        }
                    }
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}
async fn drained() -> State {
    timeout(Duration::from_secs(5), async {
        loop {
            let state = {
                let a = ACTIVE.lock().await;
                a.as_ref().map(|s| s.state.clone())
            };
            if let Some(state) = state {
                let state = *state.lock().await;
                if state.terminal()
                    && TRIALS.available_permits() == 1
                    && crate::coordination::global().network().is_ok()
                {
                    return state;
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}
fn completed(reply: RecoveryReply, expected: RecoveryExecutionState) {
    match reply {
        RecoveryReply::Completed { state, .. } => assert_eq!(state, expected),
        _ => panic!("terminal expected"),
    }
}

#[tokio::test]
async fn original_actor_saves_only_after_independent_confirm_and_releases_credentials() {
    let _serial = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let p = Arc::new(Probe::new());
    let id = started(start(&f, &c, 1, p.clone()).await.unwrap());
    drop(ready_state().await);
    assert!(crate::coordination::global()
        .telephony(CallAction::Dial)
        .is_err());
    assert!(crate::coordination::global()
        .telephony(CallAction::Hangup)
        .unwrap()
        .is_none());
    assert_eq!(TRIALS.available_permits(), 0);
    let w = write(&f, &c, 2, RecoveryAction::ConfirmWifi, Some(&id));
    completed(
        control_recovery(f.auth.clone(), w).await.unwrap(),
        RecoveryExecutionState::Saved,
    );
    assert_eq!(drained().await, State::Saved);
    assert_eq!(&*p.effects.lock().unwrap(), &["temporary", "persist"]);
    assert_eq!(row(&f, 1), Some(("saved".into(), Some(1), None)));
    assert_eq!(row(&f, 2), Some(("saved".into(), Some(1), None)));
    assert!(p.trial.lock().unwrap().upgrade().is_none());
}
#[tokio::test]
async fn actual_cancel_restores_without_persist_and_records_both_receipts() {
    let _serial = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let p = Arc::new(Probe::new());
    let id = started(start(&f, &c, 1, p.clone()).await.unwrap());
    drop(ready_state().await);
    completed(
        control_recovery(
            f.auth.clone(),
            write(&f, &c, 2, RecoveryAction::CancelWifi, Some(&id)),
        )
        .await
        .unwrap(),
        RecoveryExecutionState::RolledBack,
    );
    drained().await;
    assert_eq!(&*p.effects.lock().unwrap(), &["temporary", "restore"]);
    for n in [1, 2] {
        assert_eq!(row(&f, n), Some(("rolled_back".into(), Some(0), Some(1))));
    }
}
#[tokio::test]
async fn normal_principal_cannot_steer_recovery_and_normal_active_trial_admits_no_recovery_row() {
    let _serial = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let p = Arc::new(Probe::new());
    let trial = started(start(&f, &c, 1, p).await.unwrap());
    drop(ready_state().await);
    let principal = f
        .auth
        .lock()
        .unwrap()
        .network_manager(f.owner.expose())
        .unwrap();
    {
        let a = ACTIVE.lock().await;
        let s = a.as_ref().unwrap();
        assert!(s.owned_by(&trial, &principal).is_err());
        assert!(s.replays(&principal, &[0; 32]).is_err());
    }
    control_recovery(
        f.auth.clone(),
        write(&f, &c, 2, RecoveryAction::CancelWifi, Some(&trial)),
    )
    .await
    .unwrap();
    drained().await;
    let (tx, _rx) = mpsc::channel(1);
    *ACTIVE.lock().await = Some(Session {
        id: trial.clone(),
        key: "synthetic-normal".into(),
        owner: TrialOwner::Normal {
            principal,
            fingerprint: [0; 32],
        },
        state: Arc::new(tokio::sync::Mutex::new(State::Ready)),
        control: tx,
        deadline: Instant::now() + DEADLINE,
    });
    assert!(matches!(
        start(&f, &c, 3, Arc::new(Probe::new())).await,
        Err(Error::Busy)
    ));
    assert!(row(&f, 3).is_none());
    assert!(matches!(
        control_recovery(
            f.auth.clone(),
            write(&f, &c, 4, RecoveryAction::CancelWifi, Some(&trial))
        )
        .await,
        Err(Error::Forbidden)
    ));
    assert!(row(&f, 4).is_none());
    *ACTIVE.lock().await = None;
}
#[tokio::test]
async fn original_call_lock_and_trial_slot_reject_before_a_new_reservation() {
    let _serial = SERIAL.lock().await;
    for voice in [false, true] {
        let f = fixture(40000);
        let c = connect(&f);
        let coordinator = crate::coordination::global();
        let call = if voice {
            coordinator.telephony(CallAction::Dial).unwrap()
        } else {
            None
        };
        let permit = if !voice {
            Some(TRIALS.try_acquire().unwrap())
        } else {
            None
        };
        assert!(matches!(
            start(&f, &c, 1, Arc::new(Probe::new())).await,
            Err(Error::Busy)
        ));
        assert!(row(&f, 1).is_none());
        drop(call);
        drop(permit);
    }
}
#[tokio::test]
async fn disconnected_or_revoked_before_confirm_rolls_back_on_the_original_gate() {
    let _serial = SERIAL.lock().await;
    for revoke in [false, true] {
        let f = fixture(40000);
        let c = connect(&f);
        let p = Arc::new(Probe::new());
        start(&f, &c, 1, p.clone()).await.unwrap();
        drop(ready_state().await);
        if revoke {
            f.auth
                .lock()
                .unwrap()
                .revoke_recovery_key(
                    f.owner.expose(),
                    f.grant.independent_public_key,
                    f.grant.generation,
                    id(91),
                )
                .unwrap();
        } else {
            c.invalidate();
        }
        assert_eq!(drained().await, State::RolledBack);
        assert_eq!(&*p.effects.lock().unwrap(), &["temporary", "restore"]);
        assert_eq!(row(&f, 1), Some(("rolled_back".into(), Some(0), Some(1))));
    }
}
#[tokio::test]
async fn post_commit_disconnect_or_revoke_never_reports_saved_and_compensates() {
    let _serial = SERIAL.lock().await;
    for revoke in [false, true] {
        let f = fixture(40000);
        let c = connect(&f);
        let mut probe = Probe::new();
        if revoke {
            probe.revoke = Some(Revoke {
                auth: f.auth.clone(),
                token: Zeroizing::new(f.owner.expose().into()),
                grant: f.grant.clone(),
            });
        } else {
            probe.invalidate = Some(c.clone());
        }
        let p = Arc::new(probe);
        let trial = started(start(&f, &c, 1, p.clone()).await.unwrap());
        drop(ready_state().await);
        completed(
            control_recovery(
                f.auth.clone(),
                write(&f, &c, 2, RecoveryAction::ConfirmWifi, Some(&trial)),
            )
            .await
            .unwrap(),
            RecoveryExecutionState::RolledBack,
        );
        drained().await;
        assert_eq!(
            &*p.effects.lock().unwrap(),
            &["temporary", "persist", "restore"]
        );
        for n in [1, 2] {
            assert_eq!(row(&f, n), Some(("rolled_back".into(), Some(0), Some(1))));
        }
    }
}
#[tokio::test]
async fn partial_persist_and_failed_compensation_leave_unknown_and_refuse_new_effects() {
    let _serial = SERIAL.lock().await;
    for fail_rollback in [false, true] {
        let f = fixture(40000);
        let c = connect(&f);
        let p = Arc::new(Probe {
            fail_commit: true,
            fail_rollback,
            ..Probe::new()
        });
        let trial = started(start(&f, &c, 1, p.clone()).await.unwrap());
        drop(ready_state().await);
        let expected = if fail_rollback {
            RecoveryExecutionState::Unknown
        } else {
            RecoveryExecutionState::RolledBack
        };
        completed(
            control_recovery(
                f.auth.clone(),
                write(&f, &c, 2, RecoveryAction::ConfirmWifi, Some(&trial)),
            )
            .await
            .unwrap(),
            expected,
        );
        drained().await;
        if fail_rollback {
            let next = Arc::new(Probe::new());
            assert!(start(&f, &c, 3, next.clone()).await.is_err());
            assert!(next.effects.lock().unwrap().is_empty());
            assert_eq!(row(&f, 1), Some(("unknown".into(), None, None)));
        }
    }
}
#[tokio::test]
async fn not_ready_or_full_control_slot_does_not_consume_a_durable_control_claim() {
    let _serial = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let p = Arc::new(Probe {
        ready: AtomicBool::new(false),
        ..Probe::new()
    });
    let trial = started(start(&f, &c, 1, p.clone()).await.unwrap());
    assert!(control_recovery(
        f.auth.clone(),
        write(&f, &c, 2, RecoveryAction::ConfirmWifi, Some(&trial))
    )
    .await
    .is_err());
    assert!(row(&f, 2).is_none());
    p.ready.store(true, Ordering::Release);
    drop(ready_state().await);
    let tx = ACTIVE.lock().await.as_ref().unwrap().control.clone();
    let slot = tx.try_reserve().unwrap();
    assert!(matches!(
        control_recovery(
            f.auth.clone(),
            write(&f, &c, 3, RecoveryAction::CancelWifi, Some(&trial))
        )
        .await,
        Err(Error::Busy)
    ));
    assert!(row(&f, 3).is_none());
    drop(slot);
    control_recovery(
        f.auth.clone(),
        write(&f, &c, 4, RecoveryAction::CancelWifi, Some(&trial)),
    )
    .await
    .unwrap();
    drained().await;
}
#[tokio::test]
async fn lost_reply_keeps_the_actual_actor_and_locks_until_work_completes() {
    let _serial = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let p = Arc::new(Probe {
        hold_commit: true,
        ..Probe::new()
    });
    let trial = started(start(&f, &c, 1, p.clone()).await.unwrap());
    drop(ready_state().await);
    let w = write(&f, &c, 2, RecoveryAction::ConfirmWifi, Some(&trial));
    let auth = f.auth.clone();
    let waiter = tokio::spawn(async move { control_recovery(auth, w).await });
    timeout(Duration::from_secs(5), p.commit_entered.notified())
        .await
        .unwrap();
    waiter.abort();
    assert!(matches!(waiter.await,Err(e) if e.is_cancelled()));
    assert_eq!(TRIALS.available_permits(), 0);
    assert!(crate::coordination::global().network().is_err());
    assert_eq!(row(&f, 2).unwrap().0, "saving");
    p.release_commit.notify_one();
    assert_eq!(drained().await, State::Saved);
    assert_eq!(row(&f, 2).unwrap().0, "saved");
    assert!(p.trial.lock().unwrap().upgrade().is_none());
}
#[tokio::test]
async fn shortened_original_deadline_cancels_an_inflight_start_and_records_cleanup() {
    let _serial = SERIAL.lock().await;
    let f = fixture(1000);
    let c = connect(&f);
    let p = Arc::new(Probe {
        hold_start: true,
        ..Probe::new()
    });
    start(&f, &c, 1, p.clone()).await.unwrap();
    assert_eq!(drained().await, State::RolledBack);
    assert_eq!(&*p.effects.lock().unwrap(), &["temporary", "restore"]);
    assert_eq!(row(&f, 1), Some(("rolled_back".into(), Some(0), Some(1))));
}
#[tokio::test]
async fn trusted_shutdown_cleans_up_without_fabricating_a_confirm_capability() {
    let _serial = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let p = Arc::new(Probe::new());
    start(&f, &c, 1, p.clone()).await.unwrap();
    drop(ready_state().await);
    stop_active().await;
    assert_eq!(drained().await, State::RolledBack);
    assert_eq!(&*p.effects.lock().unwrap(), &["temporary", "restore"]);
}
#[tokio::test]
async fn normal_control_proof_cannot_confirm_a_recovery_trial_even_on_the_internal_queue() {
    let _serial = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let p = Arc::new(Probe::new());
    start(&f, &c, 1, p.clone()).await.unwrap();
    drop(ready_state().await);
    let tx = ACTIVE.lock().await.as_ref().unwrap().control.clone();
    let (reply, rx) = oneshot::channel();
    tx.try_send(Control {
        authority: ControlAuthority::Normal,
        confirm: true,
        reply,
    })
    .unwrap();
    assert_eq!(rx.await.unwrap(), State::RolledBack);
    drained().await;
    assert_eq!(&*p.effects.lock().unwrap(), &["temporary", "restore"]);
}
#[tokio::test]
async fn public_entry_stays_disabled_and_the_usb_exception_is_a_private_recovery_branch() {
    let _serial = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let req = request(&f, &c, 1, RecoveryAction::BeginWifi, BODY);
    assert!(!enabled());
    assert!(matches!(
        recovery(f.auth.clone(), req).await,
        Err(Error::Forbidden)
    ));
    assert!(row(&f, 1).is_none());
    assert!(NmProvision::default().recovery.is_none());
    assert!(matches!(
        usb_fence(None, UsbPolicy::Required),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        usb_fence(None, UsbPolicy::Recovery),
        Ok(UsbFence::Recovery)
    ));
}

#[tokio::test]
async fn busy_auth_mutex_cannot_hide_an_actual_backend_rollback() {
    let _serial = SERIAL.lock().await;
    let f = fixture(40000);
    let c = connect(&f);
    let p = Arc::new(Probe::new());
    start(&f, &c, 1, p.clone()).await.unwrap();
    drop(ready_state().await);
    let auth = f.auth.clone();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let keeper = std::thread::spawn(move || {
        let _guard = auth.lock().unwrap();
        entered_tx.send(()).unwrap();
        let _ = release_rx.recv();
    });
    entered_rx.recv().unwrap();
    let terminal = drained().await;
    let receipt = row(&f, 1);
    release_tx.send(()).unwrap();
    keeper.join().unwrap();
    assert_eq!(terminal, State::RolledBack);
    assert_eq!(receipt, Some(("rolled_back".into(), Some(0), Some(1))));
    assert_eq!(&*p.effects.lock().unwrap(), &["temporary", "restore"]);
}
