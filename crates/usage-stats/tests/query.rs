use rustsimhub_usage_stats::identity::StatisticsIdentity;
use rustsimhub_usage_stats::protocol::{
    Clock, ClockError, DisclosureAcknowledgement, EnabledPolicy, OwnerConsent, Window,
};
use rustsimhub_usage_stats::query::*;
use rustsimhub_usage_stats::store::FileStore;
use serde_json::{json, Value};
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone)]
struct TestClock {
    now: Rc<Cell<u64>>,
    reads: Rc<Cell<u32>>,
    failed: Rc<Cell<bool>>,
}
impl TestClock {
    fn new(now: u64) -> Self {
        Self {
            now: Rc::new(Cell::new(now)),
            reads: Rc::new(Cell::new(0)),
            failed: Rc::new(Cell::new(false)),
        }
    }
}
impl Clock for TestClock {
    fn now_secs(&self) -> Result<u64, ClockError> {
        self.reads.set(self.reads.get() + 1);
        if self.failed.get() {
            Err(ClockError::BeforeEpoch)
        } else {
            Ok(self.now.get())
        }
    }
}
type Handler = Box<dyn FnMut(&QueryRequest) -> Result<Vec<u8>, TransportError>>;
struct LocalTransport {
    calls: Rc<Cell<u32>>,
    handler: Handler,
}
impl QueryTransport for LocalTransport {
    fn query_page(&mut self, request: &QueryRequest) -> Result<Vec<u8>, TransportError> {
        self.calls.set(self.calls.get() + 1);
        (self.handler)(request)
    }
}
fn client(
    handler: impl FnMut(&QueryRequest) -> Result<Vec<u8>, TransportError> + 'static,
    clock: TestClock,
    cache: QueryCache,
) -> QueryClient<LocalTransport, TestClock> {
    QueryClient::new(
        LocalTransport {
            calls: Rc::new(Cell::new(0)),
            handler: Box::new(handler),
        },
        clock,
        cache,
    )
    .unwrap()
}
fn enabled() -> EnabledPolicy {
    EnabledPolicy::from_owner_consent(OwnerConsent::Granted, DisclosureAcknowledgement::Current)
        .unwrap()
}
fn page(request: &QueryRequest, count: u64, pages: u16) -> Value {
    json!({"version":request.version(),"window":request.window(),"topic":request.topic(),
        "page":request.page(),"pages":pages,"count":count,"observed_at_secs":1000,"expires_at_secs":1600})
}
fn encode(value: Value) -> Vec<u8> {
    serde_json::to_vec(&value).unwrap()
}
fn unknown(result: QueryOutcome, expected: QueryError) {
    assert_eq!(result, QueryOutcome::Unknown { reason: expected });
}
fn primed(clock: TestClock) -> QueryCache {
    let mut c = client(|r| Ok(encode(page(r, 42, 1))), clock, QueryCache::default());
    assert!(c
        .query_all(&enabled())
        .iter()
        .all(|o| matches!(o, QueryOutcome::Success { .. })));
    c.into_parts().2
}

#[test]
fn off_calls_neither_transport_nor_clock_and_creates_no_store() {
    let temp = tempfile::tempdir().unwrap();
    let clock = TestClock::new(1000);
    let mut c = client(
        |_| panic!("OFF invoked transport"),
        clock.clone(),
        QueryCache::default(),
    );
    assert_eq!(
        c.query_all(&EnabledPolicy::default()),
        std::array::from_fn(|_| QueryOutcome::Disabled)
    );
    assert_eq!(clock.reads.get(), 0);
    assert_eq!(c.into_parts().0.calls.get(), 0);
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn true_zero_and_partial_failure_remain_distinct_in_all_windows() {
    let mut c = client(
        |r| match r.window() {
            Window::Last24Hours => Ok(encode(page(r, 0, 1))),
            Window::Last30Days => Err(TransportError::Offline),
            Window::Last90Days => Err(TransportError::Timeout),
        },
        TestClock::new(1000),
        QueryCache::default(),
    );
    let outcomes = c.query_all(&enabled());
    assert!(matches!(&outcomes[0], QueryOutcome::Success { entry } if entry.count() == 0));
    assert_eq!(
        outcomes[1],
        QueryOutcome::Unknown {
            reason: QueryError::Offline
        }
    );
    assert_eq!(
        outcomes[2],
        QueryOutcome::Unknown {
            reason: QueryError::Timeout
        }
    );
    assert_eq!(c.into_parts().0.calls.get(), 3);
}

#[test]
fn all_transport_error_categories_are_preserved() {
    for (error, expected) in [
        (TransportError::Offline, QueryError::Offline),
        (TransportError::Timeout, QueryError::Timeout),
        (TransportError::Rejected, QueryError::Rejected),
        (TransportError::Unavailable, QueryError::Unavailable),
    ] {
        let mut c = client(
            move |_| Err(error),
            TestClock::new(1000),
            QueryCache::default(),
        );
        for window in Window::ALL {
            unknown(c.query_window(&enabled(), window), expected);
        }
    }
}

#[test]
fn offline_cache_retains_source_original_success_and_expiry() {
    let clock = TestClock::new(1000);
    let cache = primed(clock.clone());
    clock.now.set(1010);
    let mut c = client(|_| Err(TransportError::Offline), clock.clone(), cache);
    for window in Window::ALL {
        match c.query_window(&enabled(), window) {
            QueryOutcome::Cached {
                entry,
                freshness: Freshness::WithinTtl,
                reason: QueryError::Offline,
            } => {
                assert_eq!(entry.window(), window);
                assert_eq!(entry.count(), 42);
                assert_eq!(entry.source(), QuerySource::ReadOnlyTransport);
                assert_eq!(entry.observed_at_secs(), 1000);
                assert_eq!(entry.succeeded_at_secs(), 1000);
                assert_eq!(entry.expires_at_secs(), 1600);
            }
            other => panic!("{other:?}"),
        }
    }
    clock.now.set(1600);
    for outcome in c.query_all(&enabled()) {
        assert!(matches!(
            outcome,
            QueryOutcome::Cached {
                freshness: Freshness::Expired,
                ..
            }
        ));
    }
}

#[test]
fn repeated_explicit_queries_are_throttled_per_window() {
    let clock = TestClock::new(1000);
    let mut c = client(
        |r| Ok(encode(page(r, 7, 1))),
        clock.clone(),
        QueryCache::default(),
    );
    c.query_all(&enabled());
    for result in c.query_all(&enabled()) {
        assert!(matches!(
            result,
            QueryOutcome::Cached {
                reason: QueryError::Throttled,
                ..
            }
        ));
    }
    clock.now.set(1005);
    c.query_window(&enabled(), Window::Last30Days);
    assert_eq!(c.into_parts().0.calls.get(), 4);
}

#[test]
fn finite_pages_are_summed_once_without_trusting_urls() {
    let mut c = client(
        |r| {
            assert_eq!(r.max_bytes(), MAX_PAGE_BYTES);
            assert_eq!(r.timeout_secs(), 10);
            Ok(encode(page(r, u64::from(r.page()) + 1, MAX_PAGES)))
        },
        TestClock::new(1000),
        QueryCache::default(),
    );
    assert!(
        matches!(c.query_window(&enabled(), Window::Last24Hours), QueryOutcome::Success { entry } if entry.count() == 136)
    );
    assert_eq!(c.into_parts().0.calls.get(), u32::from(MAX_PAGES));
}

#[test]
fn malformed_duplicate_fields_and_unknown_commands_are_rejected() {
    for bytes in [
        b"{".to_vec(),
        vec![0xff],
        b"[]".to_vec(),
        b"null".to_vec(),
        b"{\"version\":1,\"version\":1}".to_vec(),
    ] {
        let mut c = client(
            move |_| Ok(bytes.clone()),
            TestClock::new(1000),
            QueryCache::default(),
        );
        unknown(
            c.query_window(&enabled(), Window::Last24Hours),
            QueryError::InvalidResponse,
        );
    }
    for field in ["url", "command", "seed", "imsi"] {
        let mut c = client(
            move |r| {
                let mut v = page(r, 1, 1);
                v[field] = json!("private-input");
                Ok(encode(v))
            },
            TestClock::new(1000),
            QueryCache::default(),
        );
        let result = c.query_window(&enabled(), Window::Last24Hours);
        assert!(!format!("{result:?}").contains("private-input"));
        assert!(!serde_json::to_string(&result)
            .unwrap()
            .contains("private-input"));
        unknown(result, QueryError::InvalidResponse);
    }
}

#[test]
fn schema_rejections_cover_every_window() {
    for window in Window::ALL {
        for (field, value, error) in [
            ("version", json!(2), QueryError::WrongVersion),
            ("window", json!("7d"), QueryError::InvalidResponse),
            (
                "topic",
                json!("nknguard-usage-v1-24h"),
                QueryError::WrongTopic,
            ),
            (
                "window",
                serde_json::to_value(Window::ALL[(window.index() + 1) % 3]).unwrap(),
                QueryError::WrongWindow,
            ),
            ("pages", json!(0), QueryError::InvalidPage),
            ("pages", json!(MAX_PAGES + 1), QueryError::InvalidPage),
            ("page", json!(1), QueryError::InvalidPage),
            ("count", json!(-1), QueryError::InvalidResponse),
            ("count", json!(1.5), QueryError::InvalidResponse),
            ("observed_at_secs", json!(1001), QueryError::InvalidTime),
            ("expires_at_secs", json!(1601), QueryError::InvalidTime),
            ("expires_at_secs", json!(1000), QueryError::InvalidTime),
        ] {
            let mut c = client(
                move |r| {
                    let mut v = page(r, 1, 1);
                    v[field] = value.clone();
                    Ok(encode(v))
                },
                TestClock::new(1000),
                QueryCache::default(),
            );
            unknown(c.query_window(&enabled(), window), error);
        }
    }
}

#[test]
fn oversized_and_already_expired_responses_are_rejected() {
    let mut c = client(
        |_| Ok(vec![b' '; MAX_PAGE_BYTES + 1]),
        TestClock::new(1000),
        QueryCache::default(),
    );
    unknown(
        c.query_window(&enabled(), Window::Last24Hours),
        QueryError::Oversized,
    );
    let mut c = client(
        |r| Ok(encode(page(r, 1, 1))),
        TestClock::new(1600),
        QueryCache::default(),
    );
    unknown(
        c.query_window(&enabled(), Window::Last24Hours),
        QueryError::ExpiredResponse,
    );
}

#[test]
fn duplicate_pages_and_changed_snapshot_never_produce_partial_success() {
    for changed in ["page", "pages", "observed_at_secs", "expires_at_secs"] {
        let mut c = client(
            move |r| {
                let mut v = page(r, 3, 2);
                v["expires_at_secs"] = json!(1599);
                if r.page() == 1 {
                    v[changed] = match changed {
                        "page" => json!(0),
                        "pages" => json!(3),
                        "observed_at_secs" => json!(999),
                        _ => json!(1598),
                    };
                }
                Ok(encode(v))
            },
            TestClock::new(1000),
            QueryCache::default(),
        );
        unknown(
            c.query_window(&enabled(), Window::Last24Hours),
            if changed == "page" {
                QueryError::InvalidPage
            } else {
                QueryError::InconsistentPages
            },
        );
        assert!(c.cache().entry(Window::Last24Hours).is_none());
    }
}

#[test]
fn integer_overflow_and_product_count_limit_are_checked() {
    for (counts, expected) in [
        ([u64::MAX, 1], QueryError::CountOverflow),
        ([MAX_COUNT, 1], QueryError::CountOutOfRange),
    ] {
        let mut c = client(
            move |r| Ok(encode(page(r, counts[r.page() as usize], 2))),
            TestClock::new(1000),
            QueryCache::default(),
        );
        unknown(c.query_window(&enabled(), Window::Last24Hours), expected);
    }
    let mut c = client(
        |r| {
            let bytes = String::from_utf8(encode(page(r, 1, 1))).unwrap();
            Ok(bytes
                .replace("\"count\":1", "\"count\":18446744073709551616")
                .into_bytes())
        },
        TestClock::new(1000),
        QueryCache::default(),
    );
    unknown(
        c.query_window(&enabled(), Window::Last24Hours),
        QueryError::InvalidResponse,
    );
}

#[test]
fn deadline_is_total_across_pages_and_checked_after_adapter_returns() {
    let clock = TestClock::new(1000);
    let advancing = clock.clone();
    let mut c = client(
        move |r| {
            assert_eq!(r.timeout_secs(), if r.page() == 0 { 10 } else { 4 });
            advancing.now.set(advancing.now.get() + 6);
            Ok(encode(page(r, 3, 2)))
        },
        clock,
        QueryCache::default(),
    );
    unknown(
        c.query_window(&enabled(), Window::Last24Hours),
        QueryError::Timeout,
    );
    assert_eq!(c.into_parts().0.calls.get(), 2);
}

#[test]
fn frozen_wall_clock_does_not_refresh_the_monotonic_budget() {
    let mut first_deadline = None;
    let mut c = client(
        move |r| {
            if let Some(first) = first_deadline {
                assert_eq!(r.deadline(), first);
            } else {
                first_deadline = Some(r.deadline());
            }
            Ok(encode(page(r, 1, MAX_PAGES)))
        },
        TestClock::new(1000),
        QueryCache::default(),
    );
    assert!(matches!(
        c.query_window(&enabled(), Window::Last24Hours),
        QueryOutcome::Success { .. }
    ));
    assert_eq!(c.into_parts().0.calls.get(), u32::from(MAX_PAGES));
}

#[test]
fn late_return_is_timeout_even_when_injected_wall_clock_is_frozen() {
    let mut c = client(
        |r| {
            // Exercise the real monotonic timeout on a local adapter; no network.
            std::thread::sleep(
                r.deadline()
                    .saturating_duration_since(std::time::Instant::now()),
            );
            Ok(encode(page(r, 0, 1)))
        },
        TestClock::new(1000),
        QueryCache::default(),
    );
    unknown(
        c.query_window(&enabled(), Window::Last24Hours),
        QueryError::Timeout,
    );
}

#[test]
fn clock_failure_never_invents_zero_or_freshness() {
    let clock = TestClock::new(1000);
    clock.failed.set(true);
    let mut c = client(
        |_| panic!("clock failure invoked transport"),
        clock,
        QueryCache::default(),
    );
    unknown(
        c.query_window(&enabled(), Window::Last24Hours),
        QueryError::ClockUnavailable,
    );
}

#[test]
fn rollback_during_response_is_rejected_and_revokes_old_cache() {
    let clock = TestClock::new(1000);
    let cache = primed(clock.clone());
    clock.now.set(1010);
    let changing = clock.clone();
    let mut c = client(
        move |r| {
            changing.now.set(999);
            Ok(encode(page(r, 1, 1)))
        },
        clock,
        cache,
    );
    assert!(matches!(
        c.query_window(&enabled(), Window::Last24Hours),
        QueryOutcome::Cached {
            reason: QueryError::ClockRollback,
            freshness: Freshness::ClockRollback,
            ..
        }
    ));
}

#[test]
fn actual_store_reopen_preserves_cache_and_irreversible_rollback_revocation() {
    let temp = tempfile::tempdir().unwrap();
    let identity = StatisticsIdentity::from_seed([19; 32]);
    let key = identity.public_key();
    let path = temp.path().join("stats");
    let store = FileStore::create(&path, identity).unwrap();
    let clock = TestClock::new(1000);
    let mut state = store.load().unwrap();
    state.cache = primed(clock.clone());
    store.save(&state).unwrap();
    drop(store);
    clock.now.set(900);
    let store = FileStore::open(&path, key).unwrap();
    let mut state = store.load().unwrap();
    let mut c = client(
        |_| panic!("rollback invoked transport"),
        clock.clone(),
        state.cache,
    );
    for result in c.query_all(&enabled()) {
        assert!(matches!(
            result,
            QueryOutcome::Cached {
                freshness: Freshness::ClockRollback,
                reason: QueryError::ClockRollback,
                ..
            }
        ));
    }
    state.cache = c.into_parts().2;
    store.save(&state).unwrap();
    drop(store);
    clock.now.set(1100);
    let store = FileStore::open(&path, key).unwrap();
    let mut c = client(
        |_| Err(TransportError::Offline),
        clock,
        store.load().unwrap().cache,
    );
    for result in c.query_all(&enabled()) {
        assert!(
            matches!(result, QueryOutcome::Cached { entry, freshness: Freshness::ClockRollback, reason: QueryError::Offline } if entry.count() == 42 && entry.succeeded_at_secs() == 1000)
        );
    }
}

#[test]
fn persisted_cache_validation_rejects_wrong_window_and_out_of_range_count() {
    let cache = primed(TestClock::new(1000));
    for (field, value) in [
        ("window", json!("30d")),
        ("count", json!(MAX_COUNT + 1)),
        ("expires_at_secs", json!(u64::MAX)),
    ] {
        let mut v = serde_json::to_value(&cache).unwrap();
        v["entries"][0][field] = value;
        let forged: QueryCache = serde_json::from_value(v).unwrap();
        assert!(matches!(
            QueryClient::new(
                LocalTransport {
                    calls: Rc::new(Cell::new(0)),
                    handler: Box::new(|_| panic!())
                },
                TestClock::new(1000),
                forged
            ),
            Err(QueryError::InvalidCache)
        ));
    }
}
