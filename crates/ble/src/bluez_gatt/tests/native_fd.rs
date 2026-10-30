// SPDX-License-Identifier: AGPL-3.0-only
//! Actual production AcquireNotify and kernel SEQPACKET endpoints, private bus.
use super::*;
use socket2::{Socket, Type};
use std::{
    io::{self, Read},
    os::fd::AsRawFd,
};
use zbus::zvariant::OwnedFd;

fn acquire_options(mtu: u16) -> HashMap<String, OwnedValue> {
    let mut values = options(false);
    values.insert("mtu".into(), OwnedValue::from(mtu));
    values.insert("link".into(), Value::from("LE").try_into().unwrap());
    values
}
async fn acquire(
    sender: &Connection,
    target: &str,
    kind: Characteristic,
    values: HashMap<String, OwnedValue>,
) -> zbus::Result<(Socket, u16)> {
    let proxy = Proxy::new(sender, target, kind.path(), "org.bluez.GattCharacteristic1").await?;
    let (fd, mtu): (OwnedFd, u16) = proxy.call("AcquireNotify", &(values,)).await?;
    Ok((Socket::from(std::os::fd::OwnedFd::from(fd)), mtu))
}
fn peer(bluez: &Connection) -> Peer {
    Peer::from_backend_callback(bluez.unique_name().unwrap().as_str(), DEVICE).unwrap()
}
fn output(kind: Characteristic, id: u8) -> OpaqueBytes {
    let mut bytes = frame();
    bytes[3] = if kind == Characteristic::Result { 3 } else { 1 };
    bytes[4] = id;
    OpaqueBytes::new(bytes).unwrap()
}
fn receive(socket: &Socket) -> io::Result<Vec<u8>> {
    let mut bytes = [0; 512];
    let mut socket = socket;
    let count = socket.read(&mut bytes)?;
    Ok(bytes[..count].to_vec())
}
fn no_packet(socket: &Socket) {
    match receive(socket) {
        Ok(bytes) => assert!(bytes.is_empty(), "unexpected packet"),
        Err(error) => assert!(
            matches!(
                error.kind(),
                io::ErrorKind::WouldBlock
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::NotConnected
                    | io::ErrorKind::BrokenPipe
            ),
            "unexpected socket error: {error}"
        ),
    }
}
async fn properties(
    sender: &Connection,
    target: &str,
    kind: Characteristic,
) -> HashMap<String, OwnedValue> {
    Proxy::new(
        sender,
        target,
        kind.path(),
        "org.freedesktop.DBus.Properties",
    )
    .await
    .unwrap()
    .call("GetAll", &("org.bluez.GattCharacteristic1",))
    .await
    .unwrap()
}

#[tokio::test]
async fn optional_fd_properties_exist_only_on_frozen_notify_characteristics() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, _requests) = publish(&control, epoch(1)).await.unwrap();
    for kind in Characteristic::ALL {
        let values = bounded(properties(&bluez, &name, kind)).await;
        let notify = matches!(kind, Characteristic::AuthExchange | Characteristic::Result);
        assert_eq!(values.contains_key("NotifyAcquired"), notify);
        assert_eq!(values.contains_key("Notifying"), notify);
        assert!(!values.contains_key("Value"));
        if notify {
            assert!(!bool::try_from(&values["NotifyAcquired"]).unwrap());
        }
    }
}

#[tokio::test]
async fn actual_acquire_transfers_cloexec_nonblocking_seqpacket_and_obeys_mtu() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, _requests) = publish(&control, epoch(1)).await.unwrap();
    let (socket, mtu) = bounded(acquire(
        &bluez,
        &name,
        Characteristic::AuthExchange,
        acquire_options(517),
    ))
    .await
    .unwrap();
    assert_eq!(mtu, 247);
    assert_eq!(socket.r#type().unwrap(), Type::SEQPACKET);
    let info =
        std::fs::read_to_string(format!("/proc/self/fdinfo/{}", socket.as_raw_fd())).unwrap();
    let flags = u64::from_str_radix(
        info.lines()
            .find_map(|l| l.strip_prefix("flags:\t"))
            .unwrap(),
        8,
    )
    .unwrap();
    assert_ne!(flags & libc::O_NONBLOCK as u64, 0);
    assert_ne!(flags & libc::O_CLOEXEC as u64, 0);
    let values = bounded(properties(&bluez, &name, Characteristic::AuthExchange)).await;
    assert!(bool::try_from(&values["NotifyAcquired"]).unwrap());
    assert!(bool::try_from(&values["Notifying"]).unwrap());
}

#[tokio::test]
async fn two_packets_and_two_characteristics_keep_exact_endpoint_and_packet_boundaries() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, _requests) = publish(&control, epoch(1)).await.unwrap();
    let (auth, _) = acquire(
        &bluez,
        &name,
        Characteristic::AuthExchange,
        acquire_options(23),
    )
    .await
    .unwrap();
    let (result, _) = acquire(&bluez, &name, Characteristic::Result, acquire_options(23))
        .await
        .unwrap();
    for id in [1, 2] {
        bounded(export.notify(
            epoch(1),
            peer(&bluez),
            Characteristic::AuthExchange,
            output(Characteristic::AuthExchange, id),
        ))
        .await
        .unwrap();
    }
    assert_eq!(
        receive(&auth).unwrap(),
        output(Characteristic::AuthExchange, 1).as_slice()
    );
    assert_eq!(
        receive(&auth).unwrap(),
        output(Characteristic::AuthExchange, 2).as_slice()
    );
    no_packet(&result);
    bounded(export.notify(
        epoch(1),
        peer(&bluez),
        Characteristic::Result,
        output(Characteristic::Result, 1),
    ))
    .await
    .unwrap();
    assert_eq!(
        receive(&result).unwrap(),
        output(Characteristic::Result, 1).as_slice()
    );
    no_packet(&auth);
}

#[tokio::test]
async fn foreign_sender_second_peer_and_invalid_options_cannot_reserve_a_channel() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, _requests) = publish(&control, epoch(1)).await.unwrap();
    let stranger = bus.connect().await;
    assert!(bounded(acquire(
        &stranger,
        &name,
        Characteristic::AuthExchange,
        acquire_options(23)
    ))
    .await
    .is_err());
    let mut second = acquire_options(23);
    second.insert(
        "device".into(),
        object_value("/org/bluez/hci0/dev_11_22_33_44_55_66"),
    );
    assert!(
        bounded(acquire(&bluez, &name, Characteristic::AuthExchange, second))
            .await
            .is_err()
    );
    let mut malformed = Vec::new();
    for key in ["device", "mtu"] {
        let mut values = acquire_options(23);
        values.remove(key);
        malformed.push(values);
    }
    for (key, value) in [
        ("mtu", OwnedValue::from(23u32)),
        ("mtu", OwnedValue::from(22u16)),
        ("offset", OwnedValue::from(0u16)),
        ("unknown", OwnedValue::from(false)),
        ("link", Value::from("BR/EDR").try_into().unwrap()),
    ] {
        let mut values = acquire_options(23);
        values.insert(key.into(), value);
        malformed.push(values);
    }
    for values in malformed {
        assert!(
            bounded(acquire(&bluez, &name, Characteristic::AuthExchange, values))
                .await
                .is_err()
        );
    }
    let (_socket, _) = bounded(acquire(
        &bluez,
        &name,
        Characteristic::AuthExchange,
        acquire_options(23),
    ))
    .await
    .unwrap();
}

#[tokio::test]
async fn duplicate_and_non_notify_characteristics_never_open_another_fd() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, _requests) = publish(&control, epoch(1)).await.unwrap();
    for kind in [Characteristic::DeviceInfo, Characteristic::Command] {
        assert!(bounded(acquire(&bluez, &name, kind, acquire_options(23)))
            .await
            .is_err());
    }
    let (_socket, _) = acquire(
        &bluez,
        &name,
        Characteristic::AuthExchange,
        acquire_options(23),
    )
    .await
    .unwrap();
    let error = acquire(
        &bluez,
        &name,
        Characteristic::AuthExchange,
        acquire_options(23),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error,zbus::Error::MethodError(ref name,_,_) if name.as_str()=="org.bluez.Error.Failed")
    );
}

#[tokio::test]
async fn missing_fd_wrong_epoch_peer_kind_and_oversized_frame_never_submit_data() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, _requests) = publish(&control, epoch(1)).await.unwrap();
    assert_eq!(
        bounded(export.notify(
            epoch(1),
            peer(&bluez),
            Characteristic::AuthExchange,
            output(Characteristic::AuthExchange, 1)
        ))
        .await,
        Err(Error::Unavailable)
    );
    let (socket, _) = acquire(
        &bluez,
        &name,
        Characteristic::AuthExchange,
        acquire_options(23),
    )
    .await
    .unwrap();
    assert_eq!(
        bounded(export.notify(
            epoch(2),
            peer(&bluez),
            Characteristic::AuthExchange,
            output(Characteristic::AuthExchange, 1)
        ))
        .await,
        Err(Error::StaleContext)
    );
    let wrong =
        Peer::from_backend_callback(control.owner(), "/org/bluez/hci0/dev_11_22_33_44_55_66")
            .unwrap();
    assert_eq!(
        bounded(export.notify(
            epoch(1),
            wrong,
            Characteristic::AuthExchange,
            output(Characteristic::AuthExchange, 1)
        ))
        .await,
        Err(Error::StaleContext)
    );
    for bytes in [
        vec![1],
        vec![0; 15],
        output(Characteristic::Result, 1).into_vec(),
        vec![0; 245],
    ] {
        assert_eq!(
            bounded(export.notify(
                epoch(1),
                peer(&bluez),
                Characteristic::AuthExchange,
                OpaqueBytes::new(bytes).unwrap()
            ))
            .await,
            Err(Error::Protocol)
        );
    }
    let mut oversized = output(Characteristic::AuthExchange, 1).into_vec();
    oversized.resize(21, 0);
    assert_eq!(
        bounded(export.notify(
            epoch(1),
            peer(&bluez),
            Characteristic::AuthExchange,
            OpaqueBytes::new(oversized).unwrap()
        ))
        .await,
        Err(Error::Protocol)
    );
    no_packet(&socket);
    bounded(export.notify(
        epoch(1),
        peer(&bluez),
        Characteristic::AuthExchange,
        output(Characteristic::AuthExchange, 1),
    ))
    .await
    .unwrap();
    assert_eq!(
        receive(&socket).unwrap(),
        output(Characteristic::AuthExchange, 1).as_slice()
    );
}

#[tokio::test]
async fn receiver_hup_is_visible_without_any_new_frame_and_cannot_reacquire() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, _requests) = publish(&control, epoch(1)).await.unwrap();
    let (socket, _) = acquire(
        &bluez,
        &name,
        Characteristic::AuthExchange,
        acquire_options(23),
    )
    .await
    .unwrap();
    drop(socket);
    assert_eq!(
        bounded(export.check_peer_current()).await,
        Err(Error::Unavailable)
    );
    assert!(bounded(acquire(
        &bluez,
        &name,
        Characteristic::AuthExchange,
        acquire_options(23)
    ))
    .await
    .is_err());
    let values = bounded(properties(&bluez, &name, Characteristic::AuthExchange)).await;
    assert!(!bool::try_from(&values["NotifyAcquired"]).unwrap());
}

#[tokio::test]
async fn unexpected_reverse_bytes_cannot_become_command_input_or_allow_a_send() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, mut requests) = publish(&control, epoch(1)).await.unwrap();
    let (socket, _) = acquire(
        &bluez,
        &name,
        Characteristic::AuthExchange,
        acquire_options(23),
    )
    .await
    .unwrap();
    socket.send_with_flags(&[0x7f], libc::MSG_NOSIGNAL).unwrap();
    assert_eq!(
        bounded(export.notify(
            epoch(1),
            peer(&bluez),
            Characteristic::AuthExchange,
            output(Characteristic::AuthExchange, 1)
        ))
        .await,
        Err(Error::Protocol)
    );
    no_packet(&socket);
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn unbound_metadata_never_opens_fd_but_can_bind_once_then_acquire() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, _requests) = GattExport::publish(&control, epoch(1)).await.unwrap();
    assert!(bounded(acquire(
        &bluez,
        &name,
        Characteristic::AuthExchange,
        acquire_options(23)
    ))
    .await
    .is_err());
    export
        .bind_peer(PeerLease::bind(&control, epoch(1), DEVICE).await.unwrap())
        .await
        .unwrap();
    let (_socket, _) = bounded(acquire(
        &bluez,
        &name,
        Characteristic::AuthExchange,
        acquire_options(23),
    ))
    .await
    .unwrap();
}

#[tokio::test]
async fn extra_method_fields_are_rejected_before_fd_reservation() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, _requests) = publish(&control, epoch(1)).await.unwrap();
    let proxy = Proxy::new(
        &bluez,
        name.as_str(),
        Characteristic::AuthExchange.path(),
        "org.bluez.GattCharacteristic1",
    )
    .await
    .unwrap();
    assert!(bounded(
        proxy.call::<_, _, (OwnedFd, u16)>("AcquireNotify", &(acquire_options(23), true))
    )
    .await
    .is_err());
    let (_socket, _) = bounded(acquire(
        &bluez,
        &name,
        Characteristic::AuthExchange,
        acquire_options(23),
    ))
    .await
    .unwrap();
}

#[tokio::test]
async fn unpolled_notify_has_no_slot_or_channel_effect() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, _requests) = publish(&control, epoch(1)).await.unwrap();
    let (socket, _) = acquire(
        &bluez,
        &name,
        Characteristic::AuthExchange,
        acquire_options(23),
    )
    .await
    .unwrap();
    drop(export.notify(
        epoch(1),
        peer(&bluez),
        Characteristic::AuthExchange,
        output(Characteristic::AuthExchange, 1),
    ));
    no_packet(&socket);
    bounded(export.notify(
        epoch(1),
        peer(&bluez),
        Characteristic::AuthExchange,
        output(Characteristic::AuthExchange, 1),
    ))
    .await
    .unwrap();
    assert_eq!(
        receive(&socket).unwrap(),
        output(Characteristic::AuthExchange, 1).as_slice()
    );
}

#[tokio::test]
async fn cancelling_started_notify_retires_channel_instead_of_reusing_uncertain_work() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, _requests) = publish(&control, epoch(1)).await.unwrap();
    let (socket, _) = acquire(
        &bluez,
        &name,
        Characteristic::AuthExchange,
        acquire_options(23),
    )
    .await
    .unwrap();
    let mut sending = export.notify(
        epoch(1),
        peer(&bluez),
        Characteristic::AuthExchange,
        output(Characteristic::AuthExchange, 1),
    );
    assert!(futures_lite::future::poll_once(&mut sending)
        .await
        .is_none());
    drop(sending);
    no_packet(&socket);
    assert_eq!(
        bounded(export.notify(
            epoch(1),
            peer(&bluez),
            Characteristic::AuthExchange,
            output(Characteristic::AuthExchange, 2)
        ))
        .await,
        Err(Error::Unavailable)
    );
}

#[tokio::test]
async fn only_one_owned_notification_can_be_inflight() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, _requests) = publish(&control, epoch(1)).await.unwrap();
    let (socket, _) = acquire(
        &bluez,
        &name,
        Characteristic::AuthExchange,
        acquire_options(23),
    )
    .await
    .unwrap();
    let mut first = export.notify(
        epoch(1),
        peer(&bluez),
        Characteristic::AuthExchange,
        output(Characteristic::AuthExchange, 1),
    );
    assert!(futures_lite::future::poll_once(&mut first).await.is_none());
    assert_eq!(
        bounded(export.notify(
            epoch(1),
            peer(&bluez),
            Characteristic::AuthExchange,
            output(Characteristic::AuthExchange, 2)
        ))
        .await,
        Err(Error::Busy)
    );
    bounded(first).await.unwrap();
    assert_eq!(
        receive(&socket).unwrap(),
        output(Characteristic::AuthExchange, 1).as_slice()
    );
    no_packet(&socket);
}

#[tokio::test]
async fn export_retire_or_drop_fences_owned_future_and_shuts_the_fd() {
    for drop_export in [false, true] {
        let bus = Bus::new().await;
        let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
        let (control, name) = setup(&bus, &bluez).await;
        let (export, _requests) = publish(&control, epoch(1)).await.unwrap();
        let (socket, _) = acquire(
            &bluez,
            &name,
            Characteristic::AuthExchange,
            acquire_options(23),
        )
        .await
        .unwrap();
        let sending = export.notify(
            epoch(1),
            peer(&bluez),
            Characteristic::AuthExchange,
            output(Characteristic::AuthExchange, 1),
        );
        if drop_export {
            drop(export);
        } else {
            export.retire_local();
        }
        assert_eq!(bounded(sending).await, Err(Error::StaleContext));
        no_packet(&socket);
    }
}

#[tokio::test]
async fn peer_reconnect_or_same_owner_gap_cannot_redirect_the_old_native_fd() {
    for owner_gap in [false, true] {
        let bus = Bus::new().await;
        let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
        let (control, name) = setup(&bus, &bluez).await;
        let (export, _requests) = publish(&control, epoch(1)).await.unwrap();
        let (socket, _) = acquire(
            &bluez,
            &name,
            Characteristic::AuthExchange,
            acquire_options(23),
        )
        .await
        .unwrap();
        if owner_gap {
            assert!(bluez.release_name("org.bluez").await.unwrap());
            bluez.request_name("org.bluez").await.unwrap();
        } else {
            crate::bluez_peer::tests::reconnect_same_path(&bluez).await;
        }
        assert_eq!(
            bounded(export.notify(
                epoch(1),
                peer(&bluez),
                Characteristic::AuthExchange,
                output(Characteristic::AuthExchange, 1)
            ))
            .await,
            Err(Error::StaleContext)
        );
        no_packet(&socket);
    }
}

#[tokio::test]
async fn retirement_during_real_acquire_proof_never_returns_a_live_fd() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, _requests) = publish(&control, epoch(1)).await.unwrap();
    crate::bluez_peer::tests::hold_snapshot(&bluez, true).await;
    let (sender, target) = (bluez.clone(), name.clone());
    let acquiring = tokio::spawn(async move {
        acquire(
            &sender,
            &target,
            Characteristic::AuthExchange,
            acquire_options(23),
        )
        .await
    });
    crate::bluez_peer::tests::wait_held_snapshot(&bluez).await;
    export.retire_local();
    crate::bluez_peer::tests::hold_snapshot(&bluez, false).await;
    assert!(bounded(acquiring).await.unwrap().is_err());
}

#[tokio::test]
async fn bounded_kernel_backpressure_expires_without_replaying_any_packet() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, _requests) = publish(&control, epoch(1)).await.unwrap();
    let (socket, _) = acquire(
        &bluez,
        &name,
        Characteristic::AuthExchange,
        acquire_options(23),
    )
    .await
    .unwrap();
    let mut stopped = None;
    for id in 1..=64 {
        let result = tokio::time::timeout(
            Duration::from_secs(7),
            export.notify(
                epoch(1),
                peer(&bluez),
                Characteristic::AuthExchange,
                output(Characteristic::AuthExchange, id),
            ),
        )
        .await
        .unwrap();
        if result.is_err() {
            assert_eq!(result, Err(Error::Deadline));
            stopped = Some(id);
            break;
        }
    }
    let stopped = stopped.expect("small fixed kernel buffer must apply backpressure");
    assert!(stopped > 1);
    assert_eq!(
        bounded(export.notify(
            epoch(1),
            peer(&bluez),
            Characteristic::AuthExchange,
            output(Characteristic::AuthExchange, 65)
        ))
        .await,
        Err(Error::Unavailable)
    );
    let mut previous = 0;
    while let Ok(bytes) = receive(&socket) {
        if bytes.is_empty() {
            break;
        }
        assert!(
            bytes[4] > previous && bytes[4] <= stopped,
            "no replay or invented output"
        );
        previous = bytes[4];
    }
}

#[tokio::test]
async fn expired_postsend_proof_can_mean_packet_was_submitted_and_must_not_retry() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, _requests) = publish(&control, epoch(1)).await.unwrap();
    let (socket, _) = acquire(
        &bluez,
        &name,
        Characteristic::AuthExchange,
        acquire_options(23),
    )
    .await
    .unwrap();
    let sending = export.notify(
        epoch(1),
        peer(&bluez),
        Characteristic::AuthExchange,
        output(Characteristic::AuthExchange, 1),
    );
    let observe = async {
        let bytes = bounded(async {
            loop {
                match receive(&socket) {
                    Ok(bytes) => break bytes,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        tokio::task::yield_now().await
                    }
                    Err(error) => panic!("receiver: {error}"),
                }
            }
        })
        .await;
        crate::bluez_peer::tests::hold_snapshot(&bluez, true).await;
        bytes
    };
    let (result, bytes) = tokio::time::timeout(Duration::from_secs(7), async {
        tokio::join!(sending, observe)
    })
    .await
    .unwrap();
    crate::bluez_peer::tests::hold_snapshot(&bluez, false).await;
    assert_eq!(bytes, output(Characteristic::AuthExchange, 1).as_slice());
    assert_eq!(result, Err(Error::Deadline));
    assert!(bounded(export.notify(
        epoch(1),
        peer(&bluez),
        Characteristic::AuthExchange,
        output(Characteristic::AuthExchange, 2)
    ))
    .await
    .is_err());
    no_packet(&socket);
}
