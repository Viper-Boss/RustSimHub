// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::manifest::{FixedAdvertisement, FixedManifest};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::Waker,
};

type ReplyGate = Arc<Mutex<Option<Result<SecureReply, Error>>>>;
type NotifyGate = Arc<Mutex<Option<Result<(), Error>>>>;

#[derive(Default)]
struct Control {
    calls: Vec<&'static str>,
    received: Vec<(Epoch, Peer, InputKind, Vec<u8>)>,
    notifications: Vec<(Epoch, Peer, Characteristic, Vec<u8>)>,
    secure_gate: Option<ReplyGate>,
    notify_gate: Option<NotifyGate>,
    reject_secure: bool,
}

#[derive(Clone)]
struct Mock(Arc<Mutex<Control>>);
struct Wait<T>(Arc<Mutex<Option<Result<T, Error>>>>);
impl<T> Future for Wait<T> {
    type Output = Result<T, Error>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        self.0
            .lock()
            .unwrap()
            .take()
            .map_or(Poll::Pending, Poll::Ready)
    }
}
fn ready<T: Send + 'static>(value: T) -> Pending<T> {
    Box::pin(async { Ok(value) })
}
impl GattBackendV1 for Mock {
    fn register_application(&mut self, _: Epoch, _: FixedManifest) -> Pending<()> {
        self.0.lock().unwrap().calls.push("app");
        ready(())
    }
    fn register_advertisement(&mut self, _: Epoch, _: FixedAdvertisement) -> Pending<()> {
        self.0.lock().unwrap().calls.push("adv");
        ready(())
    }
    fn unregister_advertisement(&mut self, _: Epoch) -> Pending<()> {
        self.0.lock().unwrap().calls.push("unadv");
        ready(())
    }
    fn unregister_application(&mut self, _: Epoch) -> Pending<()> {
        self.0.lock().unwrap().calls.push("unapp");
        ready(())
    }
    fn notify(
        &mut self,
        e: Epoch,
        peer: Peer,
        ch: Characteristic,
        bytes: OpaqueBytes,
    ) -> Pending<()> {
        let mut c = self.0.lock().unwrap();
        c.calls.push("notify");
        c.notifications.push((e, peer, ch, bytes.into_vec()));
        c.notify_gate
            .take()
            .map_or_else(|| ready(()), |g| Box::pin(Wait(g)) as Pending<()>)
    }
}
impl SecurePortV1 for Mock {
    fn receive(
        &mut self,
        e: Epoch,
        peer: Peer,
        kind: InputKind,
        bytes: OpaqueBytes,
    ) -> Pending<SecureReply> {
        let mut c = self.0.lock().unwrap();
        c.calls.push("secure");
        c.received.push((e, peer, kind, bytes.into_vec()));
        if let Some(g) = c.secure_gate.take() {
            return Box::pin(Wait(g));
        }
        if c.reject_secure {
            return Box::pin(async { Err(Error::Unavailable) });
        }
        let bytes = OpaqueBytes::new(b"synthetic-opaque-reply".to_vec()).unwrap();
        ready(match kind {
            InputKind::Auth => SecureReply::Handshake(bytes),
            InputKind::Command => SecureReply::Application(bytes),
        })
    }
    fn disconnected(&mut self, _: Epoch, _: Peer) -> Pending<()> {
        self.0.lock().unwrap().calls.push("close");
        ready(())
    }
}
fn fixture() -> (Transport<Mock, Mock>, Arc<Mutex<Control>>) {
    let control = Arc::new(Mutex::new(Control::default()));
    let mock = Mock(control.clone());
    (
        Transport::new(mock.clone(), Some(mock), ":1.42", [7; 16]).unwrap(),
        control,
    )
}
fn drive(t: &mut Transport<Mock, Mock>, now: u64) -> State {
    t.poll(&mut Context::from_waker(Waker::noop()), now)
        .unwrap()
}
fn connect(t: &mut Transport<Mock, Mock>) -> Connection {
    let epoch = t.start(0).unwrap();
    drive(t, 1);
    t.connected(
        epoch,
        Peer::from_backend_callback(":1.42", "/org/bluez/hci0/dev_SYNTHETIC").unwrap(),
        2,
    )
    .unwrap()
}
fn frame(kind: u8, id: u16, index: u16, count: u16, total: u16, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![b'R', b'B', 1, kind];
    for word in [id, index, count, total] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend_from_slice(payload);
    bytes
}
fn write(
    t: &mut Transport<Mock, Mock>,
    connection: &Connection,
    id: u16,
    kind: InputKind,
    now: u64,
) -> Result<(), Error> {
    t.write(
        connection,
        kind.characteristic(),
        WriteOptions::REQUEST,
        now,
        &frame(kind as u8, id, 0, 1, 3, b"abc"),
    )
}

#[test]
fn actual_transport_routes_only_injected_secure_results_and_erases_session_cache() {
    let (mut t, c) = fixture();
    let connection = connect(&mut t);
    assert!(matches!(
        t.read_result(&connection, 0),
        Err(Error::Unavailable)
    ));
    write(&mut t, &connection, 1, InputKind::Auth, 3).unwrap();
    drive(&mut t, 4);
    assert!(matches!(
        t.read_result(&connection, 0),
        Err(Error::Unavailable)
    ));
    write(&mut t, &connection, 2, InputKind::Command, 5).unwrap();
    drive(&mut t, 6);
    let result = t.read_result(&connection, 0).unwrap();
    assert_eq!(result.as_slice()[3], 3);
    assert!(matches!(
        t.read_result(&connection, 1),
        Err(Error::InvalidInput)
    ));
    let log = c.lock().unwrap();
    assert_eq!(log.received.len(), 2);
    assert_eq!(log.received[0].2, InputKind::Auth);
    assert_eq!(log.received[1].2, InputKind::Command);
    assert_eq!(log.received[0].3, b"abc");
    assert!(log
        .notifications
        .iter()
        .all(|(e, p, _, bytes)| *e == connection.epoch()
            && p == connection.peer()
            && bytes.len() <= 20));
    assert!(log
        .notifications
        .iter()
        .any(|(_, _, ch, _)| *ch == Characteristic::AuthExchange));
    assert!(log
        .notifications
        .iter()
        .any(|(_, _, ch, _)| *ch == Characteristic::Result));
    drop(log);
    t.disconnected(&connection, 7).unwrap();
    drive(&mut t, 8);
    let new = t
        .connected(connection.epoch(), connection.peer().clone(), 9)
        .unwrap();
    assert!(matches!(
        t.read_result(&connection, 0),
        Err(Error::StaleContext)
    ));
    assert!(matches!(t.read_result(&new, 0), Err(Error::Unavailable)));
    assert_eq!(
        write(&mut t, &connection, 3, InputKind::Command, 10),
        Err(Error::StaleContext)
    );
    t.stop(11).unwrap();
    drive(&mut t, 12);
    assert!(t.is_quiescent());
}

#[test]
fn absent_or_rejecting_security_port_has_no_command_success_fallback() {
    let c = Arc::new(Mutex::new(Control::default()));
    let mock = Mock(c.clone());
    assert!(matches!(
        Transport::new(mock, None::<Mock>, ":1.42", [7; 16]),
        Err(Error::Unavailable)
    ));
    assert!(c.lock().unwrap().calls.is_empty());
    let (mut t, c) = fixture();
    let connection = connect(&mut t);
    c.lock().unwrap().reject_secure = true;
    write(&mut t, &connection, 1, InputKind::Command, 3).unwrap();
    drive(&mut t, 4);
    assert_eq!(t.last_error(), Some(Error::Unavailable));
    assert!(c.lock().unwrap().notifications.is_empty());
    assert!(matches!(
        t.read_result(&connection, 0),
        Err(Error::StaleContext)
    ));
    t.stop(5).unwrap();
    drive(&mut t, 6);
}

#[test]
fn partial_assembly_expires_without_another_frame_and_never_calls_security() {
    let (mut t, c) = fixture();
    let connection = connect(&mut t);
    t.write(
        &connection,
        Characteristic::AuthExchange,
        WriteOptions::REQUEST,
        100,
        &frame(1, 1, 0, 2, 9, b"abcdefgh"),
    )
    .unwrap();
    assert_eq!(t.deadline_ms(), Some(15100));
    drive(&mut t, 15099);
    assert!(c.lock().unwrap().received.is_empty());
    drive(&mut t, 15100);
    assert_eq!(t.last_error(), Some(Error::Deadline));
    assert!(c.lock().unwrap().received.is_empty());
    assert!(c.lock().unwrap().notifications.is_empty());
    assert_eq!(c.lock().unwrap().calls.last(), Some(&"close"));
    t.stop(15101).unwrap();
    drive(&mut t, 15102);
}

#[test]
fn late_secure_result_after_timeout_is_retained_but_never_notified_or_replayed() {
    let (mut t, c) = fixture();
    let connection = connect(&mut t);
    let gate = Arc::new(Mutex::new(None));
    c.lock().unwrap().secure_gate = Some(gate.clone());
    write(&mut t, &connection, 1, InputKind::Command, 3).unwrap();
    assert_eq!(drive(&mut t, 5003), State::Unknown);
    assert_eq!(Arc::strong_count(&gate), 2);
    assert!(matches!(
        t.connected(connection.epoch(), connection.peer().clone(), 5004),
        Err(Error::Busy)
    ));
    *gate.lock().unwrap() = Some(Ok(SecureReply::Application(
        OpaqueBytes::new(b"late-secret".to_vec()).unwrap(),
    )));
    drive(&mut t, 5005);
    assert_eq!(Arc::strong_count(&gate), 1);
    assert!(c.lock().unwrap().notifications.is_empty());
    assert_eq!(c.lock().unwrap().received.len(), 1);
    let new = t
        .connected(connection.epoch(), connection.peer().clone(), 5006)
        .unwrap();
    assert!(matches!(t.read_result(&new, 0), Err(Error::Unavailable)));
    t.stop(5007).unwrap();
    drive(&mut t, 5008);
}

#[test]
fn unconfirmed_notification_times_out_without_retry_or_readable_success_cache() {
    let (mut t, c) = fixture();
    let connection = connect(&mut t);
    let gate = Arc::new(Mutex::new(None));
    c.lock().unwrap().notify_gate = Some(gate.clone());
    write(&mut t, &connection, 1, InputKind::Command, 3).unwrap();
    drive(&mut t, 4);
    assert!(matches!(
        t.read_result(&connection, 0),
        Err(Error::Unavailable)
    ));
    assert_eq!(drive(&mut t, 5004), State::Unknown);
    assert_eq!(c.lock().unwrap().notifications.len(), 1);
    *gate.lock().unwrap() = Some(Ok(()));
    drive(&mut t, 5005);
    assert_eq!(c.lock().unwrap().notifications.len(), 1);
    assert!(matches!(
        t.read_result(&connection, 0),
        Err(Error::StaleContext)
    ));
    t.stop(5006).unwrap();
    drive(&mut t, 5007);
}

#[test]
fn flood_during_inflight_secure_work_closes_connection_without_cancelling_or_reissuing_it() {
    let (mut t, c) = fixture();
    let connection = connect(&mut t);
    let gate = Arc::new(Mutex::new(None));
    c.lock().unwrap().secure_gate = Some(gate.clone());
    write(&mut t, &connection, 1, InputKind::Auth, 3).unwrap();
    assert_eq!(
        write(&mut t, &connection, 2, InputKind::Auth, 4),
        Err(Error::Busy)
    );
    *gate.lock().unwrap() = Some(Ok(SecureReply::Handshake(
        OpaqueBytes::new(b"late".to_vec()).unwrap(),
    )));
    drive(&mut t, 5);
    assert_eq!(c.lock().unwrap().received.len(), 1);
    assert!(c.lock().unwrap().notifications.is_empty());
    assert_eq!(t.last_error(), Some(Error::Busy));
    t.stop(6).unwrap();
    drive(&mut t, 7);
}

#[test]
fn stop_with_late_work_cleans_registration_but_cannot_claim_quiescence_early() {
    let (mut t, c) = fixture();
    let connection = connect(&mut t);
    let gate = Arc::new(Mutex::new(None));
    c.lock().unwrap().secure_gate = Some(gate.clone());
    write(&mut t, &connection, 1, InputKind::Command, 3).unwrap();
    t.stop(4).unwrap();
    assert_eq!(drive(&mut t, 5), State::Unknown);
    assert!(!t.is_quiescent());
    assert_eq!(t.start(6), Err(Error::Busy));
    assert_eq!(
        c.lock().unwrap().calls,
        vec!["app", "adv", "secure", "unadv", "close", "unapp"]
    );
    *gate.lock().unwrap() = Some(Ok(SecureReply::Application(
        OpaqueBytes::new(b"late".to_vec()).unwrap(),
    )));
    assert_eq!(drive(&mut t, 7), State::Disabled);
    assert!(t.is_quiescent());
    assert!(c.lock().unwrap().notifications.is_empty());
}

#[test]
fn wrong_reply_class_and_duplicated_fragment_close_instead_of_publishing() {
    let (mut t, c) = fixture();
    let connection = connect(&mut t);
    let gate = Arc::new(Mutex::new(Some(Ok(SecureReply::Application(
        OpaqueBytes::new(b"wrong-kind".to_vec()).unwrap(),
    )))));
    c.lock().unwrap().secure_gate = Some(gate);
    write(&mut t, &connection, 1, InputKind::Auth, 3).unwrap();
    drive(&mut t, 4);
    assert_eq!(t.last_error(), Some(Error::Protocol));
    assert!(c.lock().unwrap().notifications.is_empty());
    let new = t
        .connected(connection.epoch(), connection.peer().clone(), 5)
        .unwrap();
    let first = frame(1, 1, 0, 2, 9, b"abcdefgh");
    t.write(
        &new,
        Characteristic::AuthExchange,
        WriteOptions::REQUEST,
        6,
        &first,
    )
    .unwrap();
    assert_eq!(
        t.write(
            &new,
            Characteristic::AuthExchange,
            WriteOptions::REQUEST,
            7,
            &first
        ),
        Err(Error::Protocol)
    );
    assert_eq!(c.lock().unwrap().received.len(), 1); // No second security request.
    drive(&mut t, 8);
    t.stop(9).unwrap();
    drive(&mut t, 10);
}

#[test]
fn second_peer_and_owner_loss_cannot_inherit_replies_or_auto_restart() {
    let (mut t, c) = fixture();
    let connection = connect(&mut t);
    let second = Peer::from_backend_callback(":1.42", "/org/bluez/hci0/dev_OTHER").unwrap();
    assert!(matches!(
        t.connected(connection.epoch(), second, 3),
        Err(Error::Busy)
    ));
    let gate = Arc::new(Mutex::new(None));
    c.lock().unwrap().secure_gate = Some(gate.clone());
    write(&mut t, &connection, 1, InputKind::Command, 4).unwrap();
    t.owner_lost(":1.42", 5).unwrap();
    drive(&mut t, 6);
    *gate.lock().unwrap() = Some(Ok(SecureReply::Application(
        OpaqueBytes::new(b"old-owner".to_vec()).unwrap(),
    )));
    drive(&mut t, 7);
    assert!(c.lock().unwrap().notifications.is_empty());
    assert!(t.is_quiescent());
    assert_eq!(t.start(8), Err(Error::StaleContext));
}
