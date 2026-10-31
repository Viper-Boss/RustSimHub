// SPDX-License-Identifier: AGPL-3.0-only
// Real supervisor under test; only Root/engine/clock boundaries are synthetic.
use super::*;
use std::{collections::HashSet, sync::Mutex};

#[derive(Clone)]
struct TestClock {
    now: Arc<Mutex<Instant>>,
    calls: Arc<AtomicU64>,
}
impl Clock for TestClock {
    fn now(&self) -> Instant {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self.now.lock().unwrap()
    }
}
impl TestClock {
    fn add(&self, seconds: u64) {
        let mut t = self.now.lock().unwrap();
        *t = t.checked_add(Duration::from_secs(seconds)).unwrap();
    }
}
struct Policy;
struct Proof {
    operation: u64,
    expiry: Instant,
}
#[derive(Clone, Copy)]
enum Start {
    Started,
    Partial,
    Rejected,
    Unknown,
}
struct Process(u64);
struct Script {
    calls: Vec<(&'static str, u64, u64)>,
    root_start: u8,
    root_status: Option<Result<RootState, PortError>>,
    root_stop: Result<RootState, PortError>,
    engine_start: Start,
    engine_rejection: PortError,
    engine_status: Option<Result<EngineState, PortError>>,
    engine_stop: Result<EngineState, PortError>,
    resource: Option<u64>,
    live: HashSet<u64>,
    untracked: bool,
    next_pid: u64,
    supported: bool,
    other_owner_alive: bool,
    cancel: Option<(&'static str, Cancellation)>,
    jump: Option<(&'static str, u64)>,
    second_jump: Option<(&'static str, u64)>,
}
impl Default for Script {
    fn default() -> Self {
        Self {
            calls: vec![],
            root_start: 0,
            root_status: None,
            root_stop: Ok(RootState::Released),
            engine_start: Start::Started,
            engine_rejection: PortError::Offline,
            engine_status: None,
            engine_stop: Ok(EngineState::Exited),
            resource: None,
            live: HashSet::new(),
            untracked: false,
            next_pid: 1,
            supported: true,
            other_owner_alive: true,
            cancel: None,
            jump: None,
            second_jump: None,
        }
    }
}
struct FakeRoot {
    s: Arc<Mutex<Script>>,
    clock: TestClock,
}
struct FakeEngine {
    s: Arc<Mutex<Script>>,
    clock: TestClock,
}
fn hook(s: &Arc<Mutex<Script>>, clock: &TestClock, stage: &str) {
    let (cancel, jump, second_jump) = {
        let s = s.lock().unwrap();
        (s.cancel.clone(), s.jump, s.second_jump)
    };
    if let Some((at, c)) = cancel {
        if at == stage {
            c.cancel();
        }
    }
    if let Some((at, seconds)) = jump {
        if at == stage {
            clock.add(seconds);
        }
    }
    if let Some((at, seconds)) = second_jump {
        if at == stage {
            clock.add(seconds);
        }
    }
}
impl RootPort for FakeRoot {
    type Policy = Policy;
    type Proof = Proof;
    fn expires_at(&self, p: &Proof) -> Option<Instant> {
        Some(p.expiry)
    }
    fn start(&mut self, _: &Policy, p: &Proof, b: &Budget) -> RootStart {
        let mode = {
            let mut s = self.s.lock().unwrap();
            s.calls.push(("root_start", p.operation, b.epoch()));
            if s.resource.is_some_and(|operation| operation != p.operation) {
                return RootStart::Rejected(PortError::Denied);
            }
            if s.root_start != 1 {
                s.resource = Some(p.operation);
            }
            s.root_start
        };
        hook(&self.s, &self.clock, "root_start");
        match mode {
            0 => RootStart::Active,
            1 => RootStart::Rejected(PortError::Denied),
            _ => RootStart::Unknown(PortError::Deadline),
        }
    }
    fn status(&mut self, p: &Proof, b: &Budget) -> Result<RootState, PortError> {
        let result = {
            let mut s = self.s.lock().unwrap();
            s.calls.push(("root_status", p.operation, b.epoch()));
            s.root_status.unwrap_or(if s.resource == Some(p.operation) {
                Ok(RootState::Active)
            } else {
                Ok(RootState::Released)
            })
        };
        hook(&self.s, &self.clock, "root_status");
        result
    }
    fn stop(&mut self, p: &Proof, b: &Budget) -> Result<RootState, PortError> {
        assert!(b.is_cleanup());
        let result = {
            let mut s = self.s.lock().unwrap();
            s.calls.push(("root_stop", p.operation, b.epoch()));
            assert!(s.resource.is_none() || s.resource == Some(p.operation));
            let result = s.root_stop;
            if result == Ok(RootState::Released) {
                s.resource = None;
                s.untracked = false;
                s.live.clear();
            }
            assert!(s.other_owner_alive);
            result
        };
        hook(&self.s, &self.clock, "root_stop");
        result
    }
}
impl EnginePort<Proof> for FakeEngine {
    type Process = Process;
    fn supported(&self) -> bool {
        self.s.lock().unwrap().supported
    }
    fn start(&mut self, p: &Proof, b: &Budget) -> EngineStart<Process> {
        let (mode, id) = {
            let mut s = self.s.lock().unwrap();
            assert_eq!(s.resource, Some(p.operation));
            s.calls.push(("engine_start", p.operation, b.epoch()));
            let id = s.next_pid;
            s.next_pid += 1;
            match s.engine_start {
                Start::Started | Start::Partial => {
                    s.live.insert(id);
                }
                Start::Unknown => s.untracked = true,
                _ => {}
            }
            (s.engine_start, id)
        };
        hook(&self.s, &self.clock, "engine_start");
        match mode {
            Start::Started => EngineStart::Started(Process(id)),
            Start::Partial => EngineStart::Partial(Process(id)),
            Start::Rejected => EngineStart::Rejected(self.s.lock().unwrap().engine_rejection),
            Start::Unknown => EngineStart::Unknown(PortError::Deadline),
        }
    }
    fn status(&mut self, p: &mut Process, b: &Budget) -> Result<EngineState, PortError> {
        let result = {
            let mut s = self.s.lock().unwrap();
            s.calls.push(("engine_status", p.0, b.epoch()));
            let result = s.engine_status.unwrap_or(if s.live.contains(&p.0) {
                Ok(EngineState::Ready)
            } else {
                Ok(EngineState::Exited)
            });
            if result == Ok(EngineState::Exited) {
                s.live.remove(&p.0);
            }
            result
        };
        hook(&self.s, &self.clock, "engine_status");
        result
    }
    fn stop(&mut self, p: &mut Process, b: &Budget) -> Result<EngineState, PortError> {
        assert!(b.is_cleanup());
        let result = {
            let mut s = self.s.lock().unwrap();
            s.calls.push(("engine_stop", p.0, b.epoch()));
            let result = s.engine_stop;
            if result == Ok(EngineState::Exited) {
                s.live.remove(&p.0);
            }
            assert!(s.other_owner_alive);
            result
        };
        hook(&self.s, &self.clock, "engine_stop");
        result
    }
}
struct Fixture {
    s: Arc<Mutex<Script>>,
    clock: TestClock,
}
type Manager = Supervisor<FakeRoot, FakeEngine, TestClock>;
fn begin(m: &mut Manager, proof: Proof) -> Result<Snapshot, Error> {
    let context = m.arm()?;
    m.begin(context, Policy, proof)
}
impl Fixture {
    fn new() -> Self {
        Self {
            s: Arc::new(Mutex::new(Script::default())),
            clock: TestClock {
                now: Arc::new(Mutex::new(Instant::now())),
                calls: Arc::new(AtomicU64::new(0)),
            },
        }
    }
    fn manager(&self) -> Manager {
        Supervisor::new(
            FakeRoot {
                s: self.s.clone(),
                clock: self.clock.clone(),
            },
            FakeEngine {
                s: self.s.clone(),
                clock: self.clock.clone(),
            },
            self.clock.clone(),
        )
        .unwrap()
    }
    fn proof(&self, operation: u64) -> Proof {
        Proof {
            operation,
            expiry: *self.clock.now.lock().unwrap() + Duration::from_secs(120),
        }
    }
    fn count(&self, stage: &str) -> usize {
        self.s
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|v| v.0 == stage)
            .count()
    }
    fn ready(&self) -> (Manager, Reference) {
        let mut m = self.manager();
        let r = m.acquire(Owner::ManualEngine).unwrap();
        assert_eq!(begin(&mut m, self.proof(1)).unwrap().phase, Phase::Starting);
        assert_eq!(m.drive().unwrap().phase, Phase::Ready);
        (m, r)
    }
}

#[test]
fn default_off_has_no_clock_root_engine_or_background_work() {
    let f = Fixture::new();
    let mut m = f.manager();
    for _ in 0..10 {
        assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
    }
    assert_eq!(f.clock.calls.load(Ordering::SeqCst), 0);
    assert!(f.s.lock().unwrap().calls.is_empty());
    drop(m);
    assert!(f.s.lock().unwrap().calls.is_empty());
}
#[test]
fn unavailable_defaults_cannot_turn_a_unit_proof_into_permission() {
    let mut m = Supervisor::new(UnavailableRoot, UnavailableEngine, SystemClock).unwrap();
    let _r = m.acquire(Owner::Statistics).unwrap();
    let context = m.arm().unwrap();
    assert_eq!(m.begin(context, (), ()), Err(Error::Unavailable));
    assert_eq!(m.snapshot().phase, Phase::Disabled);
}
#[test]
fn startup_is_not_ready_until_real_port_observation() {
    let f = Fixture::new();
    let mut m = f.manager();
    assert_eq!(begin(&mut m, f.proof(1)), Err(Error::NoOwners));
    let _r = m.acquire(Owner::ManualEngine).unwrap();
    let s = begin(&mut m, f.proof(1)).unwrap();
    assert_eq!(s.phase, Phase::Starting);
    assert_eq!(s.attempts, 1);
    assert_eq!(f.count("root_start"), 1);
    assert_eq!(f.count("engine_status"), 0);
    assert_eq!(m.drive().unwrap().phase, Phase::Ready);
}
#[test]
fn expired_or_missing_software_does_not_activate_root() {
    for supported in [false, true] {
        let f = Fixture::new();
        let mut m = f.manager();
        let _r = m.acquire(Owner::Remote).unwrap();
        f.s.lock().unwrap().supported = supported;
        let p = Proof {
            operation: 1,
            expiry: *f.clock.now.lock().unwrap(),
        };
        assert_eq!(
            begin(&mut m, p),
            Err(if supported {
                Error::ExpiredProof
            } else {
                Error::Unavailable
            })
        );
        assert_eq!(f.count("root_start"), 0);
    }
}
#[test]
fn three_references_are_distinct_and_statistics_release_keeps_remote_alive() {
    let f = Fixture::new();
    let (mut m, manual) = f.ready();
    let remote = m.acquire(Owner::Remote).unwrap();
    let stats = m.acquire(Owner::Statistics).unwrap();
    assert!(matches!(
        m.acquire(Owner::Statistics),
        Err(Error::DuplicateOwner)
    ));
    assert_eq!(m.release(stats).unwrap().references, 2);
    assert_eq!(m.release(manual).unwrap().phase, Phase::Ready);
    assert_eq!(f.count("engine_stop"), 0);
    assert_eq!(m.release(remote).unwrap().phase, Phase::Stopping);
    assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
    assert_eq!(f.count("engine_stop"), 1);
    assert_eq!(f.count("root_stop"), 1);
    assert!(f.s.lock().unwrap().other_owner_alive);
}
#[test]
fn stale_and_foreign_tickets_do_not_release_a_new_owner() {
    let f = Fixture::new();
    let mut m = f.manager();
    let r = m.acquire(Owner::Remote).unwrap();
    let replay = Reference {
        instance: r.instance,
        owner: r.owner,
        id: r.id,
    };
    m.release(r).unwrap();
    let _new = m.acquire(Owner::Remote).unwrap();
    assert_eq!(m.release(replay), Err(Error::StaleReference));
    assert_eq!(m.snapshot().references, 1);
    let other = Fixture::new();
    let mut n = other.manager();
    let bad = n.acquire(Owner::Remote).unwrap();
    assert_eq!(m.release(bad), Err(Error::ForeignReference));
    assert_eq!(m.snapshot().references, 1);
}
#[test]
fn root_rejection_never_launches_child_and_unknown_ack_is_cleanup_only() {
    for mode in [1, 2] {
        let f = Fixture::new();
        let mut m = f.manager();
        let _r = m.acquire(Owner::Remote).unwrap();
        f.s.lock().unwrap().root_start = mode;
        let state = begin(&mut m, f.proof(1)).unwrap();
        assert_eq!(
            state.phase,
            if mode == 1 {
                Phase::Disabled
            } else {
                Phase::Unknown
            }
        );
        assert_eq!(f.count("engine_start"), 0);
        if mode == 2 {
            assert_eq!(begin(&mut m, f.proof(2)), Err(Error::Busy));
            assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
            assert_eq!(f.count("root_stop"), 1);
        }
    }
}
#[test]
fn partial_and_untracked_child_failures_are_cleaned_with_original_proof() {
    for mode in [Start::Partial, Start::Unknown] {
        let f = Fixture::new();
        let mut m = f.manager();
        let _r = m.acquire(Owner::Remote).unwrap();
        f.s.lock().unwrap().engine_start = mode;
        assert_eq!(begin(&mut m, f.proof(1)).unwrap().phase, Phase::Unknown);
        assert_eq!(begin(&mut m, f.proof(2)), Err(Error::Busy));
        assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
        let s = f.s.lock().unwrap();
        assert!(s.live.is_empty());
        assert!(!s.untracked);
        assert!(s
            .calls
            .iter()
            .filter(|v| v.0 == "root_stop")
            .all(|v| v.1 == 1));
    }
}
#[test]
fn cancellation_during_root_or_child_start_never_adopts_late_ready() {
    for stage in ["root_start", "engine_start"] {
        let f = Fixture::new();
        let mut m = f.manager();
        let _r = m.acquire(Owner::Remote).unwrap();
        let context = m.arm().unwrap();
        f.s.lock().unwrap().cancel = Some((stage, context.clone()));
        assert_eq!(
            m.begin(context, Policy, f.proof(1)).unwrap().phase,
            Phase::Unknown
        );
        assert_eq!(
            f.count("engine_start"),
            usize::from(stage == "engine_start")
        );
        assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
        f.s.lock().unwrap().cancel = None;
        assert_eq!(begin(&mut m, f.proof(2)).unwrap().phase, Phase::Starting);
        let cancel = m.cancellation();
        f.s.lock().unwrap().cancel = Some(("engine_status", cancel));
        assert_eq!(m.drive().unwrap().phase, Phase::Unknown);
        assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
    }
}

#[test]
fn cancelled_or_superseded_queued_start_never_reaches_root() {
    let f = Fixture::new();
    let mut m = f.manager();
    let _r = m.acquire(Owner::Remote).unwrap();
    let old = m.arm().unwrap();
    let current = m.arm().unwrap();
    assert_eq!(m.begin(old, Policy, f.proof(1)), Err(Error::StaleContext));
    current.cancel();
    assert_eq!(m.begin(current, Policy, f.proof(2)), Err(Error::Cancelled));
    assert_eq!(f.count("root_start"), 0);
    assert_eq!(f.count("engine_start"), 0);
    assert_eq!(begin(&mut m, f.proof(3)).unwrap().phase, Phase::Starting);
}
#[test]
fn active_cancel_handle_is_one_way_and_old_handle_cannot_cancel_next_epoch() {
    let f = Fixture::new();
    let (mut m, _r) = f.ready();
    let old = m.cancellation();
    old.cancel();
    assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
    begin(&mut m, f.proof(2)).unwrap();
    old.cancel();
    assert_eq!(m.drive().unwrap().phase, Phase::Ready);
    assert_eq!(m.snapshot().epoch, 2);
}
#[test]
fn root_or_engine_late_return_keeps_handles_for_cleanup() {
    for stage in ["root_start", "engine_start"] {
        let f = Fixture::new();
        let mut m = f.manager();
        let _r = m.acquire(Owner::Remote).unwrap();
        f.s.lock().unwrap().jump = Some((stage, 11));
        assert_eq!(begin(&mut m, f.proof(1)).unwrap().phase, Phase::Unknown);
        f.s.lock().unwrap().jump = None;
        assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
        assert_eq!(f.count("root_stop"), 1);
    }
}
#[test]
fn child_stop_timeout_is_queried_not_reissued_or_root_released_early() {
    let f = Fixture::new();
    let (mut m, r) = f.ready();
    f.s.lock().unwrap().engine_stop = Err(PortError::Deadline);
    m.release(r).unwrap();
    assert_eq!(m.drive().unwrap().phase, Phase::Unknown);
    assert_eq!(m.drive().unwrap().phase, Phase::Unknown);
    assert_eq!(f.count("engine_stop"), 1);
    assert_eq!(f.count("root_stop"), 0);
    f.s.lock().unwrap().engine_status = Some(Ok(EngineState::Exited));
    assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
    assert_eq!(f.count("root_stop"), 1);
}
#[test]
fn lost_root_stop_ack_blocks_restart_and_uses_original_read_only_query() {
    let f = Fixture::new();
    let (mut m, r) = f.ready();
    f.s.lock().unwrap().root_stop = Err(PortError::Deadline);
    m.release(r).unwrap();
    assert_eq!(m.drive().unwrap().phase, Phase::Unknown);
    assert_eq!(begin(&mut m, f.proof(2)), Err(Error::Busy));
    m.drive().unwrap();
    assert_eq!(f.count("root_stop"), 1);
    f.s.lock().unwrap().root_status = Some(Ok(RootState::Released));
    assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
}
#[test]
fn expired_proof_triggers_cleanup_but_denied_root_cleanup_stays_unknown() {
    let f = Fixture::new();
    let mut m = f.manager();
    let _r = m.acquire(Owner::Remote).unwrap();
    let proof = Proof {
        operation: 1,
        expiry: *f.clock.now.lock().unwrap() + Duration::from_secs(2),
    };
    begin(&mut m, proof).unwrap();
    f.clock.add(3);
    f.s.lock().unwrap().root_stop = Err(PortError::Denied);
    assert_eq!(m.drive().unwrap().phase, Phase::Unknown);
    assert_eq!(begin(&mut m, f.proof(2)), Err(Error::Busy));
    assert_eq!(f.count("root_start"), 1);
}
#[test]
fn retries_reuse_same_root_grant_with_delay_and_strict_attempt_cap() {
    let f = Fixture::new();
    let mut m = f.manager();
    let _r = m.acquire(Owner::Remote).unwrap();
    f.s.lock().unwrap().engine_start = Start::Rejected;
    begin(&mut m, f.proof(1)).unwrap();
    for _ in 0..10 {
        m.drive().unwrap();
    }
    assert_eq!(f.count("engine_start"), 1);
    f.clock.add(5);
    assert_eq!(m.drive().unwrap().attempts, 2);
    f.clock.add(5);
    assert_eq!(m.drive().unwrap().attempts, 3);
    assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
    assert_eq!(f.count("root_start"), 1);
    assert_eq!(f.count("engine_start"), 3);
}
#[test]
fn retry_window_and_current_root_evidence_cannot_be_skipped() {
    for expired in [false, true] {
        let f = Fixture::new();
        let mut m = f.manager();
        let _r = m.acquire(Owner::Remote).unwrap();
        f.s.lock().unwrap().engine_start = Start::Rejected;
        begin(&mut m, f.proof(1)).unwrap();
        f.clock.add(if expired { 31 } else { 5 });
        if !expired {
            f.s.lock().unwrap().root_status = Some(Ok(RootState::Unknown));
        }
        m.drive().unwrap();
        assert_ne!(m.snapshot().phase, Phase::Ready);
        assert_eq!(f.count("engine_start"), 1);
    }
}
#[test]
fn ready_child_exit_is_observed_before_retry_and_never_from_pid_alone() {
    let f = Fixture::new();
    let (mut m, _r) = f.ready();
    f.s.lock().unwrap().engine_status = Some(Ok(EngineState::Exited));
    assert_eq!(m.drive().unwrap().phase, Phase::WaitingRetry);
    m.drive().unwrap();
    assert_eq!(f.count("engine_start"), 1);
    f.clock.add(5);
    f.s.lock().unwrap().engine_status = None;
    assert_eq!(m.drive().unwrap().phase, Phase::Starting);
    assert_eq!(f.count("engine_start"), 2);
}
#[test]
fn rollback_retains_original_scope_and_does_not_start_again() {
    let f = Fixture::new();
    let (mut m, _r) = f.ready();
    *f.clock.now.lock().unwrap() -= Duration::from_secs(1);
    assert_eq!(m.drive(), Err(Error::ClockRollback));
    assert_eq!(m.snapshot().phase, Phase::Unknown);
    assert_eq!(f.count("root_start"), 1);
    f.clock.add(2);
    assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
}
#[test]
fn stopping_callbacks_share_one_budget_and_late_exit_needs_new_observation() {
    let f = Fixture::new();
    let (mut m, r) = f.ready();
    f.s.lock().unwrap().jump = Some(("engine_stop", 6));
    m.release(r).unwrap();
    assert_eq!(m.drive().unwrap().phase, Phase::Unknown);
    assert_eq!(f.count("root_stop"), 0);
    f.s.lock().unwrap().jump = None;
    assert_eq!(m.drive().unwrap().phase, Phase::Unknown);
    assert_eq!(f.count("root_stop"), 0);
    m.resume_cleanup().unwrap();
    assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
    assert_eq!(f.count("engine_stop"), 1);
}
#[test]
fn default_drop_only_cleans_original_owned_resources() {
    let f = Fixture::new();
    let (m, _r) = f.ready();
    drop(m);
    assert_eq!(f.count("engine_stop"), 1);
    assert_eq!(f.count("root_stop"), 1);
    assert!(f.s.lock().unwrap().other_owner_alive);
}
#[test]
fn counters_and_limits_cannot_wrap_or_raise_budgets() {
    let f = Fixture::new();
    let mut m = f.manager();
    m.next_reference = u64::MAX;
    assert!(matches!(m.acquire(Owner::Remote), Err(Error::Exhausted)));
    assert_eq!(m.snapshot().references, 0);
    m.next_reference = 1;
    let _r = m.acquire(Owner::Remote).unwrap();
    m.epoch = u64::MAX;
    assert_eq!(begin(&mut m, f.proof(1)), Err(Error::Exhausted));
    assert_eq!(f.count("root_start"), 0);
    for attempts in [0, 4] {
        assert!(Limits::bounded(
            Duration::from_secs(10),
            Duration::from_secs(5),
            Duration::from_secs(5),
            Duration::from_secs(30),
            attempts
        )
        .is_err());
    }
    assert!(Limits::bounded(
        Duration::from_secs(11),
        Duration::from_secs(5),
        Duration::from_secs(5),
        Duration::from_secs(30),
        3
    )
    .is_err());
    assert!(Limits::bounded(
        Duration::from_secs(10),
        Duration::from_secs(5),
        Duration::from_secs(4),
        Duration::from_secs(30),
        3
    )
    .is_err());
}

#[test]
fn permanent_rejection_never_uses_transient_reconnect_budget() {
    for rejection in [
        PortError::Denied,
        PortError::Unavailable,
        PortError::Protocol,
        PortError::Deadline,
    ] {
        let f = Fixture::new();
        let mut m = f.manager();
        let _r = m.acquire(Owner::Remote).unwrap();
        {
            let mut s = f.s.lock().unwrap();
            s.engine_start = Start::Rejected;
            s.engine_rejection = rejection;
        }
        assert_eq!(begin(&mut m, f.proof(1)).unwrap().phase, Phase::Unknown);
        f.clock.add(5);
        assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
        assert_eq!(f.count("engine_start"), 1);
        assert_eq!(f.count("root_stop"), 1);
    }
}

#[test]
fn another_root_owner_is_never_adopted_or_stopped() {
    let f = Fixture::new();
    f.s.lock().unwrap().resource = Some(99);
    let mut m = f.manager();
    let _r = m.acquire(Owner::Remote).unwrap();
    assert_eq!(begin(&mut m, f.proof(1)).unwrap().phase, Phase::Disabled);
    drop(m);
    assert_eq!(f.count("engine_start"), 0);
    assert_eq!(f.count("root_stop"), 0);
    assert_eq!(f.s.lock().unwrap().resource, Some(99));
}

#[test]
fn proof_expiry_and_root_time_reduce_the_same_initial_start_budget() {
    for (root_seconds, engine_seconds, expiry_seconds) in [(0, 3, 2), (9, 2, 120)] {
        let f = Fixture::new();
        let mut m = f.manager();
        let _r = m.acquire(Owner::Remote).unwrap();
        f.s.lock().unwrap().jump = Some(("root_start", root_seconds));
        if root_seconds == 0 {
            f.s.lock().unwrap().jump = Some(("engine_start", engine_seconds));
        } else {
            f.s.lock().unwrap().second_jump = Some(("engine_start", engine_seconds));
        }
        let proof = Proof {
            operation: 1,
            expiry: *f.clock.now.lock().unwrap() + Duration::from_secs(expiry_seconds),
        };
        let state = begin(&mut m, proof).unwrap();
        assert_eq!(state.phase, Phase::Unknown);
        assert_eq!(f.count("engine_start"), 1);
        f.s.lock().unwrap().jump = None;
        f.s.lock().unwrap().second_jump = None;
    }
}

#[test]
fn repeated_starting_polls_do_not_extend_original_start_deadline() {
    let f = Fixture::new();
    let mut m = f.manager();
    let _r = m.acquire(Owner::Remote).unwrap();
    f.s.lock().unwrap().engine_status = Some(Ok(EngineState::Starting));
    begin(&mut m, f.proof(1)).unwrap();
    f.clock.add(9);
    assert_eq!(m.drive().unwrap().phase, Phase::Starting);
    f.clock.add(2);
    f.s.lock().unwrap().engine_status = Some(Ok(EngineState::Ready));
    assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
    assert_eq!(f.count("root_stop"), 1);
    assert_eq!(f.count("engine_start"), 1);
}

#[test]
fn readiness_regression_does_not_open_an_unbounded_hidden_reconnect_loop() {
    let f = Fixture::new();
    let (mut m, _r) = f.ready();
    f.s.lock().unwrap().engine_status = Some(Ok(EngineState::Starting));
    assert_eq!(m.drive().unwrap().phase, Phase::Unknown);
    assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
    assert_eq!(f.count("engine_start"), 1);
    assert_eq!(f.count("root_stop"), 1);
}

#[test]
fn new_reference_cannot_rescue_an_already_cancelled_run() {
    let f = Fixture::new();
    let (mut m, old) = f.ready();
    let previous = m.cancellation();
    m.release(old).unwrap();
    let _new = m.acquire(Owner::Remote).unwrap();
    assert_eq!(m.snapshot().phase, Phase::Stopping);
    assert_eq!(begin(&mut m, f.proof(2)), Err(Error::Busy));
    assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
    begin(&mut m, f.proof(2)).unwrap();
    previous.cancel();
    assert_eq!(m.drive().unwrap().phase, Phase::Ready);
}

#[test]
fn root_release_returned_after_stop_budget_is_rechecked_without_second_stop() {
    let f = Fixture::new();
    let (mut m, r) = f.ready();
    f.s.lock().unwrap().jump = Some(("root_stop", 6));
    m.release(r).unwrap();
    assert_eq!(m.drive().unwrap().phase, Phase::Unknown);
    f.s.lock().unwrap().jump = None;
    assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
    assert_eq!(f.count("root_stop"), 1);
}

#[test]
fn stopping_polls_cannot_extend_total_budget_or_repeat_mutations() {
    let f = Fixture::new();
    let (mut m, r) = f.ready();
    f.s.lock().unwrap().engine_stop = Ok(EngineState::Unknown);
    m.release(r).unwrap();
    assert_eq!(m.drive().unwrap().phase, Phase::Unknown);
    f.clock.add(4);
    assert_eq!(m.drive().unwrap().phase, Phase::Unknown);
    f.clock.add(1);
    f.s.lock().unwrap().engine_status = Some(Ok(EngineState::Exited));
    for _ in 0..3 {
        assert_eq!(m.drive().unwrap().phase, Phase::Unknown);
        f.clock.add(2);
    }
    assert_eq!(f.count("engine_stop"), 1);
    assert_eq!(f.count("root_stop"), 0);
    assert_eq!(f.count("root_start"), 1);
    m.resume_cleanup().unwrap();
    assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
    assert_eq!(f.count("root_stop"), 1);
    assert_eq!(f.count("engine_start"), 1);
    assert_eq!(m.resume_cleanup(), Err(Error::Busy));
}

#[test]
fn late_read_only_reconciliation_requires_both_original_child_and_root_exit() {
    let f = Fixture::new();
    let (mut m, r) = f.ready();
    f.s.lock().unwrap().engine_stop = Ok(EngineState::Unknown);
    m.release(r).unwrap();
    m.drive().unwrap();
    f.clock.add(6);
    f.s.lock().unwrap().root_status = Some(Ok(RootState::Released));
    assert_eq!(m.drive().unwrap().phase, Phase::Unknown);
    assert_eq!(f.count("root_status"), 1); // initial readiness only
    f.s.lock().unwrap().engine_status = Some(Ok(EngineState::Exited));
    assert_eq!(m.drive().unwrap().phase, Phase::Disabled);
    assert_eq!(f.count("root_stop"), 0);
    assert_eq!(f.count("engine_stop"), 1);
}
