// SPDX-License-Identifier: AGPL-3.0-only
//! Review-only transport experiments. No production AcquireNotify/notify adapter.
use super::*;
use futures_lite::StreamExt;
use std::{io::ErrorKind, os::unix::net::UnixDatagram, sync::Mutex};
use zbus::{zvariant::OwnedFd, MessageStream};

const PATH: &str = "/org/rustsimhub/test_notify";
const INTERFACE: &str = "org.rustsimhub.TestNotificationRoute";

async fn value_stream(connection: &Connection) -> MessageStream {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .path(Characteristic::Result.path())
        .unwrap()
        .interface("org.freedesktop.DBus.Properties")
        .unwrap()
        .member("PropertiesChanged")
        .unwrap()
        .arg(0, "org.bluez.GattCharacteristic1")
        .unwrap()
        .build();
    MessageStream::for_match_rule(rule, connection, Some(4))
        .await
        .unwrap()
}
async fn emit_value(connection: &Connection) -> zbus::Result<()> {
    connection
        .emit_signal(
            None::<&str>,
            Characteristic::Result.path(),
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &(
                "org.bluez.GattCharacteristic1",
                HashMap::from([("Value", Value::new(frame()))]),
                Vec::<&str>::new(),
            ),
        )
        .await
}

#[tokio::test]
async fn actual_broadcast_properties_changed_reaches_both_bus_subscribers() {
    let bus = Bus::new().await;
    let sender = bus.connect().await;
    let first = bus.connect().await;
    let second = bus.connect().await;
    let mut a = value_stream(&first).await;
    let mut b = value_stream(&second).await;
    bounded(emit_value(&sender)).await.unwrap();
    for stream in [&mut a, &mut b] {
        let message = bounded(stream.next()).await.unwrap().unwrap();
        assert_eq!(
            message.header().sender().unwrap(),
            sender.unique_name().unwrap()
        );
        let (interface, mut values, invalidated): (
            String,
            HashMap<String, OwnedValue>,
            Vec<String>,
        ) = message.body().deserialize().unwrap();
        assert_eq!(interface, "org.bluez.GattCharacteristic1");
        assert!(invalidated.is_empty());
        assert_eq!(
            Vec::<u8>::try_from(values.remove("Value").unwrap()).unwrap(),
            frame()
        );
    }
}

#[tokio::test]
async fn successful_signal_send_does_not_require_any_notification_subscriber() {
    let bus = Bus::new().await;
    let sender = bus.connect().await;
    // This private bus has no other connection at all. The actual API still
    // returns Ok; it cannot attest any BlueZ/ATT/browser/application receipt.
    bounded(emit_value(&sender)).await.unwrap();
    bounded(zbus::fdo::DBusProxy::new(&sender).await.unwrap().get_id())
        .await
        .unwrap();
}

/// Synthetic candidate only at a TEST path/name. Reuses actual production
/// owner/Device1 lease fences, but does not implement org.bluez.AcquireNotify.
struct Candidate {
    publication: Arc<Publication>,
    writer: Mutex<Option<UnixDatagram>>,
}
#[zbus::interface(name = "org.rustsimhub.TestNotificationRoute")]
impl Candidate {
    async fn acquire_for_peer(
        &self,
        device: OwnedObjectPath,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<OwnedFd> {
        let owner = self
            .publication
            .check(header.sender().map(|s| s.as_str()))
            .await
            .map_err(|_| denied())?;
        let peer = Peer::from_backend_callback(&owner, device.as_str()).map_err(|_| invalid())?;
        if !self.publication.matches_peer(&peer) {
            return Err(denied());
        }
        let mut writer = self.writer.lock().map_err(|_| unavailable())?;
        if writer.is_some() {
            return Err(unavailable());
        }
        let (tx, rx) = UnixDatagram::pair().map_err(|_| unavailable())?;
        tx.set_nonblocking(true).map_err(|_| unavailable())?;
        rx.set_nonblocking(true).map_err(|_| unavailable())?;
        *writer = Some(tx);
        Ok(std::os::fd::OwnedFd::from(rx).into())
    }
}
impl Candidate {
    async fn send(&self, bytes: &[u8], owner: &str) -> Result<(), Error> {
        self.publication.check(Some(owner)).await?;
        let writer = self.writer.lock().map_err(|_| Error::Backend)?;
        let count = writer
            .as_ref()
            .ok_or(Error::Unavailable)?
            .send(bytes)
            .map_err(|_| Error::Backend)?;
        if count != bytes.len() {
            return Err(Error::Protocol);
        }
        Ok(())
    }
}
async fn install(control: &BluezControl, export: &GattExport) {
    assert!(control
        .connection()
        .object_server()
        .at(
            PATH,
            Candidate {
                publication: export.publication.clone(),
                writer: Mutex::new(None),
            }
        )
        .await
        .unwrap());
}
async fn acquire(sender: &Connection, target: &str, device: &str) -> zbus::Result<UnixDatagram> {
    let proxy = Proxy::new(sender, target, PATH, INTERFACE).await?;
    let fd: OwnedFd = proxy
        .call(
            "AcquireForPeer",
            &(OwnedObjectPath::try_from(device).unwrap(),),
        )
        .await?;
    Ok(UnixDatagram::from(std::os::fd::OwnedFd::from(fd)))
}
async fn send(control: &BluezControl) -> Result<(), Error> {
    let connection = control.connection();
    let interface = connection
        .object_server()
        .interface::<_, Candidate>(PATH)
        .await
        .unwrap();
    let candidate = interface.get().await;
    candidate.send(&frame(), control.owner()).await
}

#[tokio::test]
async fn actual_unix_fd_transfer_routes_packet_only_to_acquired_endpoint() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, _requests) = publish(&control, epoch(1)).await.unwrap();
    install(&control, &export).await;
    let receiver = bounded(acquire(&bluez, &name, DEVICE)).await.unwrap();
    let (_unrelated_tx, unrelated_rx) = UnixDatagram::pair().unwrap();
    unrelated_rx.set_nonblocking(true).unwrap();
    bounded(send(&control)).await.unwrap();
    let mut bytes = [0; 244];
    let n = receiver.recv(&mut bytes).unwrap();
    assert_eq!(&bytes[..n], frame());
    assert_eq!(
        unrelated_rx.recv(&mut bytes).unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn real_foreign_sender_and_second_device_cannot_acquire_candidate_fd() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, _requests) = publish(&control, epoch(1)).await.unwrap();
    install(&control, &export).await;
    let stranger = bus.connect().await;
    assert!(bounded(acquire(&stranger, &name, DEVICE)).await.is_err());
    assert!(bounded(acquire(
        &bluez,
        &name,
        "/org/bluez/hci0/dev_11_22_33_44_55_66"
    ))
    .await
    .is_err());
    let _receiver = bounded(acquire(&bluez, &name, DEVICE)).await.unwrap();
    assert!(bounded(acquire(&bluez, &name, DEVICE)).await.is_err());
}

#[tokio::test]
async fn peer_reconnect_fences_candidate_fd_before_any_packet_can_be_submitted() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, _requests) = publish(&control, epoch(1)).await.unwrap();
    install(&control, &export).await;
    let receiver = bounded(acquire(&bluez, &name, DEVICE)).await.unwrap();
    crate::bluez_peer::tests::reconnect_same_path(&bluez).await;
    assert_eq!(bounded(send(&control)).await, Err(Error::StaleContext));
    assert_eq!(
        receiver.recv(&mut [0; 244]).unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn receiver_shutdown_is_not_converted_to_send_success_or_broadcast_fallback() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, _requests) = publish(&control, epoch(1)).await.unwrap();
    install(&control, &export).await;
    let observer = bus.connect().await;
    let mut values = value_stream(&observer).await;
    let receiver = bounded(acquire(&bluez, &name, DEVICE)).await.unwrap();
    // A transferred descriptor can still have a transport-owned duplicate.
    // Closing this copy alone does not establish a dead receiving endpoint.
    // Explicit shutdown establishes that precondition even with a live dup;
    // no sleeps/retries or successful notification are used to drain the bus.
    let in_flight = receiver.try_clone().unwrap();
    receiver.shutdown(std::net::Shutdown::Both).unwrap();
    drop(receiver);
    assert_eq!(bounded(send(&control)).await, Err(Error::Backend));
    drop(in_flight);
    assert!(futures_lite::future::poll_once(values.next())
        .await
        .is_none());
}
