// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::nkn_statistics::tests::{enabled, Fixture};
use rustsimhub_usage_stats::{
    check_in::{Cancellation, CheckInMachine, CheckInStore, CheckInTransport, Failure, Outcome},
    protocol::{SystemClock, Window},
    query::TransportError,
    store::{
        participation::{Binding, Change, Ledger},
        StoreError, StoredState,
    },
};
use std::{sync::atomic::AtomicUsize, thread};

struct Store(Arc<FileStore>);
impl CheckInStore for Store {
    fn public_key(&self) -> StatsPublicKey {
        self.0.identity().public_key()
    }
    fn load(&self) -> std::result::Result<StoredState, StoreError> {
        self.0.load()
    }
    fn save(&self, s: &StoredState) -> std::result::Result<(), StoreError> {
        self.0.save(s)
    }
}
struct Probe<F> {
    call: F,
    cancel: Arc<OnceLock<Cancellation>>,
}
impl<F: FnMut(&StatisticsLease, &AttemptBudget, &Cancellation)> CheckInTransport for Probe<F> {
    fn acquire_statistics(
        &mut self,
        lease: &StatisticsLease,
        budget: &AttemptBudget,
    ) -> std::result::Result<(), TransportError> {
        (self.call)(lease, budget, self.cancel.get().unwrap());
        Err(TransportError::Unavailable)
    }
    fn release_statistics(&mut self, _: StatisticsLease) {}
    fn prepare(
        &mut self,
        _: &StatisticsLease,
        _: &CheckInIntent,
        _: &AttemptBudget,
    ) -> std::result::Result<[u8; 32], TransportError> {
        Err(TransportError::Unavailable)
    }
    fn submit(
        &mut self,
        _: &StatisticsLease,
        _: &CheckInIntent,
        _: [u8; 32],
        _: &AttemptBudget,
    ) -> std::result::Result<(), TransportError> {
        Err(TransportError::Unavailable)
    }
    fn query(
        &mut self,
        _: &StatisticsLease,
        _: &CheckInIntent,
        _: [u8; 32],
        _: &AttemptBudget,
    ) -> std::result::Result<Vec<u8>, TransportError> {
        Err(TransportError::Unavailable)
    }
}
// Budgets and leases are minted only by the REAL CheckInMachine; no test
// constructor or substitute cancellation/consent proof is added to production.
fn with_budget(
    store: Arc<FileStore>,
    call: impl FnMut(&StatisticsLease, &AttemptBudget, &Cancellation),
) {
    let cancel = Arc::new(OnceLock::new());
    let mut m = CheckInMachine::new(
        Store(store),
        Probe {
            call,
            cancel: cancel.clone(),
        },
        SystemClock,
    );
    assert!(cancel.set(m.cancellation()).is_ok());
    m.set_policy(enabled()).unwrap();
    let outcome = m.advance(Window::Last24Hours).unwrap();
    assert!(matches!(
        outcome,
        Outcome::Failed {
            reason: Failure::Prepare
        } | Outcome::Disabled
    ));
}
fn registration() -> Registration {
    Registration {
        generation: 7,
        boot_deadline: 999999,
        nonce: [1; 32],
        policy: [2; 32],
        pid: 1234,
    }
}
fn reply(reg: Registration, body: &[u8]) -> Vec<u8> {
    let mut raw = vec![0; 87];
    raw[..5].copy_from_slice(b"RSHS\x01");
    raw[5..9].copy_from_slice(&reg.generation.to_be_bytes());
    raw[9..17].copy_from_slice(&reg.boot_deadline.to_be_bytes());
    raw[17..49].copy_from_slice(&reg.nonce);
    raw[49..81].copy_from_slice(&reg.policy);
    raw[81..85].copy_from_slice(&reg.pid.to_be_bytes());
    raw[85] = 1;
    raw[86] = 2;
    raw.extend_from_slice(b"RSTX\x01");
    raw.extend_from_slice(&(body.len() as u32).to_be_bytes());
    raw.extend_from_slice(body);
    raw
}
fn request_from(s: &mut UnixStream) -> serde_json::Value {
    s.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let mut head = [0; 91];
    s.read_exact(&mut head).unwrap();
    assert_eq!(&head[..5], b"RSCN\x01");
    assert_eq!(&head[82..87], b"RSTX\x01");
    let n = u32::from_be_bytes(head[87..91].try_into().unwrap()) as usize;
    assert!((1..=2048).contains(&n));
    let mut raw = vec![0; n];
    s.read_exact(&mut raw).unwrap();
    serde_json::from_slice(&raw).unwrap()
}
fn nonce_reply(request: &serde_json::Value, nonce: u64) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"version":1,"operation_id":request["operation_id"],"participation_generation":request["participation_generation"],"public_key":request["public_key"],"status":"ready","nonce":nonce})).unwrap()
}

#[test]
fn fragmented_original_registration_nonce_and_reply_are_bound() {
    let f = Fixture::new();
    f.consent(true);
    let store = f.statistics();
    let signer = f.signer();
    let key = store.identity().public_key();
    with_budget(store.clone(), |lease, budget, _| {
        let generation = signer.check_current(&store, budget).unwrap();
        let id = uuid::Uuid::from_bytes(*lease.id()).to_string();
        let call = TransactionCall::Preflight;
        let raw = call.request(id.clone(), generation, key).unwrap();
        let reg = registration();
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let child = thread::spawn(move || {
            let request = request_from(&mut server);
            assert_eq!(request.as_object().unwrap().len(), 4);
            let bytes = reply(reg, &nonce_reply(&request, u64::MAX));
            for part in bytes.chunks(7) {
                server.write_all(part).unwrap();
            }
        });
        let fence = || {
            signer
                .check_current(&store, budget)
                .map(|_| ())
                .map_err(signer_error)
        };
        let value = exchange(
            &mut client,
            Wire {
                registration: reg,
                call: &call,
                raw: &raw,
                id: &id,
                generation,
                key,
            },
            IoBudget {
                budget,
                deadline: budget.deadline(),
                fence: &fence,
                signer: &signer,
                store: &store,
                reference: None,
            },
        )
        .unwrap();
        assert_eq!(value, TransactionConclusion::Nonce(u64::MAX));
        child.join().unwrap();
    });
}

#[test]
fn cancelled_original_attempt_interrupts_actual_socket_read() {
    let f = Fixture::new();
    f.consent(true);
    let store = f.statistics();
    let signer = f.signer();
    let key = store.identity().public_key();
    with_budget(store.clone(), |lease, budget, cancel| {
        let generation = signer.check_current(&store, budget).unwrap();
        let id = uuid::Uuid::from_bytes(*lease.id()).to_string();
        let call = TransactionCall::Preflight;
        let raw = call.request(id.clone(), generation, key).unwrap();
        let reg = registration();
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let cancel = cancel.clone();
        let child = thread::spawn(move || {
            let _ = request_from(&mut server);
            cancel.cancel();
            thread::sleep(Duration::from_millis(180));
        });
        let fence = || {
            signer
                .check_current(&store, budget)
                .map(|_| ())
                .map_err(signer_error)
        };
        let at = Instant::now();
        let value = exchange(
            &mut client,
            Wire {
                registration: reg,
                call: &call,
                raw: &raw,
                id: &id,
                generation,
                key,
            },
            IoBudget {
                budget,
                deadline: budget.deadline(),
                fence: &fence,
                signer: &signer,
                store: &store,
                reference: None,
            },
        );
        assert_eq!(value, Err(PortError::Deadline));
        assert!(at.elapsed() < Duration::from_millis(150));
        client.shutdown(Shutdown::Both).unwrap();
        child.join().unwrap();
    });
}

#[test]
fn completed_consent_off_fences_late_reply_without_holding_permission_lock() {
    let f = Fixture::new();
    f.consent(true);
    let store = f.statistics();
    let signer = f.signer();
    let key = store.identity().public_key();
    with_budget(store.clone(), |lease, budget, _| {
        let generation = signer.check_current(&store, budget).unwrap();
        let id = uuid::Uuid::from_bytes(*lease.id()).to_string();
        let call = TransactionCall::Preflight;
        let raw = call.request(id.clone(), generation, key).unwrap();
        let reg = registration();
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let path = f.permission();
        let actor = f.actor();
        let binding = {
            let auth = f.auth.lock().unwrap();
            Binding {
                core_public_key: auth.public_key(),
                realm: auth.durable_realm().unwrap(),
            }
        };
        let completed = Arc::new(AtomicBool::new(false));
        let done = completed.clone();
        let child = thread::spawn(move || {
            let request = request_from(&mut server);
            let until = Instant::now() + Duration::from_millis(300);
            let ledger = loop {
                match Ledger::open_if_present(&path, binding.clone()) {
                    Ok(Some(l)) => break l,
                    Err(_) if Instant::now() < until => thread::sleep(Duration::from_millis(1)),
                    other => panic!("cannot complete OFF: {}", other.is_ok()),
                }
            };
            ledger
                .change(
                    actor,
                    Change {
                        request_id: "22222222-2222-4222-8222-222222222222".into(),
                        core_instance: "c".repeat(64),
                        expected_generation: generation,
                        enabled: false,
                        disclosure_version: None,
                    },
                )
                .unwrap();
            drop(ledger);
            done.store(true, Ordering::Release);
            thread::sleep(Duration::from_millis(120));
            let _ = server.write_all(&reply(reg, &nonce_reply(&request, 0)));
        });
        let fence = || {
            signer
                .check_current(&store, budget)
                .map(|_| ())
                .map_err(signer_error)
        };
        let value = exchange(
            &mut client,
            Wire {
                registration: reg,
                call: &call,
                raw: &raw,
                id: &id,
                generation,
                key,
            },
            IoBudget {
                budget,
                deadline: budget.deadline(),
                fence: &fence,
                signer: &signer,
                store: &store,
                reference: None,
            },
        );
        assert!(value.is_err());
        client.shutdown(Shutdown::Both).unwrap();
        child.join().unwrap();
        assert!(completed.load(Ordering::Acquire));
        assert_eq!(
            signer.check_current(&store, budget),
            Err(SignerError::Denied)
        );
    });
}

#[test]
fn completed_off_between_check_and_dispatch_sends_zero_bytes() {
    let f = Fixture::new();
    f.consent(true);
    let store = f.statistics();
    let signer = f.signer();
    let key = store.identity().public_key();
    with_budget(store.clone(), |lease, budget, _| {
        let generation = signer.check_current(&store, budget).unwrap();
        let id = uuid::Uuid::from_bytes(*lease.id()).to_string();
        let call = TransactionCall::Preflight;
        let raw = call.request(id.clone(), generation, key).unwrap();
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let once = AtomicBool::new(false);
        let fence = || {
            signer.check_current(&store, budget).map_err(signer_error)?;
            if !once.swap(true, Ordering::AcqRel) {
                // Complete real OFF after the ordinary check returned. The
                // nonblocking write guard must independently reject dispatch.
                let binding = {
                    let auth = f.auth.lock().unwrap();
                    Binding {
                        core_public_key: auth.public_key(),
                        realm: auth.durable_realm().unwrap(),
                    }
                };
                let ledger = Ledger::open_if_present(&f.permission(), binding)
                    .unwrap()
                    .unwrap();
                ledger
                    .change(
                        f.actor(),
                        Change {
                            request_id: "33333333-3333-4333-8333-333333333333".into(),
                            core_instance: "c".repeat(64),
                            expected_generation: generation,
                            enabled: false,
                            disclosure_version: None,
                        },
                    )
                    .unwrap();
            }
            Ok(())
        };
        let result = exchange(
            &mut client,
            Wire {
                registration: registration(),
                call: &call,
                raw: &raw,
                id: &id,
                generation,
                key,
            },
            IoBudget {
                budget,
                deadline: budget.deadline(),
                fence: &fence,
                signer: &signer,
                store: &store,
                reference: None,
            },
        );
        assert_eq!(result, Err(PortError::Denied));
        server.set_nonblocking(true).unwrap();
        assert!(
            server
                .read(&mut [0; 1])
                .is_err_and(|e| e.kind() == std::io::ErrorKind::WouldBlock),
            "completed OFF still emitted request bytes"
        );
    });
}

#[test]
fn response_refuses_other_registration_pid_oversize_and_trailing_frame() {
    let f = Fixture::new();
    f.consent(true);
    let store = f.statistics();
    let signer = f.signer();
    let key = store.identity().public_key();
    with_budget(store.clone(), |lease, budget, _| {
        let generation = signer.check_current(&store, budget).unwrap();
        let id = uuid::Uuid::from_bytes(*lease.id()).to_string();
        let call = TransactionCall::Preflight;
        let raw = call.request(id.clone(), generation, key).unwrap();
        let reg = registration();
        for kind in [
            "generation",
            "nonce",
            "policy",
            "deadline",
            "pid",
            "oversize",
            "trailing",
            "wrong-tail",
        ] {
            let (mut client, mut server) = UnixStream::pair().unwrap();
            let child = thread::spawn(move || {
                let req = request_from(&mut server);
                let mut bytes = reply(reg, &nonce_reply(&req, 0));
                match kind {
                    "generation" => bytes[8] ^= 1,
                    "nonce" => bytes[17] ^= 1,
                    "policy" => bytes[49] ^= 1,
                    "deadline" => bytes[16] ^= 1,
                    "pid" => bytes[84] ^= 1,
                    "oversize" => bytes[92..96].copy_from_slice(&2049u32.to_be_bytes()),
                    "trailing" => bytes.push(0),
                    "wrong-tail" => bytes[87] = b'X',
                    _ => unreachable!(),
                };
                let _ = server.write_all(&bytes);
            });
            let fence = || {
                signer
                    .check_current(&store, budget)
                    .map(|_| ())
                    .map_err(signer_error)
            };
            assert!(
                exchange(
                    &mut client,
                    Wire {
                        registration: reg,
                        call: &call,
                        raw: &raw,
                        id: &id,
                        generation,
                        key
                    },
                    IoBudget {
                        budget,
                        deadline: budget.deadline(),
                        fence: &fence,
                        signer: &signer,
                        store: &store,
                        reference: None,
                    }
                )
                .is_err(),
                "{kind}"
            );
            client.shutdown(Shutdown::Both).unwrap();
            child.join().unwrap();
        }
    });
}

#[test]
fn typed_reply_never_confirms_or_accepts_null_extra_duplicate_or_cross_intent() {
    let f = Fixture::new();
    f.consent(true);
    let store = f.statistics();
    let key = store.identity().public_key();
    let id = "11111111-2222-3333-4444-555555555555";
    let generation = 1;
    let call = TransactionCall::Preflight;
    let request =
        serde_json::from_slice(&call.request(id.into(), generation, key).unwrap()).unwrap();
    let valid = nonce_reply(&request, 0);
    assert_eq!(
        call.reply(&valid, id, generation, key).unwrap(),
        TransactionConclusion::Nonce(0)
    );
    for (before, after) in [
        ("\"nonce\":0", "\"nonce\":null"),
        ("\"status\":\"ready\"", "\"status\":\"confirmed\""),
        ("\"version\":1", "\"version\":1,\"version\":1"),
        ("\"version\":1", "\"version\":1,\"url\":\"https://other/\""),
        (
            "\"participation_generation\":1",
            "\"participation_generation\":2",
        ),
    ] {
        let text = String::from_utf8(valid.clone())
            .unwrap()
            .replace(before, after);
        assert!(call.reply(text.as_bytes(), id, generation, key).is_err());
    }
    let intent = enabled()
        .check_in_intent(Window::Last24Hours, key, &SystemClock)
        .unwrap()
        .unwrap();
    let call = TransactionCall::Query {
        intent,
        digest: [3; 32],
    };
    let mut v = serde_json::json!({"version":1,"operation_id":id,"participation_generation":generation,"public_key":key,"status":"unknown","intent":intent,"digest":hex(&[3;32])});
    assert_eq!(
        call.reply(&serde_json::to_vec(&v).unwrap(), id, generation, key)
            .unwrap(),
        TransactionConclusion::Unknown
    );
    v["intent"]["renewal_period"] = serde_json::json!(intent.renewal_period() + 1);
    assert!(call
        .reply(&serde_json::to_vec(&v).unwrap(), id, generation, key)
        .is_err());
}

#[test]
fn default_policy_and_missing_reference_do_not_start_root_or_make_authority() {
    let f = Fixture::new();
    f.consent(true);
    let store = f.statistics();
    let signer = f.signer();
    let controller = Controller::new(f.auth.clone());
    let other = Fixture::new();
    let foreign = Controller::new(other.auth.clone());
    let key = store.identity().public_key();
    let mut p = serde_json::json!({"format":1,"source_commit":"a".repeat(40),"engine_sha256":"b".repeat(64),"core_pin":StatsPublicKey::from_bytes(f.auth.lock().unwrap().public_key()),"statistics_public_key":key,"interface":"usb0","seed_rpc":["https://seed.invalid"]});
    let device = f.auth.lock().unwrap().public_key();
    assert_eq!(
        policy_key(&serde_json::to_vec(&p).unwrap(), device).unwrap(),
        None
    );
    p["usage_transaction_protocol"] = serde_json::json!(1);
    assert!(policy_key(&serde_json::to_vec(&p).unwrap(), device).is_err());
    p["status_protocol"] = serde_json::json!(1);
    assert_eq!(
        policy_key(&serde_json::to_vec(&p).unwrap(), device).unwrap(),
        Some(key)
    );
    p["usage_transaction_protocol"] = serde_json::json!(2);
    assert!(policy_key(&serde_json::to_vec(&p).unwrap(), device).is_err());
    let calls = AtomicUsize::new(0);
    with_budget(store.clone(), |lease, budget, _| {
        calls.fetch_add(1, Ordering::Relaxed);
        assert_eq!(
            controller.statistics_transaction(
                &signer,
                &store,
                lease,
                TransactionCall::Preflight,
                budget
            ),
            Err(PortError::Unavailable)
        );
        assert_eq!(
            foreign.statistics_transaction(
                &signer,
                &store,
                lease,
                TransactionCall::Preflight,
                budget
            ),
            Err(PortError::Denied)
        );
    });
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(controller.snapshot().unwrap().is_none());
    assert!(controller.drive().unwrap().is_none());
}

#[test]
fn pending_nonce_and_original_digest_are_required_before_dispatch() {
    let f = Fixture::new();
    f.consent(true);
    let store = f.statistics();
    let key = store.identity().public_key();
    let intent = enabled()
        .check_in_intent(Window::Last24Hours, key, &SystemClock)
        .unwrap()
        .unwrap();
    let query = TransactionCall::Query {
        intent,
        digest: [3; 32],
    };
    assert_eq!(check_persisted(&store, &TransactionCall::Preflight), Ok(()));
    assert_eq!(check_persisted(&store, &query), Err(PortError::Denied));
    let mut state = store.load().unwrap();
    state.windows[0].period = Some(intent.renewal_period());
    state.windows[0].pending = Some(rustsimhub_usage_stats::store::PendingTransaction {
        digest: [3; 32],
        prepared_at: rustsimhub_usage_stats::protocol::Clock::now_secs(&SystemClock).unwrap(),
    });
    store.save(&state).unwrap();
    assert_eq!(
        check_persisted(&store, &TransactionCall::Preflight),
        Err(PortError::Denied)
    );
    assert_eq!(check_persisted(&store, &query), Ok(()));
    assert_eq!(
        check_persisted(
            &store,
            &TransactionCall::Query {
                intent,
                digest: [4; 32]
            }
        ),
        Err(PortError::Denied)
    );
    let other = enabled()
        .check_in_intent(Window::Last30Days, key, &SystemClock)
        .unwrap()
        .unwrap();
    assert_eq!(
        check_persisted(
            &store,
            &TransactionCall::Query {
                intent: other,
                digest: [3; 32]
            }
        ),
        Err(PortError::Denied)
    );
}

#[test]
fn submit_and_query_codec_use_exact_signed_and_persisted_intent() {
    let f = Fixture::new();
    f.consent(true);
    let store = f.statistics();
    let signer = f.signer();
    let key = store.identity().public_key();
    with_budget(store.clone(), |lease, budget, _| {
        let generation = signer.check_current(&store, budget).unwrap();
        let id = uuid::Uuid::from_bytes(*lease.id()).to_string();
        let intent = enabled()
            .check_in_intent(Window::Last24Hours, key, &SystemClock)
            .unwrap()
            .unwrap();
        let prepared = signer.prepare(&store, intent, 0, budget).unwrap();
        assert_eq!(
            check_persisted(&store, &TransactionCall::Submit(&prepared)),
            Err(PortError::Denied)
        );
        let mut state = store.load().unwrap();
        state.windows[0].period = Some(intent.renewal_period());
        state.windows[0].pending = Some(rustsimhub_usage_stats::store::PendingTransaction {
            digest: prepared.digest(),
            prepared_at: rustsimhub_usage_stats::protocol::Clock::now_secs(&SystemClock).unwrap(),
        });
        store.save(&state).unwrap();
        for call in [
            TransactionCall::Submit(&prepared),
            TransactionCall::Query {
                intent,
                digest: prepared.digest(),
            },
        ] {
            let raw = call.request(id.clone(), generation, key).unwrap();
            let reg = registration();
            let is_submit = matches!(call, TransactionCall::Submit(_));
            let expected_transaction = hex(prepared.transaction_bytes());
            let expected_digest = hex(&prepared.digest());
            let (mut client, mut server) = UnixStream::pair().unwrap();
            let child = thread::spawn(move || {
                let request = request_from(&mut server);
                assert_eq!(request["digest"], expected_digest);
                assert_eq!(request["intent"], serde_json::to_value(intent).unwrap());
                if is_submit {
                    assert_eq!(request.as_object().unwrap().len(), 7);
                    assert_eq!(request["transaction"], expected_transaction);
                } else {
                    assert_eq!(request.as_object().unwrap().len(), 6);
                    assert!(request.get("transaction").is_none());
                }
                let body=serde_json::to_vec(&serde_json::json!({"version":1,"operation_id":request["operation_id"],"participation_generation":request["participation_generation"],"public_key":request["public_key"],"status":if is_submit{"accepted"}else{"unknown"},"intent":request["intent"],"digest":request["digest"]})).unwrap();
                server.write_all(&reply(reg, &body)).unwrap();
            });
            let fence = || {
                signer
                    .check_current(&store, budget)
                    .map(|_| ())
                    .map_err(signer_error)
            };
            let result = exchange(
                &mut client,
                Wire {
                    registration: reg,
                    call: &call,
                    raw: &raw,
                    id: &id,
                    generation,
                    key,
                },
                IoBudget {
                    budget,
                    deadline: budget.deadline(),
                    fence: &fence,
                    signer: &signer,
                    store: &store,
                    reference: None,
                },
            )
            .unwrap();
            assert_eq!(
                result,
                if is_submit {
                    TransactionConclusion::Accepted
                } else {
                    TransactionConclusion::Unknown
                }
            );
            child.join().unwrap();
        }
        assert_eq!(
            store.load().unwrap().windows[0]
                .pending
                .as_ref()
                .unwrap()
                .digest,
            prepared.digest()
        );
    });
}
