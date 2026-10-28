// SPDX-License-Identifier: AGPL-3.0-only
//! Mock implementations exist only in this cfg(test) target. The state machine
//! under test is the actual public Lifecycle, not a second fixture state machine.
use super::*;
use crate::{
    framing::{InputKind, OpaqueBytes},
    manifest::Characteristic,
    ports::SecureReply,
};
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::Waker,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Call {
    App,
    Adv,
    Unadv,
    Close,
    Unapp,
}
type Gate = Arc<Mutex<Option<Result<(), Error>>>>;

#[derive(Default)]
struct Control {
    calls: Vec<(Call, Epoch)>,
    steps: VecDeque<(Call, Gate)>,
}

impl Control {
    fn expect(&mut self, call: Call, result: Option<Result<(), Error>>) -> Gate {
        let gate = Arc::new(Mutex::new(result));
        self.steps.push_back((call, gate.clone()));
        gate
    }

    fn call(&mut self, call: Call, epoch: Epoch) -> Pending<()> {
        self.calls.push((call, epoch));
        let gate = if let Some((expected, gate)) = self.steps.pop_front() {
            assert_eq!(call, expected, "unexpected backend side effect");
            gate
        } else {
            Arc::new(Mutex::new(Some(Ok(()))))
        };
        Box::pin(Wait(gate))
    }

    fn names(&self) -> Vec<Call> {
        self.calls.iter().map(|c| c.0).collect()
    }
}

struct Wait(Gate);
impl Future for Wait {
    type Output = Result<(), Error>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        self.0
            .lock()
            .unwrap()
            .take()
            .map_or(Poll::Pending, Poll::Ready)
    }
}

#[derive(Clone)]
struct Mock(Arc<Mutex<Control>>);
impl GattBackendV1 for Mock {
    fn register_application(&mut self, e: Epoch, _: FixedManifest) -> Pending<()> {
        self.0.lock().unwrap().call(Call::App, e)
    }
    fn register_advertisement(&mut self, e: Epoch, _: FixedAdvertisement) -> Pending<()> {
        self.0.lock().unwrap().call(Call::Adv, e)
    }
    fn unregister_advertisement(&mut self, e: Epoch) -> Pending<()> {
        self.0.lock().unwrap().call(Call::Unadv, e)
    }
    fn unregister_application(&mut self, e: Epoch) -> Pending<()> {
        self.0.lock().unwrap().call(Call::Unapp, e)
    }
    fn notify(&mut self, _: Epoch, _: Peer, _: Characteristic, _: OpaqueBytes) -> Pending<()> {
        panic!("lifecycle must not publish unverified notifications")
    }
}
impl SecurePortV1 for Mock {
    fn receive(&mut self, _: Epoch, _: Peer, _: InputKind, _: OpaqueBytes) -> Pending<SecureReply> {
        panic!("lifecycle must not manufacture application messages")
    }
    fn disconnected(&mut self, e: Epoch, _: Peer) -> Pending<()> {
        self.0.lock().unwrap().call(Call::Close, e)
    }
}

fn drive(lifecycle: &mut Lifecycle<Mock, Mock>, now: u64) -> State {
    lifecycle
        .poll(&mut Context::from_waker(Waker::noop()), now)
        .unwrap()
}
fn fixture() -> (Lifecycle<Mock, Mock>, Arc<Mutex<Control>>) {
    let control = Arc::new(Mutex::new(Control::default()));
    let mock = Mock(control.clone());
    (
        Lifecycle::new(mock.clone(), Some(mock), ":1.42", [7; 16]).unwrap(),
        control,
    )
}
fn peer() -> Peer {
    Peer::from_backend_callback(":1.42", "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF").unwrap()
}

fn is_peer_live(adapter: &Lifecycle<Mock, Mock>, epoch: Epoch, peer: &Peer) -> bool {
    adapter
        .connection()
        .is_some_and(|c| c.epoch() == epoch && c.peer() == peer && adapter.is_current(&c))
}

#[test]
fn reconnecting_the_same_device_cannot_revalidate_an_old_async_connection_token() {
    let (mut adapter, _) = fixture();
    let epoch = adapter.start(0).unwrap();
    drive(&mut adapter, 1);
    adapter.connected(epoch, peer(), 2).unwrap();
    let old = adapter.connection().unwrap();
    adapter.disconnected(&old, 3).unwrap();
    drive(&mut adapter, 4);
    adapter.connected(epoch, peer(), 5).unwrap();
    let new = adapter.connection().unwrap();
    assert_ne!(old, new);
    assert_eq!(old.peer(), new.peer());
    assert_eq!(old.epoch(), new.epoch());
    assert!(!adapter.is_current(&old));
    assert!(adapter.is_current(&new));
    assert_eq!(adapter.disconnected(&old, 6), Err(Error::StaleContext));
    assert!(adapter.is_current(&new));
    adapter.stop(7).unwrap();
    drive(&mut adapter, 8);
}

#[test]
fn real_lifecycle_orders_registration_and_stop_and_keeps_unique_epochs() {
    let (mut adapter, control) = fixture();
    assert_eq!(adapter.state(), State::Disabled);
    let first = adapter.start(0).unwrap();
    assert_eq!(adapter.state(), State::Registering);
    assert_eq!(adapter.start(1), Err(Error::Busy));
    assert_eq!(drive(&mut adapter, 1), State::Advertising);
    adapter.connected(first, peer(), 2).unwrap();
    assert_eq!(adapter.state(), State::Connected);
    assert!(is_peer_live(&adapter, first, &peer()));
    adapter.stop(3).unwrap();
    assert!(!is_peer_live(&adapter, first, &peer()));
    assert_eq!(drive(&mut adapter, 4), State::Disabled);
    assert!(adapter.is_quiescent());
    assert_eq!(
        control.lock().unwrap().names(),
        vec![Call::App, Call::Adv, Call::Unadv, Call::Close, Call::Unapp]
    );
    assert!(control
        .lock()
        .unwrap()
        .calls
        .iter()
        .all(|(_, e)| *e == first));
    let second = adapter.start(5).unwrap();
    assert_ne!(first, second);
    drive(&mut adapter, 6);
    assert_eq!(
        adapter.connected(first, peer(), 7),
        Err(Error::StaleContext)
    );
    adapter.stop(8).unwrap();
    drive(&mut adapter, 9);
}

#[test]
fn missing_secure_port_invalid_owner_and_invalid_domain_cannot_register() {
    let control = Arc::new(Mutex::new(Control::default()));
    let mock = Mock(control.clone());
    assert!(matches!(
        Lifecycle::new(mock.clone(), None::<Mock>, ":1.42", [1; 16]),
        Err(Error::Unavailable)
    ));
    assert!(matches!(
        Lifecycle::new(mock.clone(), Some(mock.clone()), "org.bluez", [1; 16]),
        Err(Error::InvalidInput)
    ));
    assert!(matches!(
        Lifecycle::new(mock.clone(), Some(mock), ":1.42", [0; 16]),
        Err(Error::InvalidInput)
    ));
    assert!(control.lock().unwrap().calls.is_empty());
}

#[test]
fn failed_advertisement_compensates_only_its_epoch_and_never_reports_success() {
    let (mut adapter, control) = fixture();
    control.lock().unwrap().expect(Call::App, Some(Ok(())));
    control
        .lock()
        .unwrap()
        .expect(Call::Adv, Some(Err(Error::Backend)));
    let epoch = adapter.start(0).unwrap();
    assert_eq!(drive(&mut adapter, 1), State::Disabled);
    assert_eq!(adapter.last_error(), Some(Error::Backend));
    assert_eq!(
        control.lock().unwrap().names(),
        vec![Call::App, Call::Adv, Call::Unadv, Call::Unapp]
    );
    assert!(control
        .lock()
        .unwrap()
        .calls
        .iter()
        .all(|(_, e)| *e == epoch));
    assert!(adapter.is_quiescent());
    drive(&mut adapter, 2);
    assert_eq!(control.lock().unwrap().calls.len(), 4);
}

#[test]
fn timed_out_application_future_is_retained_and_late_success_is_unregistered() {
    let (mut adapter, control) = fixture();
    let gate = control.lock().unwrap().expect(Call::App, None);
    let epoch = adapter.start(0).unwrap();
    assert_eq!(adapter.deadline_ms(), Some(5000));
    assert_eq!(drive(&mut adapter, 4999), State::Registering);
    assert_eq!(drive(&mut adapter, 5000), State::Unknown);
    assert!(adapter.has_pending_backend());
    assert_eq!(Arc::strong_count(&gate), 2); // Owning future is still retained.
    assert_eq!(adapter.deadline_ms(), None); // No busy-spin on an expired timer.
    assert_eq!(adapter.start(5001), Err(Error::Busy));
    assert_eq!(control.lock().unwrap().names(), vec![Call::App]);
    *gate.lock().unwrap() = Some(Ok(()));
    assert_eq!(drive(&mut adapter, 5002), State::Disabled);
    assert_eq!(
        control.lock().unwrap().names(),
        vec![Call::App, Call::Unapp]
    );
    assert!(control
        .lock()
        .unwrap()
        .calls
        .iter()
        .all(|(_, e)| *e == epoch));
    assert_eq!(adapter.last_error(), Some(Error::Deadline));
    assert_eq!(Arc::strong_count(&gate), 1);
}

#[test]
fn timed_out_advertisement_still_cleans_up_late_success_without_autostart() {
    let (mut adapter, control) = fixture();
    control.lock().unwrap().expect(Call::App, Some(Ok(())));
    let gate = control.lock().unwrap().expect(Call::Adv, None);
    adapter.start(0).unwrap();
    assert_eq!(drive(&mut adapter, 10), State::Registering);
    assert_eq!(drive(&mut adapter, 5010), State::Unknown);
    assert_eq!(
        adapter.connected(adapter.epoch().unwrap(), peer(), 5010),
        Err(Error::Busy)
    );
    *gate.lock().unwrap() = Some(Ok(()));
    assert_eq!(drive(&mut adapter, 5011), State::Disabled);
    assert_eq!(
        control.lock().unwrap().names(),
        vec![Call::App, Call::Adv, Call::Unadv, Call::Unapp]
    );
    drive(&mut adapter, 5012);
    assert_eq!(control.lock().unwrap().calls.len(), 4);
}

#[test]
fn stopping_during_pending_registration_never_starts_advertising_afterward() {
    let (mut adapter, control) = fixture();
    let gate = control.lock().unwrap().expect(Call::App, None);
    adapter.start(0).unwrap();
    adapter.stop(1).unwrap();
    adapter.stop(2).unwrap();
    assert_eq!(drive(&mut adapter, 3), State::Stopping);
    *gate.lock().unwrap() = Some(Ok(()));
    assert_eq!(drive(&mut adapter, 4), State::Disabled);
    assert_eq!(
        control.lock().unwrap().names(),
        vec![Call::App, Call::Unapp]
    );
}

#[test]
fn release_and_explicit_unregister_are_idempotently_coordinated() {
    let (mut adapter, control) = fixture();
    let epoch = adapter.start(0).unwrap();
    drive(&mut adapter, 1);
    adapter.advertisement_released(epoch, ":1.42", 2).unwrap();
    adapter.advertisement_released(epoch, ":1.42", 3).unwrap();
    assert_eq!(drive(&mut adapter, 4), State::Disabled);
    assert_eq!(
        control.lock().unwrap().names(),
        vec![Call::App, Call::Adv, Call::Unapp]
    );
    adapter.advertisement_released(epoch, ":1.42", 5).unwrap();
    assert_eq!(control.lock().unwrap().calls.len(), 3);
}

#[test]
fn release_while_unregister_is_pending_does_not_start_a_second_unregister() {
    let (mut adapter, control) = fixture();
    let epoch = adapter.start(0).unwrap();
    drive(&mut adapter, 1);
    let gate = control.lock().unwrap().expect(Call::Unadv, None);
    adapter.stop(2).unwrap();
    adapter.advertisement_released(epoch, ":1.42", 3).unwrap();
    *gate.lock().unwrap() = Some(Err(Error::Backend));
    assert_eq!(drive(&mut adapter, 4), State::Disabled);
    assert_eq!(
        control.lock().unwrap().names(),
        vec![Call::App, Call::Adv, Call::Unadv, Call::Unapp]
    );
}

#[test]
fn failed_cleanup_remains_unknown_and_does_not_retry_or_restart_itself() {
    let (mut adapter, control) = fixture();
    adapter.start(0).unwrap();
    drive(&mut adapter, 1);
    control
        .lock()
        .unwrap()
        .expect(Call::Unadv, Some(Err(Error::Unknown)));
    adapter.stop(2).unwrap();
    assert_eq!(drive(&mut adapter, 3), State::Unknown);
    assert!(!adapter.is_quiescent());
    assert!(!adapter.has_pending_backend());
    assert_eq!(adapter.start(4), Err(Error::Busy));
    adapter.stop(5).unwrap();
    drive(&mut adapter, 6);
    assert_eq!(
        control.lock().unwrap().names(),
        vec![Call::App, Call::Adv, Call::Unadv, Call::Unapp]
    );
}

#[test]
fn session_cleanup_is_awaited_before_application_removal_or_a_new_peer() {
    let (mut adapter, control) = fixture();
    let epoch = adapter.start(0).unwrap();
    drive(&mut adapter, 1);
    adapter.connected(epoch, peer(), 2).unwrap();
    control.lock().unwrap().expect(Call::Unadv, Some(Ok(())));
    let gate = control.lock().unwrap().expect(Call::Close, None);
    adapter.stop(3).unwrap();
    assert_eq!(drive(&mut adapter, 4), State::Stopping);
    assert_eq!(
        control.lock().unwrap().names(),
        vec![Call::App, Call::Adv, Call::Unadv, Call::Close]
    );
    assert!(!is_peer_live(&adapter, epoch, &peer()));
    assert_eq!(drive(&mut adapter, 5004), State::Unknown);
    *gate.lock().unwrap() = Some(Ok(()));
    assert_eq!(drive(&mut adapter, 5005), State::Disabled);
    assert_eq!(control.lock().unwrap().names().last(), Some(&Call::Unapp));
}

#[test]
fn failed_secure_close_cannot_report_quiescence_after_application_removal() {
    for error in [Error::Unknown, Error::Backend, Error::Deadline] {
        let (mut adapter, control) = fixture();
        let epoch = adapter.start(0).unwrap();
        drive(&mut adapter, 1);
        adapter.connected(epoch, peer(), 2).unwrap();
        control.lock().unwrap().expect(Call::Unadv, Some(Ok(())));
        control
            .lock()
            .unwrap()
            .expect(Call::Close, Some(Err(error)));
        adapter.stop(3).unwrap();
        assert_eq!(drive(&mut adapter, 4), State::Unknown);
        assert!(!adapter.has_pending_backend());
        assert!(!adapter.app_maybe && !adapter.adv_maybe && adapter.peer.is_none());
        assert!(
            !adapter.is_quiescent(),
            "unconfirmed security cleanup must remain owned"
        );
        assert_eq!(adapter.last_error(), Some(error));
        assert_eq!(adapter.start(5), Err(Error::Busy));
        let calls = control.lock().unwrap().names();
        assert_eq!(
            calls,
            vec![Call::App, Call::Adv, Call::Unadv, Call::Close, Call::Unapp]
        );
        adapter.stop(6).unwrap();
        assert_eq!(drive(&mut adapter, 7), State::Unknown);
        assert!(!adapter.is_quiescent());
        assert_eq!(control.lock().unwrap().names(), calls);
    }
}

#[test]
fn failed_secure_close_remains_owned_for_every_shutdown_route() {
    for route in ["stop", "disconnect", "owner_lost", "release"] {
        for error in [Error::Unknown, Error::Backend, Error::Deadline] {
            let (mut adapter, control) = fixture();
            let epoch = adapter.start(0).unwrap();
            drive(&mut adapter, 1);
            adapter.connected(epoch, peer(), 2).unwrap();
            let connection = adapter.connection().unwrap();
            if matches!(route, "stop" | "owner_lost") {
                control.lock().unwrap().expect(Call::Unadv, Some(Ok(())));
            }
            control
                .lock()
                .unwrap()
                .expect(Call::Close, Some(Err(error)));
            match route {
                "stop" => adapter.stop(3).unwrap(),
                "disconnect" => adapter.disconnected(&connection, 3).unwrap(),
                "owner_lost" => adapter.owner_lost(":1.42", 3).unwrap(),
                "release" => adapter.advertisement_released(epoch, ":1.42", 3).unwrap(),
                _ => unreachable!(),
            }
            assert_eq!(drive(&mut adapter, 4), State::Unknown, "{route}");
            assert!(!adapter.is_current(&connection));
            assert!(adapter.connection().is_none());
            assert!(!adapter.is_quiescent(), "{route}: {error:?}");
            assert!(!adapter.has_pending_backend());
            assert_eq!(adapter.last_error(), Some(error));
            let calls = control.lock().unwrap().names();
            assert_eq!(calls.iter().filter(|c| **c == Call::Close).count(), 1);
            assert_eq!(calls.iter().filter(|c| **c == Call::Unapp).count(), 1);
            assert!(adapter.start(5).is_err());
            adapter.stop(6).unwrap();
            assert_eq!(drive(&mut adapter, 7), State::Unknown);
            assert!(!adapter.is_quiescent());
            assert_eq!(control.lock().unwrap().names(), calls);
        }
    }
}

#[test]
fn timed_out_secure_close_requires_actual_late_success_to_be_quiescent() {
    for late in [
        Err(Error::Unknown),
        Err(Error::Backend),
        Err(Error::Deadline),
        Ok(()),
    ] {
        let (mut adapter, control) = fixture();
        let epoch = adapter.start(0).unwrap();
        drive(&mut adapter, 1);
        adapter.connected(epoch, peer(), 2).unwrap();
        control.lock().unwrap().expect(Call::Unadv, Some(Ok(())));
        let gate = control.lock().unwrap().expect(Call::Close, None);
        adapter.stop(3).unwrap();
        assert_eq!(drive(&mut adapter, 4), State::Stopping);
        assert_eq!(drive(&mut adapter, 5004), State::Unknown);
        assert!(adapter.has_pending_backend());
        assert!(!adapter.is_quiescent());
        *gate.lock().unwrap() = Some(late);
        let expected = if late.is_ok() {
            State::Disabled
        } else {
            State::Unknown
        };
        assert_eq!(drive(&mut adapter, 5005), expected);
        assert!(!adapter.has_pending_backend());
        assert_eq!(adapter.is_quiescent(), late.is_ok());
        let calls = control.lock().unwrap().names();
        assert_eq!(
            calls,
            vec![Call::App, Call::Adv, Call::Unadv, Call::Close, Call::Unapp]
        );
        assert_eq!(drive(&mut adapter, 5006), expected);
        assert_eq!(control.lock().unwrap().names(), calls);
    }
}

#[test]
fn foreign_epoch_owner_and_second_peer_cannot_overwrite_the_current_session() {
    let (mut adapter, control) = fixture();
    let epoch = adapter.start(0).unwrap();
    drive(&mut adapter, 1);
    let foreign_epoch = Epoch::new([8; 16], 1).unwrap();
    assert_eq!(
        adapter.connected(foreign_epoch, peer(), 2),
        Err(Error::StaleContext)
    );
    let foreign_owner = Peer::from_backend_callback(":1.43", peer().device_path()).unwrap();
    assert_eq!(
        adapter.connected(epoch, foreign_owner, 2),
        Err(Error::StaleContext)
    );
    adapter.connected(epoch, peer(), 2).unwrap();
    adapter.connected(epoch, peer(), 3).unwrap(); // Same callback is idempotent.
    let second = Peer::from_backend_callback(":1.42", "/org/bluez/hci0/dev_OTHER").unwrap();
    assert_eq!(adapter.connected(epoch, second, 3), Err(Error::Busy));
    assert!(is_peer_live(&adapter, epoch, &peer()));
    assert_eq!(
        adapter.advertisement_released(epoch, ":1.43", 3),
        Err(Error::StaleContext)
    );
    assert_eq!(adapter.owner_lost(":1.43", 3), Err(Error::StaleContext));
    adapter.owner_lost(":1.42", 4).unwrap();
    assert!(!is_peer_live(&adapter, epoch, &peer()));
    assert_eq!(drive(&mut adapter, 5), State::Disabled);
    assert_eq!(adapter.start(6), Err(Error::StaleContext));
    assert_eq!(
        control.lock().unwrap().names(),
        vec![Call::App, Call::Adv, Call::Unadv, Call::Close, Call::Unapp]
    );
}

#[test]
fn normal_disconnect_closes_secure_state_before_accepting_the_next_connection() {
    let (mut adapter, control) = fixture();
    let epoch = adapter.start(0).unwrap();
    drive(&mut adapter, 1);
    adapter.connected(epoch, peer(), 2).unwrap();
    let gate = control.lock().unwrap().expect(Call::Close, None);
    adapter
        .disconnected(&adapter.connection().unwrap(), 3)
        .unwrap();
    assert!(!is_peer_live(&adapter, epoch, &peer()));
    assert_eq!(adapter.connected(epoch, peer(), 4), Err(Error::Busy));
    *gate.lock().unwrap() = Some(Ok(()));
    assert_eq!(drive(&mut adapter, 5), State::Advertising);
    adapter.connected(epoch, peer(), 6).unwrap();
    adapter.stop(7).unwrap();
    drive(&mut adapter, 8);
}

#[test]
fn deadline_overflow_and_exhausted_start_counter_issue_no_backend_calls() {
    let (mut adapter, control) = fixture();
    assert_eq!(adapter.start(u64::MAX), Err(Error::Deadline));
    assert!(control.lock().unwrap().calls.is_empty());
    adapter.next_start = Some(u64::MAX);
    adapter.start(0).unwrap();
    drive(&mut adapter, 1);
    adapter.stop(2).unwrap();
    drive(&mut adapter, 3);
    assert_eq!(adapter.start(4), Err(Error::Exhausted));
    assert_eq!(control.lock().unwrap().calls.len(), 4);
}

#[test]
fn invalid_release_timestamp_cannot_modify_resource_ownership_before_validation() {
    let (mut adapter, control) = fixture();
    let epoch = adapter.start(10).unwrap();
    drive(&mut adapter, 11);
    assert_eq!(
        adapter.advertisement_released(epoch, ":1.42", 9),
        Err(Error::InvalidInput)
    );
    assert_eq!(adapter.state(), State::Advertising);
    adapter.stop(12).unwrap();
    drive(&mut adapter, 13);
    assert_eq!(
        control.lock().unwrap().names(),
        vec![Call::App, Call::Adv, Call::Unadv, Call::Unapp]
    );
}
