// SPDX-License-Identifier: AGPL-3.0-only
//! Real Auth/Machine budgets and original Supervisor reference bookkeeping.
//! Socketpair tests do NOT assert installed Root/kernel/cgroup readiness.
use super::*;
use crate::nkn_root::transactions::tests::with_budget;
use crate::nkn_statistics::tests::Fixture;
use rustsimhub_nkn_agent::{Owner, Supervisor, SystemClock};
use rustsimhub_usage_stats::{
    check_in::{Failure, Outcome},
    protocol::{
        Clock, DisclosureAcknowledgement, EnabledPolicy, OwnerConsent, SystemClock as UsageClock,
        Window, RETRY_AFTER_SECS,
    },
    store::{
        participation::{Binding, Change, Ledger},
        PendingTransaction,
    },
};
use std::thread;

fn shared(f: &Fixture, signer: &SubscriptionSigner, store: &FileStore) -> Arc<Shared> {
    let authority = f
        .auth
        .lock()
        .unwrap()
        .nkn_root_authority(signer.owner_token())
        .unwrap();
    let deadline = authority.deadline();
    Arc::new(Shared {
        auth: f.auth.clone(),
        authority,
        generation: AtomicU32::new(0),
        nonce: [1; 32],
        policy: [2; 32],
        deadline,
        boot_deadline: boot_ms().unwrap()
            + u64::try_from(
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis(),
            )
            .unwrap(),
        live: AtomicBool::new(true),
        sent: AtomicBool::new(false),
        connection: Mutex::new(Connection {
            stream: None,
            poisoned: false,
        }),
        observed_ready: AtomicBool::new(false),
        observed_pid: AtomicU32::new(0),
        observed_at: AtomicU64::new(0),
        observed_root: AtomicU32::new(0),
        observed_engine: AtomicU32::new(0),
        sdk_status: true,
        sdk_usage: true,
        transaction_key: Some(store.identity().public_key()),
        host_closed: OnceLock::new(),
        startup: Mutex::new(None),
    })
}
fn off(f: &Fixture) {
    let binding = {
        let a = f.auth.lock().unwrap();
        Binding {
            core_public_key: a.public_key(),
            realm: a.durable_realm().unwrap(),
        }
    };
    let deadline = Instant::now() + Duration::from_millis(300);
    let ledger = loop {
        match Ledger::open_if_present(&f.permission(), binding.clone()) {
            Ok(Some(ledger)) => break ledger,
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(1)),
            _ => panic!("actual OFF could not acquire the original bounded permission lock"),
        }
    };
    let generation = ledger.snapshot().unwrap().generation;
    ledger
        .change(
            f.actor(),
            Change {
                request_id: "aaaaaaaa-1111-4111-8111-111111111111".into(),
                core_instance: "c".repeat(64),
                expected_generation: generation,
                enabled: false,
                disclosure_version: None,
            },
        )
        .unwrap();
}
#[test]
fn cloned_budget_is_original_deadline_and_cancellation_not_a_new_attempt() {
    let f = Fixture::new();
    f.consent(true);
    let store = f.statistics();
    with_budget(store, |_, budget, cancel| {
        let retained = budget.clone();
        assert_eq!(retained.deadline(), budget.deadline());
        assert_eq!(retained.max_reply_bytes(), budget.max_reply_bytes());
        assert!(!retained.cancelled());
        cancel.cancel();
        assert!(retained.cancelled() && budget.cancelled());
        assert_eq!(retained.deadline(), budget.deadline());
    });
}
fn startup_wait(revoke: bool) {
    let f = Fixture::new();
    f.consent(true);
    let store = f.statistics();
    let signer = Arc::new(f.signer());
    with_budget(store.clone(), |_, budget, cancel| {
        let core = shared(&f, &signer, &store);
        *core.startup.lock().unwrap() = Some(Arc::new(Startup {
            signer: signer.clone(),
            store: store.clone(),
            budget: budget.clone(),
        }));
        let (ours, mut server) = UnixStream::pair().unwrap();
        *core.connection.lock().unwrap() = Connection {
            stream: Some(ours),
            poisoned: false,
        };
        thread::scope(|s| {
            let child =
                s.spawn(|| core.port_request(1, Instant::now() + Duration::from_secs(3), false));
            server
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut request = [0; 82];
            server.read_exact(&mut request).unwrap();
            assert_eq!(&request[..6], b"RSCN\x01\x01");
            let at = Instant::now();
            if revoke {
                off(&f);
            } else {
                cancel.cancel();
            }
            let result = child.join().unwrap();
            if revoke {
                assert!(result.is_err(), "startup accepted completed OFF");
            } else {
                assert!(
                    matches!(result, Err(PortError::Deadline)),
                    "startup did not reject original cancellation"
                );
            }
            assert!(
                at.elapsed() < Duration::from_millis(250),
                "startup ignored original cancellation/consent"
            );
        });
        assert!(!core.live.load(Ordering::Acquire));
        assert!(core.connection.lock().unwrap().poisoned);
    });
}
#[test]
fn original_cancel_interrupts_startup_read_while_root_is_silent() {
    startup_wait(false)
}
#[test]
fn completed_off_interrupts_startup_read_without_waiting_for_root_reply() {
    startup_wait(true)
}

// These tests make NO Ready assertion and do not start a fake Root or SDK.
// They exercise exact ownership changes on the actual Supervisor/Controller.
fn reference_fixture(
    f: &Fixture,
    store: &FileStore,
    signer: &SubscriptionSigner,
    id: [u8; 16],
    query: bool,
) -> Controller {
    let shared = shared(f, signer, store);
    let mut supervisor =
        Supervisor::new(Root(shared.clone()), Engine(shared.clone()), SystemClock).unwrap();
    let manual = supervisor.acquire(Owner::ManualEngine).unwrap();
    let remote = supervisor.acquire(Owner::Remote).unwrap();
    let statistics = supervisor.acquire(Owner::Statistics).unwrap();
    let c = Controller::new(f.auth.clone());
    *c.run.lock().unwrap() = Some(Run {
        supervisor,
        source: Arc::new(Source {
            shared,
            reference: None,
        }),
        references: [Some(manual), Some(remote), Some(statistics)],
        remote: Some(Arc::new(AtomicBool::new(true))),
        statistics_epoch: 1,
        statistics_query: query,
        check_in: Some(id),
    });
    c
}
#[test]
fn release_only_reporting_preserves_query_remote_and_manual_references() {
    let f = Fixture::new();
    f.consent(true);
    let store = f.statistics();
    let signer = f.signer();
    with_budget(store.clone(), |lease, _, _| {
        let c = reference_fixture(&f, &store, &signer, *lease.id(), true);
        c.release_check_in(&[0; 16]);
        assert_eq!(c.snapshot().unwrap().unwrap().references, 3);
        c.release_check_in(lease.id());
        assert_eq!(c.snapshot().unwrap().unwrap().references, 3);
        assert!(c.run.lock().unwrap().as_ref().unwrap().statistics_query);
        c.disable(signer.owner_token(), Owner::Statistics).unwrap();
        let state = c.run.lock().unwrap();
        let run = state.as_ref().unwrap();
        assert_eq!(run.supervisor.snapshot().references, 2);
        assert!(run.references[0].is_some() && run.references[1].is_some());
        assert!(run.remote.as_ref().unwrap().load(Ordering::Acquire));
    });
}
#[test]
fn query_off_preserves_reporting_and_reporting_off_does_not_require_live_auth() {
    let f = Fixture::new();
    f.consent(true);
    let store = f.statistics();
    let signer = f.signer();
    with_budget(store.clone(), |lease, _, _| {
        let c = reference_fixture(&f, &store, &signer, *lease.id(), true);
        c.disable(signer.owner_token(), Owner::Statistics).unwrap();
        assert_eq!(c.snapshot().unwrap().unwrap().references, 3);
        {
            let state = c.run.lock().unwrap();
            let run = state.as_ref().unwrap();
            assert_eq!(run.check_in, Some(*lease.id()));
            assert_eq!(run.statistics_epoch, 1);
        }
        // Invalidate the actual original finite Root authority, then release
        // without minting any replacement authority.
        c.run
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .source
            .shared
            .authority
            .invalidate();
        c.release_check_in(lease.id());
        c.release_check_in(lease.id());
        assert_eq!(c.snapshot().unwrap().unwrap().references, 2);
    });
}
#[test]
fn completed_off_prevents_run_once_from_acquiring_or_changing_pending() {
    let f = Fixture::new();
    f.consent(true);
    let store = f.statistics();
    let signer = f.signer();
    let root = Arc::new(Controller::new(f.auth.clone()));
    let transport = StatisticsTransport::new(root.clone(), signer, store.clone()).unwrap();
    off(&f);
    let before = store.load().unwrap();
    assert!(matches!(
        transport.run_once(Window::Last24Hours).unwrap(),
        Outcome::Failed {
            reason: Failure::Prepare
        }
    ));
    assert_eq!(
        serde_json::to_value(store.load().unwrap()).unwrap(),
        serde_json::to_value(before).unwrap()
    );
    assert!(root.snapshot().unwrap().is_none());
}
#[test]
fn restart_pending_is_preserved_when_installed_root_is_unavailable() {
    let f = Fixture::new();
    f.consent(true);
    let store = f.statistics();
    let signer = f.signer();
    let now = UsageClock.now_secs().unwrap();
    let prepared_at = now - RETRY_AFTER_SECS - 1;
    struct ClockAt(u64);
    impl Clock for ClockAt {
        fn now_secs(
            &self,
        ) -> std::result::Result<u64, rustsimhub_usage_stats::protocol::ClockError> {
            Ok(self.0)
        }
    }
    let policy = EnabledPolicy::from_owner_consent(
        OwnerConsent::Granted,
        DisclosureAcknowledgement::Current,
    )
    .unwrap();
    let intent = policy
        .check_in_intent(
            Window::Last24Hours,
            store.identity().public_key(),
            &ClockAt(prepared_at),
        )
        .unwrap()
        .unwrap();
    let digest = [77; 32];
    let mut state = store.load().unwrap();
    state.windows[0].period = Some(intent.renewal_period());
    state.windows[0].pending = Some(PendingTransaction {
        digest,
        prepared_at,
    });
    store.save(&state).unwrap();
    let root = Arc::new(Controller::new(f.auth.clone()));
    let transport = StatisticsTransport::new(root.clone(), signer, store.clone()).unwrap();
    assert!(
        matches!(transport.run_once(Window::Last24Hours).unwrap(),Outcome::Unknown{digest:d,..} if d==digest)
    );
    assert_eq!(
        serde_json::to_value(store.load().unwrap()).unwrap(),
        serde_json::to_value(state).unwrap()
    );
    assert!(root.snapshot().unwrap().is_none());
}
