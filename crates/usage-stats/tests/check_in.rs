// SPDX-License-Identifier: AGPL-3.0-only
#![cfg(target_os = "linux")]
use rustsimhub_usage_stats::{
    check_in::{
        AttemptBudget, Cancellation, CheckInMachine, CheckInStore, CheckInTransport, Error,
        Failure, Outcome, StatisticsLease,
    },
    identity::StatisticsIdentity,
    protocol::{
        CheckInIntent, Clock, ClockError, DisclosureAcknowledgement, EnabledPolicy, OwnerConsent,
        StatsPublicKey, Window, RENEW_INTERVAL_SECS, RETRY_AFTER_SECS,
    },
    query::TransportError,
    store::{FileStore, StoreError, StoredState},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

const START: u64 = 100 * RENEW_INTERVAL_SECS;
const W: Window = Window::Last24Hours;
fn granted() -> EnabledPolicy {
    EnabledPolicy::from_owner_consent(OwnerConsent::Granted, DisclosureAcknowledgement::Current)
        .unwrap()
}
#[derive(Clone)]
struct TestClock {
    value: Arc<AtomicU64>,
    calls: Arc<AtomicUsize>,
    unavailable: Arc<AtomicBool>,
}
impl Clock for TestClock {
    fn now_secs(&self) -> Result<u64, ClockError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.unavailable.load(Ordering::SeqCst) {
            Err(ClockError::Unavailable)
        } else {
            Ok(self.value.load(Ordering::SeqCst))
        }
    }
}
#[derive(Default)]
struct Counts {
    loads: AtomicUsize,
    saves: AtomicUsize,
    fault: AtomicUsize,
    cancel_on_load: Mutex<Option<Cancellation>>,
    delay_save_millis: AtomicU64,
}
struct ActualStore {
    file: FileStore,
    counts: Arc<Counts>,
}
impl CheckInStore for ActualStore {
    fn public_key(&self) -> StatsPublicKey {
        self.file.identity().public_key()
    }
    fn load(&self) -> Result<StoredState, StoreError> {
        self.counts.loads.fetch_add(1, Ordering::SeqCst);
        if let Some(cancel) = self.counts.cancel_on_load.lock().unwrap().take() {
            cancel.cancel();
        }
        self.file.load()
    }
    fn save(&self, state: &StoredState) -> Result<(), StoreError> {
        self.counts.saves.fetch_add(1, Ordering::SeqCst);
        let fault = self.counts.fault.swap(0, Ordering::SeqCst);
        if fault == 1 {
            return Err(StoreError::Io(std::io::ErrorKind::StorageFull));
        }
        self.file.save(state)?;
        std::thread::sleep(Duration::from_millis(
            self.counts.delay_save_millis.load(Ordering::SeqCst),
        ));
        if fault == 2 {
            return Err(StoreError::Io(std::io::ErrorKind::Other));
        }
        Ok(())
    }
}
#[derive(Default)]
struct Adapter {
    acquires: usize,
    releases: usize,
    prepares: usize,
    submits: usize,
    queries: usize,
    owners: HashSet<[u8; 16]>,
    other_owner: bool,
    fail_acquire: bool,
    fail_prepare: bool,
    fail_submit: bool,
    zero_digest: bool,
    response: String,
    cancellation: Option<(String, Cancellation)>,
    sleep: Option<(String, u64)>,
    submitted_digests: Vec<[u8; 32]>,
    queried_digests: Vec<[u8; 32]>,
    clock_change: Option<(String, TestClock, u64)>,
}
struct SyntheticTransport {
    state: Arc<Mutex<Adapter>>,
    directory: PathBuf,
}
fn stored(directory: &Path) -> StoredState {
    let mut wire: Value =
        serde_json::from_slice(&fs::read(directory.join("state.json")).unwrap()).unwrap();
    serde_json::from_value(wire["state"].take()).unwrap()
}
impl SyntheticTransport {
    fn hook(&self, stage: &str) {
        let state = self.state.lock().unwrap();
        if let Some((name, cancel)) = &state.cancellation {
            if name == stage {
                cancel.cancel();
            }
        }
        if let Some((name, clock, value)) = &state.clock_change {
            if name == stage {
                clock.value.store(*value, Ordering::SeqCst);
            }
        }
        if let Some((name, millis)) = &state.sleep {
            if name == stage {
                std::thread::sleep(Duration::from_millis(*millis));
            }
        }
    }
}
impl CheckInTransport for SyntheticTransport {
    fn acquire_statistics(
        &mut self,
        lease: &StatisticsLease,
        budget: &AttemptBudget,
    ) -> Result<(), TransportError> {
        assert!(!budget.cancelled());
        assert!(!budget.expired());
        assert_eq!(budget.max_reply_bytes(), 2048);
        {
            let mut a = self.state.lock().unwrap();
            a.acquires += 1;
            a.owners.insert(*lease.id());
        }
        self.hook("acquire");
        if self.state.lock().unwrap().fail_acquire {
            Err(TransportError::Unavailable)
        } else {
            Ok(())
        }
    }
    fn release_statistics(&mut self, lease: StatisticsLease) {
        let mut a = self.state.lock().unwrap();
        a.releases += 1;
        assert!(a.owners.remove(lease.id()));
        assert!(a.other_owner);
    }
    fn prepare(
        &mut self,
        lease: &StatisticsLease,
        intent: &CheckInIntent,
        _: &AttemptBudget,
    ) -> Result<[u8; 32], TransportError> {
        {
            let mut a = self.state.lock().unwrap();
            assert!(a.owners.contains(lease.id()));
            a.prepares += 1;
        }
        self.hook("prepare");
        let a = self.state.lock().unwrap();
        if a.fail_prepare {
            return Err(TransportError::Rejected);
        }
        // Synthetic transaction ID only. No production signer or chain exists
        // in this fixture; the real machine merely persists this adapter ID.
        Ok(if a.zero_digest {
            [0; 32]
        } else {
            Sha256::digest(intent.to_json().unwrap()).into()
        })
    }
    fn submit(
        &mut self,
        lease: &StatisticsLease,
        intent: &CheckInIntent,
        digest: [u8; 32],
        _: &AttemptBudget,
    ) -> Result<(), TransportError> {
        let s = stored(&self.directory);
        let row = &s.windows[intent.window().index()];
        assert_eq!(row.pending.as_ref().unwrap().digest, digest);
        assert_eq!(row.period, Some(intent.renewal_period()));
        {
            let mut a = self.state.lock().unwrap();
            assert!(a.owners.contains(lease.id()));
            a.submits += 1;
            a.submitted_digests.push(digest);
        }
        self.hook("submit");
        if self.state.lock().unwrap().fail_submit {
            Err(TransportError::Timeout)
        } else {
            Ok(())
        }
    }
    fn query(
        &mut self,
        lease: &StatisticsLease,
        intent: &CheckInIntent,
        digest: [u8; 32],
        _: &AttemptBudget,
    ) -> Result<Vec<u8>, TransportError> {
        {
            let mut a = self.state.lock().unwrap();
            assert!(a.owners.contains(lease.id()));
            a.queries += 1;
            a.queried_digests.push(digest);
        }
        self.hook("query");
        let mode = self.state.lock().unwrap().response.clone();
        let mut reply = json!({"intent":intent,"digest":digest,"status":"pending"});
        match mode.as_str() {
            "confirmed" | "rejected" => reply["status"] = mode.clone().into(),
            "wrong_identity" => {
                reply["intent"]["public_key"] =
                    serde_json::to_value(StatsPublicKey::from_bytes([3; 32])).unwrap()
            }
            "wrong_topic" => reply["intent"]["topic"] = "other-product".into(),
            "wrong_period" => {
                reply["intent"]["renewal_period"] = (intent.renewal_period() + 1).into()
            }
            "wrong_window" => {
                reply["intent"] = serde_json::to_value(
                    granted()
                        .check_in_intent(Window::Last90Days, intent.public_key(), &Fixed(START))
                        .unwrap()
                        .unwrap(),
                )
                .unwrap()
            }
            "wrong_digest" => reply["digest"] = serde_json::to_value([9_u8; 32]).unwrap(),
            "extra" => reply["private_seed"] = "SYNTHETIC_FORBIDDEN_EXTRA".into(),
            "unknown_status" => reply["status"] = "success".into(),
            "huge" => return Ok(vec![b' '; 2049]),
            "invalid" => return Ok(b"{SYNTHETIC_MALFORMED_INPUT".to_vec()),
            "offline" => return Err(TransportError::Offline),
            "duplicate" => {
                return Ok(serde_json::to_string(&reply)
                    .unwrap()
                    .replacen('{', "{\"status\":\"confirmed\",", 1)
                    .into_bytes())
            }
            _ => {}
        }
        Ok(serde_json::to_vec(&reply).unwrap())
    }
}
struct Fixed(u64);
impl Clock for Fixed {
    fn now_secs(&self) -> Result<u64, ClockError> {
        Ok(self.0)
    }
}
struct Harness {
    _parent: tempfile::TempDir,
    path: PathBuf,
    key: StatsPublicKey,
    clock: TestClock,
    counts: Arc<Counts>,
    adapter: Arc<Mutex<Adapter>>,
}
type Machine = CheckInMachine<ActualStore, SyntheticTransport, TestClock>;
impl Harness {
    fn new() -> Self {
        let parent = tempfile::tempdir().unwrap();
        let path = parent.path().join("statistics");
        let identity = StatisticsIdentity::from_seed([71; 32]);
        let key = identity.public_key();
        drop(FileStore::create(&path, identity).unwrap());
        Self {
            _parent: parent,
            path,
            key,
            clock: TestClock {
                value: Arc::new(AtomicU64::new(START)),
                calls: Arc::new(AtomicUsize::new(0)),
                unavailable: Arc::new(AtomicBool::new(false)),
            },
            counts: Arc::new(Counts::default()),
            adapter: Arc::new(Mutex::new(Adapter {
                other_owner: true,
                ..Adapter::default()
            })),
        }
    }
    fn open(&self) -> Machine {
        CheckInMachine::new(
            ActualStore {
                file: FileStore::open(&self.path, self.key).unwrap(),
                counts: self.counts.clone(),
            },
            SyntheticTransport {
                state: self.adapter.clone(),
                directory: self.path.clone(),
            },
            self.clock.clone(),
        )
    }
    fn enabled(&self) -> Machine {
        let mut m = self.open();
        m.set_policy(granted()).unwrap();
        m
    }
    fn later(&self) {
        self.clock
            .value
            .fetch_add(RETRY_AFTER_SECS, Ordering::SeqCst);
    }
}

#[test]
fn default_off_has_zero_clock_store_transport_or_resource_calls() {
    let h = Harness::new();
    let before = fs::read(h.path.join("state.json")).unwrap();
    let mut m = h.open();
    for w in Window::ALL {
        assert_eq!(m.advance(w).unwrap(), Outcome::Disabled);
    }
    m.set_policy(EnabledPolicy::disabled()).unwrap();
    drop(m);
    assert_eq!(h.clock.calls.load(Ordering::SeqCst), 0);
    assert_eq!(h.counts.loads.load(Ordering::SeqCst), 0);
    assert_eq!(h.counts.saves.load(Ordering::SeqCst), 0);
    let a = h.adapter.lock().unwrap();
    assert_eq!(
        (a.acquires, a.releases, a.prepares, a.submits, a.queries),
        (0, 0, 0, 0, 0)
    );
    assert_eq!(fs::read(h.path.join("state.json")).unwrap(), before);
}
#[test]
fn exact_intent_is_durable_before_single_submit_and_only_query_can_confirm() {
    let h = Harness::new();
    let mut m = h.enabled();
    let prepared = m.advance(W).unwrap();
    assert!(matches!(prepared, Outcome::Prepared { .. }));
    assert!(stored(&h.path).windows[0].pending.is_some());
    assert!(matches!(m.advance(W).unwrap(), Outcome::Pending { .. }));
    assert!(matches!(m.advance(W).unwrap(), Outcome::Due { .. }));
    assert!(stored(&h.path).windows[0].last_success.is_none());
    h.later();
    h.adapter.lock().unwrap().response = "confirmed".into();
    assert_eq!(m.advance(W).unwrap(), Outcome::Confirmed { period: 100 });
    assert_eq!(m.advance(W).unwrap(), Outcome::Confirmed { period: 100 });
    let state = stored(&h.path);
    assert!(state.windows[0].pending.is_none());
    assert_eq!(state.windows[0].last_success.as_ref().unwrap().period, 100);
    assert_eq!(
        (
            h.adapter.lock().unwrap().submits,
            h.counts.saves.load(Ordering::SeqCst)
        ),
        (1, 2)
    );
}
#[test]
fn crash_after_prepare_and_after_lost_submit_reply_are_query_only() {
    for submitted in [false, true] {
        let h = Harness::new();
        let mut m = h.enabled();
        m.advance(W).unwrap();
        if submitted {
            h.adapter.lock().unwrap().fail_submit = true;
            assert!(matches!(m.advance(W).unwrap(), Outcome::Unknown { .. }));
        }
        let digest = stored(&h.path).windows[0].pending.as_ref().unwrap().digest;
        drop(m);
        h.later();
        let mut m = h.enabled();
        assert!(matches!(m.advance(W).unwrap(), Outcome::Pending { .. }));
        let a = h.adapter.lock().unwrap();
        assert_eq!(a.prepares, 1);
        assert_eq!(a.submits, usize::from(submitted));
        assert_eq!(a.queried_digests, vec![digest]);
    }
}
#[test]
fn three_windows_are_independent_and_confirmed_renewal_skips_catchup_transactions() {
    let h = Harness::new();
    let mut m = h.enabled();
    for w in Window::ALL {
        assert!(matches!(m.advance(w).unwrap(), Outcome::Prepared { .. }));
        assert!(matches!(m.advance(w).unwrap(), Outcome::Pending { .. }));
    }
    h.later();
    h.adapter.lock().unwrap().response = "confirmed".into();
    m.advance(W).unwrap();
    assert!(stored(&h.path).windows[1].pending.is_some());
    h.clock
        .value
        .fetch_add(5 * RENEW_INTERVAL_SECS, Ordering::SeqCst);
    assert!(matches!(m.advance(W).unwrap(), Outcome::Prepared { .. }));
    assert_eq!(stored(&h.path).windows[0].period, Some(105));
    assert_eq!(h.adapter.lock().unwrap().prepares, 4);
}
#[test]
fn malicious_mismatched_duplicate_and_oversized_replies_never_erase_pending_or_confirm() {
    for mode in [
        "wrong_identity",
        "wrong_topic",
        "wrong_period",
        "wrong_window",
        "wrong_digest",
        "extra",
        "unknown_status",
        "huge",
        "invalid",
        "duplicate",
        "offline",
    ] {
        let h = Harness::new();
        let mut m = h.enabled();
        m.advance(W).unwrap();
        m.advance(W).unwrap();
        h.later();
        h.adapter.lock().unwrap().response = mode.into();
        let before = fs::read(h.path.join("state.json")).unwrap();
        let out = m.advance(W).unwrap();
        assert!(matches!(out, Outcome::Unknown { .. }), "{mode}");
        assert!(!format!("{out:?}").contains("SYNTHETIC"));
        assert_eq!(fs::read(h.path.join("state.json")).unwrap(), before);
        assert_eq!(h.adapter.lock().unwrap().submits, 1);
    }
}
#[test]
fn cancel_during_acquire_prepare_submit_or_query_fences_late_results_and_releases_only_owned_reference(
) {
    for stage in ["acquire", "prepare", "submit", "query"] {
        let h = Harness::new();
        let mut m = h.enabled();
        if ["submit", "query"].contains(&stage) {
            m.advance(W).unwrap();
        }
        if stage == "query" {
            m.advance(W).unwrap();
            h.later();
            h.adapter.lock().unwrap().response = "confirmed".into();
        }
        h.adapter.lock().unwrap().cancellation = Some((stage.into(), m.cancellation()));
        assert_eq!(m.advance(W).unwrap(), Outcome::Disabled);
        assert_eq!(stored(&h.path).disabled_generation, 1);
        assert!(stored(&h.path).windows[0].last_success.is_none());
        let a = h.adapter.lock().unwrap();
        assert_eq!(a.releases, 1);
        assert!(a.owners.is_empty());
        assert!(a.other_owner);
        if ["acquire", "prepare"].contains(&stage) {
            assert_eq!(a.submits, 0);
        }
    }
}
#[test]
fn disable_then_reenable_invalidates_prepared_ticket_without_resubmit() {
    let h = Harness::new();
    let mut m = h.enabled();
    m.advance(W).unwrap();
    m.set_policy(EnabledPolicy::disabled()).unwrap();
    let digest = stored(&h.path).windows[0].pending.as_ref().unwrap().digest;
    m.set_policy(granted()).unwrap();
    h.later();
    assert!(matches!(m.advance(W).unwrap(), Outcome::Pending { .. }));
    let a = h.adapter.lock().unwrap();
    assert_eq!(a.submits, 0);
    assert_eq!(a.queried_digests, vec![digest]);
    assert_eq!(stored(&h.path).disabled_generation, 1);
}
#[test]
fn idle_external_cancel_is_flushed_before_fresh_enable() {
    let h = Harness::new();
    let mut m = h.enabled();
    m.advance(W).unwrap();
    m.cancellation().cancel();
    m.set_policy(granted()).unwrap();
    assert_eq!(stored(&h.path).disabled_generation, 1);
    h.later();
    m.advance(W).unwrap();
    assert_eq!(h.adapter.lock().unwrap().submits, 0);
}
#[test]
fn prepare_failures_and_partial_resource_acquire_never_submit_or_stop_another_owner() {
    for mode in ["acquire", "prepare", "zero"] {
        let h = Harness::new();
        let mut m = h.enabled();
        {
            let mut a = h.adapter.lock().unwrap();
            a.fail_acquire = mode == "acquire";
            a.fail_prepare = mode == "prepare";
            a.zero_digest = mode == "zero";
        }
        assert!(matches!(m.advance(W).unwrap(), Outcome::Failed { .. }));
        assert!(matches!(m.advance(W).unwrap(), Outcome::Due { .. }));
        drop(m);
        let a = h.adapter.lock().unwrap();
        assert_eq!(a.submits, 0);
        assert_eq!(a.releases, 1);
        assert!(a.other_owner);
        assert!(a.owners.is_empty());
        assert!(stored(&h.path).windows[0].pending.is_none());
    }
}
#[test]
fn save_failure_before_or_after_atomic_publish_poison_handle_and_never_submit() {
    for fault in [1, 2] {
        let h = Harness::new();
        let mut m = h.enabled();
        h.counts.fault.store(fault, Ordering::SeqCst);
        assert!(matches!(m.advance(W), Err(Error::Store(_))));
        assert_eq!(m.advance(W), Err(Error::Poisoned));
        assert_eq!(h.adapter.lock().unwrap().submits, 0);
        assert_eq!(stored(&h.path).windows[0].pending.is_some(), fault == 2);
        drop(m);
        if fault == 2 {
            h.later();
            let mut reopened = h.enabled();
            assert!(matches!(
                reopened.advance(W).unwrap(),
                Outcome::Pending { .. }
            ));
            assert_eq!(h.adapter.lock().unwrap().prepares, 1);
            assert_eq!(h.adapter.lock().unwrap().submits, 0);
        }
    }
}
#[test]
fn failed_confirmation_save_never_reports_confirmed_and_reopen_queries_original() {
    let h = Harness::new();
    let mut m = h.enabled();
    m.advance(W).unwrap();
    m.advance(W).unwrap();
    h.later();
    h.adapter.lock().unwrap().response = "confirmed".into();
    h.counts.fault.store(1, Ordering::SeqCst);
    assert!(matches!(m.advance(W), Err(Error::Store(_))));
    assert!(stored(&h.path).windows[0].pending.is_some());
    drop(m);
    h.later();
    let mut m = h.enabled();
    assert_eq!(m.advance(W).unwrap(), Outcome::Confirmed { period: 100 });
    assert_eq!(h.adapter.lock().unwrap().submits, 1);
}
#[test]
fn query_rejection_consumes_period_and_is_not_a_new_transaction_on_reenable_or_restart() {
    let h = Harness::new();
    let mut m = h.enabled();
    m.advance(W).unwrap();
    m.advance(W).unwrap();
    h.later();
    h.adapter.lock().unwrap().response = "rejected".into();
    assert!(matches!(m.advance(W).unwrap(), Outcome::Failed { .. }));
    drop(m);
    let mut m = h.enabled();
    assert!(matches!(m.advance(W).unwrap(), Outcome::Failed { .. }));
    assert_eq!(h.adapter.lock().unwrap().prepares, 1);
    assert_eq!(h.adapter.lock().unwrap().submits, 1);
}
#[test]
fn pending_expiration_remains_unknown_and_blocks_later_period_submissions() {
    let h = Harness::new();
    let mut m = h.enabled();
    m.advance(W).unwrap();
    m.advance(W).unwrap();
    h.clock
        .value
        .fetch_add(2 * RENEW_INTERVAL_SECS, Ordering::SeqCst);
    assert!(matches!(
        m.advance(W).unwrap(),
        Outcome::Unknown {
            reason: Failure::ExpiredUnresolved,
            ..
        }
    ));
    h.later();
    m.advance(W).unwrap();
    assert_eq!(h.adapter.lock().unwrap().prepares, 1);
    assert_eq!(h.adapter.lock().unwrap().submits, 1);
    assert!(stored(&h.path).windows[0].pending.is_some());
}
#[test]
fn native_late_returns_have_no_success_and_cannot_reuse_submit_ticket() {
    for stage in ["prepare", "submit", "query"] {
        let h = Harness::new();
        let mut m = h.enabled().with_budget(Duration::from_millis(100)).unwrap();
        if ["submit", "query"].contains(&stage) {
            m.advance(W).unwrap();
        }
        if stage == "query" {
            m.advance(W).unwrap();
            h.later();
            h.adapter.lock().unwrap().response = "confirmed".into();
        }
        h.adapter.lock().unwrap().sleep = Some((stage.into(), 150));
        let out = m.advance(W).unwrap();
        assert!(matches!(
            out,
            Outcome::Failed {
                reason: Failure::Deadline
            } | Outcome::Unknown {
                reason: Failure::Deadline,
                ..
            }
        ));
        assert!(stored(&h.path).windows[0].last_success.is_none());
        h.adapter.lock().unwrap().sleep = None;
        if stage != "prepare" {
            h.later();
            m.advance(W).unwrap();
            assert_eq!(h.adapter.lock().unwrap().submits, 1);
        }
    }
}
#[test]
fn wall_clock_jump_rollback_and_unavailable_clock_do_not_create_or_adopt_success() {
    for new_time in [START - 1, START + 10] {
        let h = Harness::new();
        let mut m = h.enabled();
        m.advance(W).unwrap();
        m.advance(W).unwrap();
        h.later();
        let now = h.clock.value.load(Ordering::SeqCst);
        {
            let mut a = h.adapter.lock().unwrap();
            a.response = "confirmed".into();
            a.clock_change = Some((
                "query".into(),
                h.clock.clone(),
                if new_time < START { new_time } else { now + 10 },
            ));
        }
        let out = m.advance(W);
        assert!(matches!(
            out,
            Err(Error::ClockRollback)
                | Ok(Outcome::Unknown {
                    reason: Failure::Deadline,
                    ..
                })
        ));
        assert!(stored(&h.path).windows[0].last_success.is_none());
    }
    let h = Harness::new();
    let mut m = h.enabled();
    h.clock.unavailable.store(true, Ordering::SeqCst);
    assert_eq!(m.advance(W), Err(Error::Clock));
    assert_eq!(h.adapter.lock().unwrap().acquires, 0);
}
#[test]
fn restarted_clock_rollback_and_cross_period_prepared_ticket_never_submit() {
    let h = Harness::new();
    h.clock
        .value
        .store(START + RENEW_INTERVAL_SECS - 1, Ordering::SeqCst);
    let mut m = h.enabled();
    m.advance(W).unwrap();
    h.clock.value.fetch_add(1, Ordering::SeqCst);
    assert!(matches!(m.advance(W).unwrap(), Outcome::Due { .. }));
    assert_eq!(h.adapter.lock().unwrap().submits, 0);
    drop(m);
    h.clock.value.store(START - 1, Ordering::SeqCst);
    let mut m = h.enabled();
    assert_eq!(m.advance(W), Err(Error::ClockRollback));
    assert_eq!(h.adapter.lock().unwrap().submits, 0);
}
#[test]
fn corrupt_or_wrong_permission_store_is_preserved_without_another_prepare() {
    use std::os::unix::fs::PermissionsExt;
    for mode in ["corrupt", "permissions"] {
        let h = Harness::new();
        let mut m = h.enabled();
        m.advance(W).unwrap();
        let path = h.path.join("state.json");
        if mode == "corrupt" {
            fs::write(&path, b"{synthetic broken state").unwrap();
        } else {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        }
        let before = fs::read(&path).unwrap();
        assert!(matches!(m.advance(W), Err(Error::Store(_))));
        assert_eq!(fs::read(path).unwrap(), before);
        assert_eq!(h.adapter.lock().unwrap().prepares, 1);
        assert_eq!(h.adapter.lock().unwrap().submits, 0);
    }
}
#[test]
fn bounded_constructor_rejects_zero_and_extended_deadline() {
    for budget in [Duration::ZERO, Duration::from_secs(11)] {
        let h = Harness::new();
        assert!(matches!(
            h.open().with_budget(budget),
            Err(Error::InvalidBudget)
        ));
        assert_eq!(h.counts.loads.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn cancelling_while_enable_loads_cannot_resurrect_a_stale_policy_grant() {
    let h = Harness::new();
    let mut m = h.open();
    *h.counts.cancel_on_load.lock().unwrap() = Some(m.cancellation());
    m.set_policy(granted()).unwrap();
    assert_eq!(m.advance(W).unwrap(), Outcome::Disabled);
    assert_eq!(h.clock.calls.load(Ordering::SeqCst), 0);
    assert_eq!(h.adapter.lock().unwrap().acquires, 0);
    m.set_policy(granted()).unwrap();
    assert!(matches!(m.advance(W).unwrap(), Outcome::Prepared { .. }));
}

#[test]
fn confirmation_commit_exceeding_deadline_is_not_returned_as_timely_success() {
    let h = Harness::new();
    let mut m = h.enabled().with_budget(Duration::from_millis(100)).unwrap();
    m.advance(W).unwrap();
    m.advance(W).unwrap();
    h.later();
    h.adapter.lock().unwrap().response = "confirmed".into();
    h.counts.delay_save_millis.store(150, Ordering::SeqCst);
    assert!(matches!(
        m.advance(W).unwrap(),
        Outcome::Unknown {
            reason: Failure::Deadline,
            ..
        }
    ));
    // A real, timely proof was already durably committed. Do not undo it or
    // invent another transaction merely because returning its result was late.
    assert_eq!(
        stored(&h.path).windows[0]
            .last_success
            .as_ref()
            .unwrap()
            .period,
        100
    );
    assert_eq!(h.adapter.lock().unwrap().submits, 1);
}

#[test]
fn expired_in_process_ticket_becomes_query_only_and_old_period_confirmation_does_not_advance_period(
) {
    let h = Harness::new();
    let mut m = h.enabled().with_budget(Duration::from_millis(100)).unwrap();
    m.advance(W).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    assert!(matches!(m.advance(W).unwrap(), Outcome::Due { .. }));
    assert_eq!(h.adapter.lock().unwrap().submits, 0);
    h.clock
        .value
        .fetch_add(2 * RENEW_INTERVAL_SECS, Ordering::SeqCst);
    h.adapter.lock().unwrap().response = "confirmed".into();
    assert_eq!(m.advance(W).unwrap(), Outcome::Confirmed { period: 100 });
    assert_eq!(stored(&h.path).windows[0].period, Some(100));
    assert!(matches!(m.advance(W).unwrap(), Outcome::Due { .. }));
    h.later();
    assert!(matches!(m.advance(W).unwrap(), Outcome::Prepared { .. }));
    assert_eq!(stored(&h.path).windows[0].period, Some(102));
}

#[test]
fn disable_generation_overflow_fails_closed_without_erasing_original_pending() {
    let h = Harness::new();
    let f = FileStore::open(&h.path, h.key).unwrap();
    let mut state = f.load().unwrap();
    state.disabled_generation = u64::MAX;
    f.save(&state).unwrap();
    drop(f);
    let mut m = h.enabled();
    m.advance(W).unwrap();
    assert_eq!(
        m.set_policy(EnabledPolicy::disabled()),
        Err(Error::Exhausted)
    );
    assert_eq!(m.set_policy(granted()), Err(Error::Poisoned));
    assert!(stored(&h.path).windows[0].pending.is_some());
    assert_eq!(stored(&h.path).disabled_generation, u64::MAX);
    assert_eq!(h.adapter.lock().unwrap().submits, 0);
}

#[test]
fn concurrent_read_only_reporter_observes_complete_snapshots_without_duplication() {
    let h = Harness::new();
    let mut m = h.enabled();
    m.advance(W).unwrap();
    h.adapter.lock().unwrap().sleep = Some(("submit".into(), 25));
    let path = h.path.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    let (ready, started) = std::sync::mpsc::sync_channel(1);
    let reporter = std::thread::spawn(move || {
        let mut reads = 0;
        stored(&path).validate().unwrap();
        ready.send(()).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !stopped.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            stored(&path).validate().unwrap();
            reads += 1;
        }
        reads
    });
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    m.advance(W).unwrap();
    h.later();
    h.adapter.lock().unwrap().response = "confirmed".into();
    m.advance(W).unwrap();
    stop.store(true, Ordering::SeqCst);
    assert!(reporter.join().unwrap() > 0);
    assert_eq!(h.adapter.lock().unwrap().submits, 1);
    assert_eq!(h.adapter.lock().unwrap().queries, 1);
}
