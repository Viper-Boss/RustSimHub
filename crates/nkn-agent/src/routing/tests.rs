// SPDX-License-Identifier: AGPL-3.0-only
//! Real Router tests. Only the external crypto/Core ports and clock are synthetic.
//! Fixture byte strings are NOT a cipher or a production authentication adapter.
use super::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone)]
struct TestClock(Rc<Cell<Instant>>);
impl Clock for TestClock {
    fn now(&self) -> Instant {
        self.0.get()
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Binding([u64; 10]);
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Admit,
    Inspect(usize),
    Decrypt,
    Execute,
    Seal,
}
enum Effect {
    Delay(Duration),
    Rollback,
    Revoke,
    LoseParent,
    Rebind,
    Shorten(Instant),
    Extend,
}
#[derive(Clone, Copy)]
enum Mode {
    Reply,
    Denied,
    Unavailable,
    Unknown,
}
struct World {
    clock: TestClock,
    binding: Binding,
    record_binding: Binding,
    purpose: Purpose,
    record_purpose: Purpose,
    direction: Direction,
    version: u16,
    sequence: u64,
    service: String,
    body: Vec<u8>,
    response: Vec<u8>,
    sealed: Vec<u8>,
    accepted_wire: Vec<u8>,
    expires: Instant,
    live: bool,
    membership: bool,
    core_identity: bool,
    scope: bool,
    mode: Mode,
    admit_error: Option<PortError>,
    decode_error: Option<PortError>,
    seal_error: Option<PortError>,
    hook: Option<(Stage, Effect)>,
    revocation: Option<Revocation>,
    admitted: usize,
    inspected: usize,
    decoded: usize,
    sealed_count: usize,
    retired: usize,
    core_calls: usize,
    media_calls: usize,
    effects: usize,
    budgets: Vec<(Stage, Instant)>,
    replies: Vec<(Service, u64)>,
    received_bodies: Vec<Vec<u8>>,
}
impl World {
    fn new(clock: TestClock) -> Self {
        let expires = clock.now() + Duration::from_secs(60);
        Self {
            clock,
            binding: Binding([1; 10]),
            record_binding: Binding([1; 10]),
            purpose: Purpose::NknRemote,
            record_purpose: Purpose::NknRemote,
            direction: Direction::PeerToDevice,
            version: 1,
            sequence: 0,
            service: "core-auth-v2".into(),
            body: b"original request UUID and AuthRequest".to_vec(),
            response: b"original Core result".to_vec(),
            sealed: b"sealed fixture".to_vec(),
            accepted_wire: b"cipher fixture".to_vec(),
            expires,
            live: true,
            membership: true,
            core_identity: true,
            scope: true,
            mode: Mode::Reply,
            admit_error: None,
            decode_error: None,
            seal_error: None,
            hook: None,
            revocation: None,
            admitted: 0,
            inspected: 0,
            decoded: 0,
            sealed_count: 0,
            retired: 0,
            core_calls: 0,
            media_calls: 0,
            effects: 0,
            budgets: Vec::new(),
            replies: Vec::new(),
            received_bodies: Vec::new(),
        }
    }
    fn stage(&mut self, stage: Stage, budget: &CallBudget) {
        self.budgets.push((stage, budget.deadline()));
        if self
            .hook
            .as_ref()
            .is_none_or(|(target, _)| *target != stage)
        {
            return;
        }
        match self.hook.take().unwrap().1 {
            Effect::Delay(duration) => self.clock.0.set(self.clock.now() + duration),
            Effect::Rollback => self.clock.0.set(self.clock.now() - Duration::from_secs(1)),
            Effect::Revoke => self.revocation.as_ref().unwrap().revoke(),
            Effect::LoseParent => self.live = false,
            Effect::Rebind => self.binding.0[3] += 1,
            Effect::Shorten(until) => self.expires = until,
            Effect::Extend => self.expires += Duration::from_secs(600),
        }
    }
    fn facts(&self) -> SessionFacts<Binding> {
        SessionFacts {
            binding: self.binding,
            purpose: self.purpose,
            expires_at: self.expires,
        }
    }
}
type Shared = Rc<RefCell<World>>;
struct Transport(Shared);
impl TransportPort for Transport {
    type Admission = ();
    type Session = usize;
    type Binding = Binding;
    fn admit(
        &mut self,
        _: (),
        budget: &CallBudget,
    ) -> Result<(usize, SessionFacts<Binding>), PortError> {
        let mut w = self.0.borrow_mut();
        w.admitted += 1;
        w.stage(Stage::Admit, budget);
        if let Some(error) = w.admit_error {
            return Err(error);
        }
        if !w.membership || !w.core_identity {
            return Err(PortError::Denied);
        }
        Ok((w.admitted, w.facts()))
    }
    fn inspect(
        &mut self,
        _: &mut usize,
        budget: &CallBudget,
    ) -> Result<SessionFacts<Binding>, PortError> {
        let mut w = self.0.borrow_mut();
        w.inspected += 1;
        let count = w.inspected;
        w.stage(Stage::Inspect(count), budget);
        if !w.live || !w.membership || !w.core_identity {
            return Err(PortError::Denied);
        }
        Ok(w.facts())
    }
    fn open_record(
        &mut self,
        _: &mut usize,
        wire: &[u8],
        budget: &CallBudget,
    ) -> Result<VerifiedRecord<Binding>, PortError> {
        let mut w = self.0.borrow_mut();
        w.decoded += 1;
        w.stage(Stage::Decrypt, budget);
        if let Some(error) = w.decode_error {
            return Err(error);
        }
        if wire != w.accepted_wire {
            return Err(PortError::InvalidRecord);
        }
        Ok(VerifiedRecord {
            binding: w.record_binding,
            version: w.version,
            purpose: w.record_purpose,
            direction: w.direction,
            sequence: w.sequence,
            service: w.service.clone(),
            body: w.body.clone(),
        })
    }
    fn seal_reply(
        &mut self,
        _: &mut usize,
        service: Service,
        sequence: u64,
        _: &[u8],
        budget: &CallBudget,
    ) -> Result<Vec<u8>, PortError> {
        let mut w = self.0.borrow_mut();
        w.sealed_count += 1;
        w.replies.push((service, sequence));
        w.stage(Stage::Seal, budget);
        if let Some(error) = w.seal_error {
            return Err(error);
        }
        Ok(w.sealed.clone())
    }
    fn retire(&mut self, _: usize) {
        self.0.borrow_mut().retired += 1;
    }
}
struct Business(Shared);
impl Business {
    fn execute(
        &mut self,
        binding: &Binding,
        body: &[u8],
        budget: &CallBudget,
        media: bool,
    ) -> BusinessOutcome {
        let mut w = self.0.borrow_mut();
        if media {
            w.media_calls += 1;
        } else {
            w.core_calls += 1;
        }
        w.received_bodies.push(body.to_vec());
        if !w.live
            || !w.scope
            || *binding != w.binding
            || budget.cancelled()
            || w.clock.now() >= budget.deadline()
        {
            return BusinessOutcome::Denied;
        }
        if matches!(w.mode, Mode::Reply | Mode::Unknown) {
            w.effects += 1;
        }
        w.stage(Stage::Execute, budget);
        match w.mode {
            Mode::Reply => BusinessOutcome::Reply(w.response.clone()),
            Mode::Denied => BusinessOutcome::Denied,
            Mode::Unavailable => BusinessOutcome::Unavailable,
            Mode::Unknown => BusinessOutcome::Unknown,
        }
    }
}
impl BusinessPort<Binding> for Business {
    fn core_auth(
        &mut self,
        binding: &Binding,
        body: &[u8],
        budget: &CallBudget,
    ) -> BusinessOutcome {
        self.execute(binding, body, budget, false)
    }
    fn call_media(
        &mut self,
        binding: &Binding,
        body: &[u8],
        budget: &CallBudget,
    ) -> BusinessOutcome {
        self.execute(binding, body, budget, true)
    }
}
struct Fixture {
    router: Router<Transport, Business, TestClock>,
    world: Shared,
    clock: TestClock,
}
impl Fixture {
    fn unopened() -> Self {
        let clock = TestClock(Rc::new(Cell::new(Instant::now())));
        let world = Rc::new(RefCell::new(World::new(clock.clone())));
        let router = Router::new(
            Transport(world.clone()),
            Business(world.clone()),
            clock.clone(),
        );
        Self {
            router,
            world,
            clock,
        }
    }
    fn open(&mut self) {
        self.world.borrow_mut().revocation = Some(self.router.open(()).unwrap());
    }
    fn new() -> Self {
        let mut f = Self::unopened();
        f.open();
        f
    }
    fn dispatch(&mut self) -> Result<Vec<u8>, Error> {
        let wire = self.world.borrow().accepted_wire.clone();
        self.router.dispatch(&wire)
    }
    fn hook(&self, stage: Stage, effect: Effect) {
        self.world.borrow_mut().hook = Some((stage, effect));
    }
    fn assert_pre_execution_rejected(&self) {
        assert!(!self.router.is_open());
        let w = self.world.borrow();
        assert_eq!(
            (
                w.core_calls,
                w.media_calls,
                w.effects,
                w.sealed_count,
                w.retired
            ),
            (0, 0, 0, 0, 1)
        );
        assert!(w.revocation.as_ref().unwrap().is_revoked());
    }
}

#[test]
fn default_adapter_is_unavailable_and_never_opens() {
    let mut router = Router::default();
    assert_eq!(router.open(()).err(), Some(Error::Unavailable));
    assert!(!router.is_open());
    assert_eq!(router.dispatch(b"anything"), Err(Error::Closed));
}
#[test]
fn unavailable_business_cannot_produce_a_reply() {
    let f = Fixture::unopened();
    let mut router = Router::new(
        Transport(f.world.clone()),
        UnavailableBusiness,
        f.clock.clone(),
    );
    router.open(()).unwrap();
    assert_eq!(router.dispatch(b"cipher fixture"), Err(Error::Unavailable));
    assert_eq!(f.world.borrow().sealed_count, 0);
    assert!(!router.is_open());
}
#[test]
fn both_services_deliver_the_original_body_to_only_the_matching_port() {
    for (service, expected) in [
        ("core-auth-v2", (1, 0, Service::CoreAuthV2)),
        ("call-media-v1", (0, 1, Service::CallMediaV1)),
    ] {
        let mut f = Fixture::new();
        f.world.borrow_mut().service = service.into();
        assert_eq!(f.dispatch().unwrap(), b"sealed fixture");
        let w = f.world.borrow();
        assert_eq!((w.core_calls, w.media_calls), (expected.0, expected.1));
        assert_eq!(w.received_bodies, vec![w.body.clone()]);
        assert_eq!(w.replies, vec![(expected.2, 0)]);
        assert_eq!(w.effects, 1);
    }
}
#[test]
fn membership_never_substitutes_for_registered_core_identity() {
    for (member, identity) in [(true, false), (false, true), (false, false)] {
        let mut f = Fixture::unopened();
        {
            let mut w = f.world.borrow_mut();
            w.membership = member;
            w.core_identity = identity;
        }
        assert_eq!(f.router.open(()).err(), Some(Error::Denied));
        assert_eq!(f.world.borrow().core_calls, 0);
        assert!(!f.router.is_open());
    }
}
#[test]
fn ble_session_and_ble_records_cannot_enter_normal_remote_services() {
    let mut f = Fixture::unopened();
    f.world.borrow_mut().purpose = Purpose::BleRecovery;
    assert_eq!(f.router.open(()).err(), Some(Error::Denied));
    assert_eq!(f.world.borrow().retired, 1);
    let mut f = Fixture::new();
    f.world.borrow_mut().record_purpose = Purpose::BleRecovery;
    assert_eq!(f.dispatch(), Err(Error::InvalidRecord));
    f.assert_pre_execution_rejected();
}
#[test]
fn unknown_services_and_lan_targets_are_denied_without_execution() {
    for service in [
        "",
        "Core-auth-v2",
        "core-auth-v2\0",
        "core-auth-v2 ",
        "owner",
        "ble-recovery",
        "tcp",
        "192.168.1.1:80",
        "https://localhost/",
        "/run/core.sock",
        "call-media-v2",
        "../core-auth-v2",
    ] {
        let mut f = Fixture::new();
        f.world.borrow_mut().service = service.into();
        assert_eq!(f.dispatch(), Err(Error::Denied));
        f.assert_pre_execution_rejected();
    }
}
#[test]
fn plaintext_authenticated_flag_is_not_a_verified_record() {
    let mut f = Fixture::new();
    assert_eq!(
        f.router
            .dispatch(br#"{"authenticated":true,"role":"owner","service":"core-auth-v2"}"#),
        Err(Error::InvalidRecord)
    );
    f.assert_pre_execution_rejected();
}
#[test]
fn empty_oversize_truncated_and_modified_wire_never_execute() {
    for wire in [
        Vec::new(),
        vec![1; MAX_RECORD_BYTES + 1],
        b"cipher fixtur".to_vec(),
        b"Cipher fixture".to_vec(),
    ] {
        let mut f = Fixture::new();
        assert_eq!(f.router.dispatch(&wire), Err(Error::InvalidRecord));
        f.assert_pre_execution_rejected();
    }
}
#[test]
fn authenticated_version_direction_and_body_limits_are_checked() {
    for case in 0..5 {
        let mut f = Fixture::new();
        {
            let mut w = f.world.borrow_mut();
            match case {
                0 => w.version = 0,
                1 => w.version = 2,
                2 => w.direction = Direction::DeviceToPeer,
                3 => w.body.clear(),
                _ => w.body = vec![1; MAX_BODY_BYTES + 1],
            }
        }
        assert_eq!(f.dispatch(), Err(Error::InvalidRecord));
        f.assert_pre_execution_rejected();
    }
}
#[test]
fn exact_wire_and_body_caps_are_usable() {
    let mut f = Fixture::new();
    {
        let mut w = f.world.borrow_mut();
        w.accepted_wire = vec![1; MAX_RECORD_BYTES];
        w.body = vec![2; MAX_BODY_BYTES];
        w.response = vec![3; MAX_BODY_BYTES];
        w.sealed = vec![4; MAX_RECORD_BYTES];
    }
    assert_eq!(f.dispatch().unwrap(), vec![4; MAX_RECORD_BYTES]);
    assert_eq!(f.world.borrow().effects, 1);
}
#[test]
fn every_dimension_of_foreign_record_binding_is_rejected() {
    // Synthetic dimensions stand for pin/client/Core/connection/transport/
    // realm/Owner/client-generation/line/session; equality must cover ALL.
    for index in 0..10 {
        let mut f = Fixture::new();
        f.world.borrow_mut().record_binding.0[index] += 1;
        assert_eq!(f.dispatch(), Err(Error::InvalidRecord));
        f.assert_pre_execution_rejected();
    }
}
#[test]
fn changes_in_live_binding_retire_old_session_before_decryption() {
    for index in 0..10 {
        let mut f = Fixture::new();
        f.world.borrow_mut().binding.0[index] += 1;
        assert_eq!(f.dispatch(), Err(Error::Denied));
        f.assert_pre_execution_rejected();
        assert_eq!(f.world.borrow().decoded, 0);
    }
}
#[test]
fn revoke_or_parent_loss_before_dispatch_cannot_execute() {
    for effect in [Effect::Revoke, Effect::LoseParent] {
        let mut f = Fixture::new();
        f.hook(Stage::Inspect(2), effect);
        assert!(matches!(f.dispatch(), Err(Error::Revoked | Error::Denied)));
        f.assert_pre_execution_rejected();
    }
}
#[test]
fn revocation_and_context_changes_during_decryption_cannot_execute() {
    for effect in [Effect::Revoke, Effect::LoseParent, Effect::Rebind] {
        let mut f = Fixture::new();
        f.hook(Stage::Decrypt, effect);
        assert!(matches!(f.dispatch(), Err(Error::Revoked | Error::Denied)));
        f.assert_pre_execution_rejected();
    }
}
#[test]
fn core_scope_and_current_authority_are_rechecked_on_each_request() {
    let mut f = Fixture::new();
    f.dispatch().unwrap();
    {
        let mut w = f.world.borrow_mut();
        w.scope = false;
        w.sequence = 1;
    }
    assert_eq!(f.dispatch(), Err(Error::Denied));
    let w = f.world.borrow();
    assert_eq!(
        (w.core_calls, w.effects, w.sealed_count, w.retired),
        (2, 1, 1, 1)
    );
}
#[test]
fn repeats_gaps_reordering_and_counter_wrap_are_rejected() {
    for sequence in [1, 2, u64::MAX] {
        let mut f = Fixture::new();
        f.world.borrow_mut().sequence = sequence;
        assert_eq!(f.dispatch(), Err(Error::InvalidRecord));
        f.assert_pre_execution_rejected();
    }
    let mut f = Fixture::new();
    f.dispatch().unwrap();
    assert_eq!(f.dispatch(), Err(Error::InvalidRecord));
    assert_eq!(f.world.borrow().effects, 1);
    assert_eq!(f.dispatch(), Err(Error::Closed));
}
#[test]
fn request_quota_closes_after_final_reply_without_reset_or_wrap() {
    let mut f = Fixture::new();
    for sequence in 0..MAX_REQUESTS {
        f.world.borrow_mut().sequence = sequence;
        f.dispatch().unwrap();
    }
    assert!(!f.router.is_open());
    assert_eq!(f.dispatch(), Err(Error::Closed));
    let w = f.world.borrow();
    assert_eq!(w.effects, MAX_REQUESTS as usize);
    assert_eq!(
        w.replies.last(),
        Some(&(Service::CoreAuthV2, MAX_REQUESTS - 1))
    );
    assert_eq!(w.retired, 1);
}
#[test]
fn exhausted_internal_counters_fail_closed() {
    for sent in [false, true] {
        let mut f = Fixture::new();
        let active = f.router.active.as_mut().unwrap();
        if sent {
            active.sent = u64::MAX;
        } else {
            active.received = u64::MAX;
        }
        assert_eq!(f.dispatch(), Err(Error::Exhausted));
        f.assert_pre_execution_rejected();
    }
}
#[test]
fn slow_admission_is_retired_without_opening_a_late_session() {
    let mut f = Fixture::unopened();
    f.hook(Stage::Admit, Effect::Delay(CALL_BUDGET));
    assert_eq!(f.router.open(()).err(), Some(Error::Expired));
    assert!(!f.router.is_open());
    assert_eq!(f.world.borrow().retired, 1);
    assert_eq!(f.world.borrow().inspected, 0);
}
#[test]
fn initial_short_authority_also_bounds_the_first_live_inspection() {
    let mut f = Fixture::unopened();
    let until = f.clock.now() + Duration::from_secs(1);
    f.world.borrow_mut().expires = until;
    f.open();
    let w = f.world.borrow();
    let inspection = w
        .budgets
        .iter()
        .find(|(s, _)| *s == Stage::Inspect(1))
        .unwrap();
    assert_eq!(inspection.1, until);
}
#[test]
fn session_expiry_and_shorter_call_deadline_apply_at_the_boundary() {
    let mut f = Fixture::new();
    f.clock.0.set(f.world.borrow().expires);
    assert_eq!(f.dispatch(), Err(Error::Expired));
    f.assert_pre_execution_rejected();
    let mut f = Fixture::new();
    let until = f.clock.now() + Duration::from_secs(1);
    f.world.borrow_mut().expires = until;
    f.hook(Stage::Decrypt, Effect::Delay(Duration::from_secs(1)));
    assert_eq!(f.dispatch(), Err(Error::Expired));
    f.assert_pre_execution_rejected();
}
#[test]
fn all_stages_share_the_original_absolute_deadline() {
    let mut f = Fixture::new();
    f.world.borrow_mut().budgets.clear();
    let deadline = f.clock.now() + CALL_BUDGET;
    f.hook(Stage::Decrypt, Effect::Delay(Duration::from_secs(4)));
    f.dispatch().unwrap();
    assert!(f
        .world
        .borrow()
        .budgets
        .iter()
        .all(|(_, until)| *until == deadline));
}
#[test]
fn expiry_shortened_after_decrypt_is_passed_to_actual_business_gate() {
    let mut f = Fixture::new();
    let until = f.clock.now() + Duration::from_secs(1);
    f.hook(Stage::Decrypt, Effect::Shorten(until));
    f.dispatch().unwrap();
    let w = f.world.borrow();
    assert!(w
        .budgets
        .iter()
        .filter(|(s, _)| matches!(s, Stage::Execute | Stage::Seal))
        .all(|(_, d)| *d == until));
}
#[test]
fn provider_refresh_cannot_extend_the_original_session() {
    let mut f = Fixture::new();
    let original = f.world.borrow().expires;
    f.hook(Stage::Decrypt, Effect::Extend);
    f.dispatch().unwrap();
    f.clock.0.set(original);
    f.world.borrow_mut().sequence = 1;
    assert_eq!(f.dispatch(), Err(Error::Expired));
    assert_eq!(f.world.borrow().effects, 1);
}
#[test]
fn late_decrypt_and_slow_inspect_never_enter_core() {
    for stage in [Stage::Decrypt, Stage::Inspect(2), Stage::Inspect(3)] {
        let mut f = Fixture::new();
        f.hook(stage, Effect::Delay(CALL_BUDGET));
        assert_eq!(f.dispatch(), Err(Error::Expired));
        f.assert_pre_execution_rejected();
    }
}
#[test]
fn clock_rollback_before_or_during_decrypt_retires_authority() {
    for during in [false, true] {
        let mut f = Fixture::new();
        if during {
            f.hook(Stage::Decrypt, Effect::Rollback);
        } else {
            f.clock.0.set(f.clock.now() - Duration::from_secs(1));
        }
        assert_eq!(f.dispatch(), Err(Error::ClockRollback));
        f.assert_pre_execution_rejected();
    }
}
#[test]
fn post_execution_revocation_disconnect_rebind_and_lateness_preserve_unknown() {
    for stage in [Stage::Execute, Stage::Seal] {
        for effect in [
            Effect::Revoke,
            Effect::LoseParent,
            Effect::Rebind,
            Effect::Delay(CALL_BUDGET),
            Effect::Rollback,
        ] {
            let mut f = Fixture::new();
            f.hook(stage, effect);
            assert_eq!(f.dispatch(), Err(Error::OutcomeUnknown));
            assert_eq!(f.dispatch(), Err(Error::Closed));
            assert_eq!(f.world.borrow().effects, 1);
            assert_eq!(f.world.borrow().retired, 1);
        }
    }
}
#[test]
fn core_unknown_never_retries_or_seals_a_success() {
    let mut f = Fixture::new();
    f.world.borrow_mut().mode = Mode::Unknown;
    assert_eq!(f.dispatch(), Err(Error::OutcomeUnknown));
    assert_eq!(f.dispatch(), Err(Error::Closed));
    let w = f.world.borrow();
    assert_eq!((w.core_calls, w.effects, w.sealed_count), (1, 1, 0));
}
#[test]
fn definite_business_denial_or_unavailable_never_seals_a_reply() {
    for mode in [Mode::Denied, Mode::Unavailable] {
        let mut f = Fixture::new();
        f.world.borrow_mut().mode = mode;
        assert!(matches!(
            f.dispatch(),
            Err(Error::Denied | Error::Unavailable)
        ));
        let w = f.world.borrow();
        assert_eq!((w.effects, w.sealed_count, w.retired), (0, 0, 1));
    }
}
#[test]
fn crypto_failures_before_execution_are_finite_and_retire_session() {
    for error in [
        PortError::Denied,
        PortError::Unavailable,
        PortError::InvalidRecord,
    ] {
        let mut f = Fixture::new();
        f.world.borrow_mut().decode_error = Some(error);
        assert_eq!(f.dispatch(), Err(error.into()));
        f.assert_pre_execution_rejected();
    }
}
#[test]
fn seal_errors_or_bad_reply_sizes_preserve_unknown_without_plaintext() {
    for case in 0..7 {
        let mut f = Fixture::new();
        {
            let mut w = f.world.borrow_mut();
            match case {
                0 => w.response.clear(),
                1 => w.response = vec![1; MAX_BODY_BYTES + 1],
                2 => w.sealed.clear(),
                3 => w.sealed = vec![1; MAX_RECORD_BYTES + 1],
                4 => w.seal_error = Some(PortError::Denied),
                5 => w.seal_error = Some(PortError::Unavailable),
                _ => w.seal_error = Some(PortError::InvalidRecord),
            }
        }
        assert_eq!(f.dispatch(), Err(Error::OutcomeUnknown));
        assert_eq!(f.world.borrow().effects, 1);
        assert!(!f.router.is_open());
    }
}
#[test]
fn duplicate_open_never_replaces_live_session_or_resets_counters() {
    let mut f = Fixture::new();
    f.dispatch().unwrap();
    assert_eq!(f.router.open(()).err(), Some(Error::Busy));
    assert_eq!(f.world.borrow().admitted, 1);
    assert_eq!(f.dispatch(), Err(Error::InvalidRecord));
    assert_eq!(f.world.borrow().effects, 1);
}
#[test]
fn close_drop_and_old_revocation_handle_cannot_affect_new_session() {
    let mut f = Fixture::new();
    let old = f.world.borrow().revocation.clone().unwrap();
    f.router.close();
    f.router.close();
    assert!(old.is_revoked());
    assert_eq!(f.world.borrow().retired, 1);
    {
        let mut w = f.world.borrow_mut();
        w.binding.0[9] += 1;
        w.record_binding = w.binding;
    }
    f.open();
    old.revoke();
    f.dispatch().unwrap();
    let current = f.world.borrow().revocation.clone().unwrap();
    drop(f.router);
    assert!(current.is_revoked());
    assert_eq!(f.world.borrow().retired, 2);
}
#[test]
fn old_connection_record_is_rejected_after_explicit_new_admission() {
    let mut f = Fixture::new();
    f.router.close();
    f.world.borrow_mut().binding.0[3] += 1;
    f.open();
    assert_eq!(f.dispatch(), Err(Error::InvalidRecord));
    assert_eq!(f.world.borrow().effects, 0);
}
