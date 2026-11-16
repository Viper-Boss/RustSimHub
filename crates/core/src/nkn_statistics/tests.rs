// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rustsimhub_policy::{Kind, ALL_SCOPES};
use rustsimhub_usage_stats::{
    check_in::{
        Cancellation, CheckInMachine, CheckInStore, CheckInTransport, Outcome, StatisticsLease,
    },
    identity::StatisticsIdentity,
    protocol::{Clock, ClockError, Window, RETRY_AFTER_SECS},
    query::TransportError,
    store::{participation::Change, StoreError, StoredState},
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
pub(crate) struct Fixture {
    state: tempfile::TempDir,
    _identity: tempfile::TempDir,
    store: rustsimhub_storage::Store,
    pub(crate) auth: Arc<Mutex<Auth>>,
    token: String,
    owner: Principal,
    binding: Binding,
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
impl Fixture {
    pub(crate) fn new() -> Self {
        let state = tempfile::tempdir().unwrap();
        let identity = tempfile::tempdir().unwrap();
        for d in [state.path(), identity.path()] {
            fs::set_permissions(d, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let store = rustsimhub_storage::Store::open(state.path()).unwrap();
        let mut auth = Auth::open(identity.path(), store.handle()).unwrap();
        let key = SigningKey::from_bytes(&[71; 32]);
        let c = auth
            .bootstrap_challenge(key.verifying_key().to_bytes())
            .unwrap();
        auth.bootstrap_finish(&c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap();
        let token = login(&mut auth, &key);
        let owner = store
            .handle()
            .principal(key.verifying_key().to_bytes())
            .unwrap()
            .unwrap();
        let binding = Binding {
            core_public_key: auth.public_key(),
            realm: auth.durable_realm().unwrap(),
        };
        Self {
            state,
            _identity: identity,
            store,
            auth: Arc::new(Mutex::new(auth)),
            token,
            owner,
            binding,
        }
    }
    pub(crate) fn permission(&self) -> PathBuf {
        self.state.path().join("usage-participation")
    }
    pub(crate) fn actor(&self) -> Actor {
        Actor {
            public_key: self.owner.public_key,
            generation: self.owner.generation,
        }
    }
    pub(crate) fn consent(&self, enabled: bool) {
        drop(
            Ledger::create(
                &self.permission(),
                self.binding.clone(),
                self.actor(),
                change(1, 0, enabled),
            )
            .unwrap(),
        );
    }
    pub(crate) fn signer(&self) -> SubscriptionSigner {
        SubscriptionSigner::from_current_owner(self.auth.clone(), &self.token, &self.permission())
            .unwrap()
    }
    pub(crate) fn statistics(&self) -> Arc<FileStore> {
        Arc::new(
            FileStore::create(
                self.state.path().join("statistics"),
                StatisticsIdentity::from_seed([41; 32]),
            )
            .unwrap(),
        )
    }
}
fn change(n: u32, expected: u64, enabled: bool) -> Change {
    Change {
        request_id: format!("{n:08x}-1111-4111-8111-111111111111"),
        core_instance: "c".repeat(64),
        expected_generation: expected,
        enabled,
        disclosure_version: enabled.then_some(1),
    }
}
pub(crate) fn enabled() -> EnabledPolicy {
    EnabledPolicy::from_owner_consent(OwnerConsent::Granted, DisclosureAcknowledgement::Current)
        .unwrap()
}
struct At(u64);
impl Clock for At {
    fn now_secs(&self) -> std::result::Result<u64, ClockError> {
        Ok(self.0)
    }
}

#[derive(Clone)]
struct SharedStore(Arc<FileStore>);
impl CheckInStore for SharedStore {
    fn public_key(&self) -> rustsimhub_usage_stats::protocol::StatsPublicKey {
        self.0.identity().public_key()
    }
    fn load(&self) -> std::result::Result<StoredState, StoreError> {
        self.0.load()
    }
    fn save(&self, s: &StoredState) -> std::result::Result<(), StoreError> {
        self.0.save(s)
    }
}
#[derive(Default)]
struct Observed {
    prepared: Mutex<Option<PreparedSubscription>>,
    failure: Mutex<Option<Error>>,
    submits: AtomicUsize,
    queries: AtomicUsize,
    releases: AtomicUsize,
}
// Deliberately local test adapter: no claimed Root/SDK nonce or RPC support.
// It exercises the ORIGINAL state machine's budget and durable submit boundary.
struct LocalAdapter {
    signer: SubscriptionSigner,
    store: Arc<FileStore>,
    seen: Arc<Observed>,
    before: Option<Box<dyn FnMut()>>,
    foreign: bool,
}
impl CheckInTransport for LocalAdapter {
    fn acquire_statistics(
        &mut self,
        _: &StatisticsLease,
        _: &AttemptBudget,
    ) -> std::result::Result<(), TransportError> {
        Ok(())
    }
    fn release_statistics(&mut self, _: StatisticsLease) {
        self.seen.releases.fetch_add(1, Ordering::SeqCst);
    }
    fn prepare(
        &mut self,
        _: &StatisticsLease,
        intent: &CheckInIntent,
        budget: &AttemptBudget,
    ) -> std::result::Result<[u8; 32], TransportError> {
        if let Some(mut before) = self.before.take() {
            before();
        }
        let intent = if self.foreign {
            enabled()
                .check_in_intent(
                    intent.window(),
                    StatisticsIdentity::from_seed([99; 32]).public_key(),
                    &SystemClock,
                )
                .unwrap()
                .unwrap()
        } else {
            *intent
        };
        match self.signer.prepare(&self.store, intent, u64::MAX, budget) {
            Ok(prepared) => {
                let digest = prepared.digest();
                *self.seen.prepared.lock().unwrap() = Some(prepared);
                Ok(digest)
            }
            Err(error) => {
                *self.seen.failure.lock().unwrap() = Some(error);
                Err(TransportError::Unavailable)
            }
        }
    }
    fn submit(
        &mut self,
        _: &StatisticsLease,
        intent: &CheckInIntent,
        digest: [u8; 32],
        _: &AttemptBudget,
    ) -> std::result::Result<(), TransportError> {
        let persisted = self.store.load().unwrap();
        assert_eq!(
            persisted.windows[intent.window().index()]
                .pending
                .as_ref()
                .unwrap()
                .digest,
            digest
        );
        self.seen.submits.fetch_add(1, Ordering::SeqCst);
        Err(TransportError::Unavailable)
    }
    fn query(
        &mut self,
        _: &StatisticsLease,
        intent: &CheckInIntent,
        digest: [u8; 32],
        _: &AttemptBudget,
    ) -> std::result::Result<Vec<u8>, TransportError> {
        self.seen.queries.fetch_add(1, Ordering::SeqCst);
        Ok(serde_json::to_vec(
            &serde_json::json!({"intent":intent,"digest":digest,"status":"pending"}),
        )
        .unwrap())
    }
}
fn machine(
    signer: SubscriptionSigner,
    store: Arc<FileStore>,
    seen: Arc<Observed>,
    before: Option<Box<dyn FnMut()>>,
) -> CheckInMachine<SharedStore, LocalAdapter, SystemClock> {
    let mut machine = CheckInMachine::new(
        SharedStore(store.clone()),
        LocalAdapter {
            signer,
            store,
            seen,
            before,
            foreign: false,
        },
        SystemClock,
    );
    machine.set_policy(enabled()).unwrap();
    machine
}

#[test]
fn absence_off_invalid_token_clients_and_nas_never_create_a_statistics_identity() {
    let f = Fixture::new();
    assert!(
        SubscriptionSigner::from_current_owner(f.auth.clone(), &f.token, &f.permission()).is_err()
    );
    assert!(!f.permission().exists());
    assert!(!f.state.path().join("statistics").exists());
    f.consent(false);
    assert!(
        SubscriptionSigner::from_current_owner(f.auth.clone(), &f.token, &f.permission()).is_err()
    );
    assert!(SubscriptionSigner::from_current_owner(
        f.auth.clone(),
        &"0".repeat(64),
        &f.permission()
    )
    .is_err());
    for (kind, seed, scopes) in [(Kind::Client, 72, ALL_SCOPES), (Kind::Nas, 73, 1)] {
        let key = SigningKey::from_bytes(&[seed; 32]);
        f.store
            .handle()
            .approve_client(
                f.owner.clone(),
                key.verifying_key().to_bytes(),
                kind,
                scopes,
            )
            .unwrap();
        let token = login(&mut f.auth.lock().unwrap(), &key);
        assert!(
            SubscriptionSigner::from_current_owner(f.auth.clone(), &token, &f.permission())
                .is_err()
        );
    }
    assert!(!f.state.path().join("statistics").exists());
}
#[test]
fn original_machine_persists_real_digest_before_single_submit_and_restart_only_queries() {
    let f = Fixture::new();
    f.consent(true);
    let signer = f.signer();
    assert!(signer.authority.deadline() <= Instant::now() + Duration::from_secs(300));
    let store = f.statistics();
    let seen = Arc::new(Observed::default());
    let mut m = machine(signer, store.clone(), seen.clone(), None);
    let outcome = m.advance(Window::Last24Hours).unwrap();
    let Outcome::Prepared { digest } = outcome else {
        panic!("not prepared: {outcome:?}")
    };
    {
        let held = seen.prepared.lock().unwrap();
        let prepared = held.as_ref().unwrap();
        assert_eq!(prepared.digest(), digest);
        assert_eq!(prepared.nonce(), u64::MAX);
        VerifyingKey::from_bytes(store.identity().public_key().as_bytes())
            .unwrap()
            .verify_strict(
                prepared.signing_hash(),
                &Signature::from_bytes(prepared.signature()),
            )
            .unwrap();
    }
    assert_eq!(
        store.load().unwrap().windows[0]
            .pending
            .as_ref()
            .unwrap()
            .digest,
        digest
    );
    assert_eq!(seen.submits.load(Ordering::SeqCst), 0);
    // The test transport loses its sole submission acknowledgement.
    assert!(matches!(
        m.advance(Window::Last24Hours).unwrap(),
        Outcome::Unknown { .. }
    ));
    assert_eq!(seen.submits.load(Ordering::SeqCst), 1);
    drop(m);
    let reopened = Arc::new(Observed::default());
    let mut m = machine(f.signer(), store.clone(), reopened.clone(), None);
    assert!(
        matches!(m.advance(Window::Last24Hours).unwrap(), Outcome::Due { .. }),
        "Restart keeps the original 300s cooldown"
    );
    drop(m);
    let due = store.load().unwrap().windows[0]
        .pending
        .as_ref()
        .unwrap()
        .prepared_at
        + RETRY_AFTER_SECS;
    let adapter = LocalAdapter {
        signer: f.signer(),
        store: store.clone(),
        seen: reopened.clone(),
        before: None,
        foreign: false,
    };
    let mut after = CheckInMachine::new(SharedStore(store.clone()), adapter, At(due));
    after.set_policy(enabled()).unwrap();
    assert!(matches!(
        after.advance(Window::Last24Hours).unwrap(),
        Outcome::Pending { .. }
    ));
    assert!(reopened.prepared.lock().unwrap().is_none());
    assert_eq!(reopened.submits.load(Ordering::SeqCst), 0);
    assert_eq!(reopened.queries.load(Ordering::SeqCst), 1);
    assert!(store.load().unwrap().windows[0].pending.is_some());
}
#[test]
fn completed_off_reenable_realm_and_owner_generation_changes_fence_existing_signer() {
    for mode in 0..4 {
        let f = Fixture::new();
        f.consent(true);
        let signer = f.signer();
        let store = f.statistics();
        let seen = Arc::new(Observed::default());
        let ledger_path = f.permission();
        let binding = f.binding.clone();
        let actor = f.actor();
        let database = f.state.path().join("store.sqlite3");
        let before = Box::new(move || match mode {
            0 | 1 => {
                let ledger = Ledger::open_if_present(&ledger_path, binding.clone())
                    .unwrap()
                    .unwrap();
                ledger.change(actor.clone(), change(2, 1, false)).unwrap();
                if mode == 1 {
                    ledger.change(actor.clone(), change(3, 2, true)).unwrap();
                }
            }
            2 => {
                // An independent connection changes only this private fixture's
                // durable realm, modeling the fence without widening Store API.
                let db = rusqlite::Connection::open(&database).unwrap();
                db.execute("UPDATE meta SET db_epoch='aaaaaaaa-1111-4111-8111-111111111111' WHERE singleton=1",[]).unwrap();
            }
            3 => {
                let db = rusqlite::Connection::open(&database).unwrap();
                db.execute(
                    "UPDATE principals SET generation=generation+1 WHERE kind='owner'",
                    [],
                )
                .unwrap();
            }
            _ => unreachable!(),
        });
        let mut m = machine(signer, store.clone(), seen.clone(), Some(before));
        assert!(matches!(
            m.advance(Window::Last24Hours).unwrap(),
            Outcome::Failed { .. }
        ));
        assert!(seen.prepared.lock().unwrap().is_none());
        assert_eq!(*seen.failure.lock().unwrap(), Some(Error::Denied));
        assert!(store
            .load()
            .unwrap()
            .windows
            .iter()
            .all(|w| w.pending.is_none()));
        assert_eq!(seen.submits.load(Ordering::SeqCst), 0);
    }
}
#[test]
fn invalidated_finite_proof_corrupt_store_and_wrong_identity_never_return_a_signature() {
    for mode in 0..3 {
        let f = Fixture::new();
        f.consent(true);
        let signer = f.signer();
        if mode == 0 {
            signer.authority.invalidate();
        }
        let store = f.statistics();
        if mode == 1 {
            fs::write(
                f.state.path().join("statistics/state.json"),
                b"synthetic corrupt store",
            )
            .unwrap();
        }
        let seen = Arc::new(Observed::default());
        if mode == 1 {
            // The original machine refuses this store before invoking any adapter.
            let adapter = LocalAdapter {
                signer,
                store: store.clone(),
                seen: seen.clone(),
                before: None,
                foreign: false,
            };
            let mut m = CheckInMachine::new(SharedStore(store.clone()), adapter, SystemClock);
            assert!(m.set_policy(enabled()).is_err());
            assert!(seen.prepared.lock().unwrap().is_none());
            assert_eq!(
                fs::read(f.state.path().join("statistics/state.json")).unwrap(),
                b"synthetic corrupt store"
            );
            continue;
        }
        let adapter = LocalAdapter {
            signer,
            store: store.clone(),
            seen: seen.clone(),
            before: None,
            foreign: mode == 2,
        };
        let mut m = CheckInMachine::new(SharedStore(store.clone()), adapter, SystemClock);
        m.set_policy(enabled()).unwrap();
        assert!(matches!(
            m.advance(Window::Last24Hours).unwrap(),
            Outcome::Failed { .. }
        ));
        assert!(seen.prepared.lock().unwrap().is_none());
        assert!(store
            .load()
            .unwrap()
            .windows
            .iter()
            .all(|w| w.pending.is_none()));
        assert_eq!(seen.submits.load(Ordering::SeqCst), 0);
    }
}
#[test]
fn exact_attempt_cancellation_and_deadline_are_checked_before_signing() {
    for expired in [false, true] {
        let f = Fixture::new();
        f.consent(true);
        let store = f.statistics();
        let seen = Arc::new(Observed::default());
        let cancellation: Arc<Mutex<Option<Cancellation>>> = Arc::new(Mutex::new(None));
        let cancel = cancellation.clone();
        let before = Box::new(move || {
            if expired {
                std::thread::sleep(Duration::from_millis(150));
            } else {
                cancel.lock().unwrap().as_ref().unwrap().cancel();
            }
        });
        let mut m = machine(f.signer(), store.clone(), seen.clone(), Some(before))
            .with_budget(Duration::from_millis(100))
            .unwrap();
        *cancellation.lock().unwrap() = Some(m.cancellation());
        let _ = m.advance(Window::Last24Hours).unwrap();
        assert!(seen.prepared.lock().unwrap().is_none());
        assert_eq!(*seen.failure.lock().unwrap(), Some(Error::Deadline));
        assert_eq!(seen.submits.load(Ordering::SeqCst), 0);
        assert!(store
            .load()
            .unwrap()
            .windows
            .iter()
            .all(|w| w.pending.is_none()));
    }
}
