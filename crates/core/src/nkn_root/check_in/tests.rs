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

#[test]
#[ignore = "explicit disposable private-cgroup/UID fixture with actual fixed SDK only"]
fn statistics_transport_kernel_sdk() {
    use rustsimhub_usage_stats::check_in::CheckInMachine;
    use std::{fs, os::unix::fs::OpenOptionsExt};
    assert_eq!(std::env::var("RSH_STATS_KERNEL_FIXTURE").unwrap(), "1");
    let roles = InstalledRoles::read().unwrap();
    assert_eq!(rustix::process::geteuid().as_raw(), roles.core);
    let scenario = std::env::var("RSH_STATS_SCENARIO").unwrap();
    assert!([
        "startup",
        "shared",
        "lost_ack",
        "off",
        "cancel",
        "off_before",
        "cancel_before",
        "expired_prepare",
        "late_reply",
        "root_revoke",
        "save_failure",
        "save_full",
        "start_failure",
        "bootstrap_failure",
    ]
    .contains(&scenario.as_str()));
    let shared_run = ![
        "startup",
        "root_revoke",
        "start_failure",
        "bootstrap_failure",
    ]
    .contains(&scenario.as_str());
    let f = Fixture::new();
    f.consent(true);
    let store = if scenario == "save_full" {
        Arc::new(
            FileStore::create(
                "/tmp/statistics-disk/statistics",
                rustsimhub_usage_stats::identity::StatisticsIdentity::from_seed([41; 32]),
            )
            .unwrap(),
        )
    } else {
        f.statistics()
    };
    let owner = f.signer().owner_token().to_owned();
    let public = serde_json::json!({"core_pin":f.auth.lock().unwrap().public_key().iter().map(|b|format!("{b:02x}")).collect::<String>(),"statistics_public_key":store.identity().public_key()});
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open("/tmp/statistics-public.json")
        .unwrap();
    file.write_all(&serde_json::to_vec(&public).unwrap())
        .unwrap();
    file.sync_all().unwrap();
    drop(file);
    let wait = |path: &str, seconds: u64| {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        while !std::path::Path::new(path).exists() {
            assert!(
                Instant::now() < deadline,
                "fixture handshake timeout: {path}"
            );
            thread::sleep(Duration::from_millis(5));
        }
    };
    wait("/tmp/statistics-fixture-ready", 10);
    let root = Arc::new(Controller::new(f.auth.clone()));
    if shared_run {
        root.enable(&owner, Owner::ManualEngine).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while root.drive().unwrap().unwrap().phase != rustsimhub_nkn_agent::Phase::Ready {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(20));
        }
        root.enable(&owner, Owner::Remote).unwrap();
        root.enable(&owner, Owner::Statistics).unwrap();
        assert_eq!(root.snapshot().unwrap().unwrap().references, 3);
    }
    let transport = StatisticsTransport::new(root.clone(), f.signer(), store.clone()).unwrap();
    let mut machine =
        CheckInMachine::new(SharedStatisticsStore(store.clone()), transport, UsageClock);
    if ["expired_prepare", "late_reply"].contains(&scenario.as_str()) {
        machine = machine.with_budget(Duration::from_secs(3)).unwrap();
    }
    machine
        .set_policy(crate::nkn_statistics::tests::enabled())
        .unwrap();
    let before_prepare = serde_json::to_value(store.load().unwrap()).unwrap();
    if ["save_failure", "save_full"].contains(&scenario.as_str()) {
        fs::write("/tmp/statistics-write-limit-request", b"1").unwrap();
        wait("/tmp/statistics-write-limit-active", 3);
    }
    let preparing = machine.advance(Window::Last24Hours);
    if ["save_failure", "save_full"].contains(&scenario.as_str()) {
        // Unlink requires no file write under RLIMIT_FSIZE=0. The fixture then
        // restores only this test process's original soft/hard limit.
        fs::remove_file("/tmp/statistics-write-limit-request").unwrap();
        wait("/tmp/statistics-write-limit-restored", 3);
        assert!(
            matches!(
                preparing,
                Err(rustsimhub_usage_stats::check_in::Error::Store(
                    StoreError::Io(_)
                ))
            ),
            "{preparing:?}"
        );
    }
    if [
        "save_failure",
        "save_full",
        "start_failure",
        "bootstrap_failure",
    ]
    .contains(&scenario.as_str())
    {
        if ["start_failure", "bootstrap_failure"].contains(&scenario.as_str()) {
            assert!(
                matches!(
                    preparing,
                    Ok(Outcome::Failed {
                        reason: Failure::Prepare | Failure::Deadline
                    })
                ),
                "{preparing:?}"
            );
        }
        drop(machine);
        assert_eq!(
            serde_json::to_value(store.load().unwrap()).unwrap(),
            before_prepare
        );
        let rows = fs::read_to_string("/tmp/statistics-rpc.jsonl").unwrap_or_default();
        assert_eq!(
            rows.lines()
                .filter(|x| x.contains("sendrawtransaction"))
                .count(),
            0
        );
        assert_eq!(
            rows.lines()
                .filter(|x| x.contains("getnoncebyaddr"))
                .count(),
            usize::from(["save_failure", "save_full"].contains(&scenario.as_str()))
        );
        if shared_run {
            assert_eq!(root.snapshot().unwrap().unwrap().references, 3);
            assert_eq!(
                root.drive().unwrap().unwrap().phase,
                rustsimhub_nkn_agent::Phase::Ready
            );
            root.disable(&owner, Owner::Statistics).unwrap();
            root.disable(&owner, Owner::Remote).unwrap();
            root.disable(&owner, Owner::ManualEngine).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(7);
        while root
            .drive()
            .unwrap()
            .is_some_and(|s| s.phase != rustsimhub_nkn_agent::Phase::Disabled)
        {
            assert!(
                Instant::now() < deadline,
                "failed attempt scope did not clean up"
            );
            thread::sleep(Duration::from_millis(20));
        }
        let result = serde_json::json!({"scenario":scenario,"prepare_result":format!("{preparing:?}"),"prepare_failed":true,"pending_preserved":true,"pending_existed":false,"durable_state_unchanged":true,"original_scope_cleaned":true,"shared_reference_checks":shared_run});
        fs::write(
            "/tmp/statistics-result.json",
            serde_json::to_vec(&result).unwrap(),
        )
        .unwrap();
        return;
    }
    let prepared = preparing.unwrap();
    let Outcome::Prepared { digest } = prepared else {
        panic!("real prepare failed: {prepared:?}")
    };
    let pending = store.load().unwrap().windows[0].pending.clone().unwrap();
    assert_eq!(pending.digest, digest);
    if !shared_run {
        assert!(
            !root.owners().unwrap().statistics,
            "reporting lease must not pretend to enable aggregate query"
        );
        assert!(
            !root.read_status(&owner).unwrap().owners.statistics,
            "status report must describe the explicit query owner"
        );
    }
    fn records() -> Vec<serde_json::Value> {
        std::fs::read_to_string("/tmp/statistics-rpc.jsonl")
            .unwrap()
            .lines()
            .map(|x| serde_json::from_str(x).unwrap())
            .collect()
    }
    assert_eq!(
        records()
            .iter()
            .filter(|r| r["method"] == "getnoncebyaddr")
            .count(),
        1
    );
    assert_eq!(
        records()
            .iter()
            .filter(|r| r["method"] == "sendrawtransaction")
            .count(),
        0,
        "prepare emitted a network submission"
    );
    let pid = root
        .run
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .source
        .shared
        .observed_pid
        .load(Ordering::Acquire);
    assert!(pid > 1);
    assert!(std::path::Path::new(&format!("/proc/{pid}")).exists());
    if scenario == "shared" {
        root.disable(&owner, Owner::Statistics).unwrap();
        assert!(
            !root.owners().unwrap().statistics,
            "query OFF must be visible while reporter remains live"
        );
        root.enable(&owner, Owner::Statistics).unwrap();
        assert!(root.owners().unwrap().statistics);
        root.disable(&owner, Owner::Statistics).unwrap();
        assert_eq!(
            root.snapshot().unwrap().unwrap().references,
            3,
            "query OFF stole reporting reference"
        );
        assert!(root
            .read_usage_count(
                &owner,
                Window::Last24Hours,
                Instant::now() + Duration::from_secs(1)
            )
            .is_err());
    }
    let cancellation = machine.cancellation();
    if scenario == "off_before" {
        off(&f);
    } else if scenario == "cancel_before" {
        cancellation.cancel();
    } else if scenario == "expired_prepare" {
        thread::sleep(Duration::from_millis(3100));
    }
    let submitted = thread::scope(|scope| {
        let interrupter = if ["off", "cancel", "root_revoke"].contains(&scenario.as_str()) {
            Some(scope.spawn(|| {
                wait("/tmp/statistics-submit-entered", 5);
                if scenario == "off" {
                    off(&f);
                } else if scenario == "cancel" {
                    cancellation.cancel();
                } else {
                    root.run
                        .lock()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .source
                        .shared
                        .authority
                        .invalidate();
                }
                fs::write("/tmp/statistics-interruption-complete", b"1").unwrap();
            }))
        } else {
            None
        };
        let result = machine.advance(Window::Last24Hours).unwrap();
        if let Some(child) = interrupter {
            child.join().unwrap();
        }
        result
    });
    match scenario.as_str() {
        "startup" | "shared" => assert!(
            matches!(submitted,Outcome::Pending{digest:d} if d==digest),
            "{submitted:?}"
        ),
        "lost_ack" | "off" | "off_before" | "root_revoke" | "late_reply" => assert!(
            matches!(submitted,Outcome::Unknown{digest:d,..} if d==digest),
            "{submitted:?}"
        ),
        "cancel" | "cancel_before" => assert_eq!(submitted, Outcome::Disabled),
        "expired_prepare" => assert!(matches!(submitted, Outcome::Due { .. }), "{submitted:?}"),
        _ => unreachable!(),
    }
    drop(machine);
    assert_eq!(
        store.load().unwrap().windows[0]
            .pending
            .as_ref()
            .unwrap()
            .digest,
        digest
    );
    if shared_run {
        assert_eq!(
            root.snapshot().unwrap().unwrap().references,
            if scenario == "shared" { 2 } else { 3 }
        );
        assert_eq!(
            root.drive().unwrap().unwrap().phase,
            rustsimhub_nkn_agent::Phase::Ready
        );
        assert!(
            std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "reporter stopped shared SDK"
        );
    }
    if scenario == "shared" || scenario == "lost_ack" {
        // Test-only logical wall-clock advance reaches the original 300s query
        // cooldown without changing Auth/Root/AttemptBudget's real Instant TTL.
        struct QueryClock(Arc<AtomicU64>);
        impl Clock for QueryClock {
            fn now_secs(
                &self,
            ) -> std::result::Result<u64, rustsimhub_usage_stats::protocol::ClockError>
            {
                Ok(self.0.load(Ordering::Acquire))
            }
        }
        let before = serde_json::to_value(store.load().unwrap()).unwrap();
        let transport = StatisticsTransport::new(root.clone(), f.signer(), store.clone()).unwrap();
        let logical_clock = Arc::new(AtomicU64::new(pending.prepared_at + RETRY_AFTER_SECS + 1));
        let mut restarted = CheckInMachine::new(
            SharedStatisticsStore(store.clone()),
            transport,
            QueryClock(logical_clock.clone()),
        );
        restarted
            .set_policy(crate::nkn_statistics::tests::enabled())
            .unwrap();
        for attempt in 0..2 {
            let result = restarted.advance(Window::Last24Hours).unwrap();
            assert!(matches!(result,Outcome::Unknown{digest:d,..} if d==digest));
            let requests = records()
                .iter()
                .filter(|r| r["method"] == "gettransaction")
                .count();
            eprintln!(
                "original query attempt {}: {:?}, observed node requests {}",
                attempt + 1,
                result,
                requests
            );
            if requests > 0 {
                break;
            }
            if attempt == 0 {
                // Unavailable may occur before Root/SDK dispatch. Exercise the
                // ORIGINAL Machine's later read-only query, with the SAME digest,
                // original cancellation and real Auth/Root clocks. No submit or
                // preflight retry is added. Respect Root's actual 5s cooldown.
                logical_clock.fetch_add(RETRY_AFTER_SECS + 1, Ordering::AcqRel);
                thread::sleep(Duration::from_millis(5100));
            }
        }
        drop(restarted);
        assert_eq!(serde_json::to_value(store.load().unwrap()).unwrap(), before);
        assert_eq!(
            records()
                .iter()
                .filter(|r| r["method"] == "gettransaction")
                .count(),
            1
        );
    }
    let rows = records();
    assert_eq!(
        rows.iter()
            .filter(|r| r["method"] == "getnoncebyaddr")
            .count(),
        1
    );
    let expected_submits = usize::from(
        !["off_before", "cancel_before", "expired_prepare"].contains(&scenario.as_str()),
    );
    assert_eq!(
        rows.iter()
            .filter(|r| r["method"] == "sendrawtransaction")
            .count(),
        expected_submits
    );
    if expected_submits != 0 {
        let send = rows
            .iter()
            .find(|r| r["method"] == "sendrawtransaction")
            .unwrap();
        assert_eq!(
            send["digest"],
            digest
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
    if shared_run {
        if scenario != "shared" {
            root.disable(&owner, Owner::Statistics).unwrap();
        }
        assert_eq!(root.snapshot().unwrap().unwrap().references, 2);
        let remote = root.source().unwrap();
        root.disable(&owner, Owner::Remote).unwrap();
        assert!(!remote.reference.as_ref().unwrap().load(Ordering::Acquire));
        assert_eq!(root.drive().unwrap().unwrap().references, 1);
        assert!(std::path::Path::new(&format!("/proc/{pid}")).exists());
        root.disable(&owner, Owner::ManualEngine).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(7);
    while root.drive().unwrap().unwrap().phase != rustsimhub_nkn_agent::Phase::Disabled {
        assert!(Instant::now() < deadline, "original scope cleanup failed");
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "original SDK was not reaped"
    );
    let result = serde_json::json!({"scenario":scenario,"prepared_digest":digest,"submit_outcome":submitted,"observed_submits":expected_submits,"pending_preserved":true,"original_sdk_reaped":true,"original_scope_cleaned":true,"shared_reference_checks":shared_run,"restart_query":scenario=="shared"||scenario=="lost_ack","query_clock":"test-only logical cooldown; Auth/Root/budget Instant clocks unchanged"});
    fs::write(
        "/tmp/statistics-result.json",
        serde_json::to_vec(&result).unwrap(),
    )
    .unwrap();
}
