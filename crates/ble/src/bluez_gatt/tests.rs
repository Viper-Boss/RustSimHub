// SPDX-License-Identifier: AGPL-3.0-only
#[cfg(feature = "bluez-notify-fd")]
mod native_fd;
mod notification_routing;
use super::*;
use crate::bluez_control::{
    tests::{Bus, Calls},
    Adapter,
};
use zbus::{zvariant::Value, Proxy};

const DEVICE: &str = "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF";
fn epoch(n: u64) -> Epoch {
    Epoch::new([9; 16], n).unwrap()
}
fn frame() -> Vec<u8> {
    vec![b'R', b'B', 1, 1, 1, 0, 0, 0, 1, 0, 3, 0, b'a', b'b', b'c']
}
fn options(write: bool) -> HashMap<String, OwnedValue> {
    let mut out = HashMap::from([("device".into(), object_value(DEVICE))]);
    if write {
        out.insert("type".into(), Value::from("request").try_into().unwrap());
    }
    out
}
fn object_value(path: &str) -> OwnedValue {
    Value::from(zbus::zvariant::ObjectPath::try_from(path).unwrap())
        .try_into()
        .unwrap()
}
async fn bounded<T>(f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(3), f)
        .await
        .expect("private GATT deadline")
}
async fn setup(bus: &Bus, bluez: &Connection) -> (BluezControl, String) {
    crate::bluez_peer::tests::install_connected_device(bluez).await;
    let connection = bus.connect().await;
    let name = connection.unique_name().unwrap().to_string();
    let control = bounded(BluezControl::bind(
        connection,
        bluez.unique_name().unwrap().as_str(),
        Adapter::Hci0,
    ))
    .await
    .unwrap();
    (control, name)
}
async fn write(
    sender: &Connection,
    name: &str,
    ch: Characteristic,
    bytes: Vec<u8>,
    options: HashMap<String, OwnedValue>,
) -> zbus::Result<()> {
    Proxy::new(sender, name, ch.path(), "org.bluez.GattCharacteristic1")
        .await?
        .call("WriteValue", &(bytes, options))
        .await
}
async fn read(
    sender: &Connection,
    name: &str,
    ch: Characteristic,
    options: HashMap<String, OwnedValue>,
) -> zbus::Result<Vec<u8>> {
    Proxy::new(sender, name, ch.path(), "org.bluez.GattCharacteristic1")
        .await?
        .call("ReadValue", &(options,))
        .await
}
fn writing(sender: &Connection, name: &str) -> tokio::task::JoinHandle<zbus::Result<()>> {
    let (sender, name) = (sender.clone(), name.to_owned());
    tokio::spawn(async move {
        write(
            &sender,
            &name,
            Characteristic::AuthExchange,
            frame(),
            options(true),
        )
        .await
    })
}

#[tokio::test]
async fn actual_object_manager_contains_only_the_fixed_service_and_four_characteristics() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, _requests) = bounded(publish(&control, epoch(1))).await.unwrap();
    let proxy = Proxy::new(
        &bluez,
        name.as_str(),
        manifest::ROOT,
        "org.freedesktop.DBus.ObjectManager",
    )
    .await
    .unwrap();
    let objects: HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>> =
        bounded(proxy.call("GetManagedObjects", &())).await.unwrap();
    assert_eq!(objects.len(), 5);
    let service = &objects[&OwnedObjectPath::try_from(manifest::SERVICE_PATH).unwrap()]
        ["org.bluez.GattService1"];
    assert_eq!(
        <&str>::try_from(&service["UUID"]).unwrap(),
        manifest::SERVICE_UUID
    );
    assert!(bool::try_from(&service["Primary"]).unwrap());
    for kind in Characteristic::ALL {
        let props = &objects[&OwnedObjectPath::try_from(kind.path()).unwrap()]
            ["org.bluez.GattCharacteristic1"];
        assert_eq!(<&str>::try_from(&props["UUID"]).unwrap(), kind.uuid());
        assert_eq!(
            OwnedObjectPath::try_from(props["Service"].try_clone().unwrap())
                .unwrap()
                .as_str(),
            manifest::SERVICE_PATH
        );
        assert_eq!(
            Vec::<String>::try_from(props["Flags"].try_clone().unwrap()).unwrap(),
            kind.flags()
        );
        assert!(
            !props.contains_key("Value"),
            "no unauthenticated ciphertext property cache"
        );
    }
}

#[tokio::test]
async fn device_info_supports_public_long_read_offsets_without_host_or_command_success() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, mut requests) = publish(&control, epoch(1)).await.unwrap();
    assert_eq!(
        bounded(read(
            &bluez,
            &name,
            Characteristic::DeviceInfo,
            options(false)
        ))
        .await
        .unwrap(),
        manifest::DEVICE_INFO
    );
    let mut opts = options(false);
    opts.insert("offset".into(), OwnedValue::from(20u16));
    assert_eq!(
        bounded(read(&bluez, &name, Characteristic::DeviceInfo, opts))
            .await
            .unwrap(),
        manifest::DEVICE_INFO[20..]
    );
    let mut opts = options(false);
    opts.insert("offset".into(), OwnedValue::from(u16::MAX));
    assert!(
        bounded(read(&bluez, &name, Characteristic::DeviceInfo, opts))
            .await
            .is_err()
    );
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn real_write_captures_header_owner_device_epoch_and_opaque_frame_before_host_ack() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, mut requests) = publish(&control, epoch(4)).await.unwrap();
    let call = writing(&bluez, &name);
    let request = bounded(requests.recv()).await.unwrap();
    assert!(!call.is_finished());
    assert_eq!(request.epoch(), epoch(4));
    assert_eq!(
        request.peer().owner(),
        bluez.unique_name().unwrap().as_str()
    );
    assert_eq!(request.peer().device_path(), DEVICE);
    assert_eq!(request.characteristic(), Characteristic::AuthExchange);
    match request.operation() {
        CallbackOperation::Write {
            frame: bytes,
            budget,
        } => {
            assert_eq!(bytes.as_slice(), frame());
            assert_eq!(*budget, Budget::INITIAL);
        }
        _ => panic!("wrong callback operation"),
    }
    request.check_current().await.unwrap();
    request.finish(Ok(CallbackReply::Accepted));
    bounded(call).await.unwrap().unwrap();
}

#[tokio::test]
async fn result_read_requires_host_value_and_checks_trusted_reply_size() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, mut requests) = publish(&control, epoch(1)).await.unwrap();
    for size in [15, 21] {
        let (sender, target) = (bluez.clone(), name.clone());
        let call = tokio::spawn(async move {
            read(&sender, &target, Characteristic::Result, options(false)).await
        });
        let request = bounded(requests.recv()).await.unwrap();
        assert!(matches!(
            request.operation(),
            CallbackOperation::ReadResult { .. }
        ));
        request.finish(Ok(CallbackReply::Value(
            OpaqueBytes::new(vec![7; size]).unwrap(),
        )));
        let result = bounded(call).await.unwrap();
        if size == 15 {
            assert_eq!(result.unwrap(), vec![7; 15]);
        } else {
            assert!(result.is_err());
        }
    }
}

#[tokio::test]
async fn foreign_actual_sender_is_denied_even_with_exact_valid_options() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let stranger = bus.connect().await;
    let (_export, mut requests) = publish(&control, epoch(1)).await.unwrap();
    assert!(bounded(write(
        &stranger,
        &name,
        Characteristic::AuthExchange,
        frame(),
        options(true)
    ))
    .await
    .is_err());
    assert!(bounded(read(
        &stranger,
        &name,
        Characteristic::DeviceInfo,
        options(false)
    ))
    .await
    .is_err());
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn malformed_options_offsets_modes_devices_and_frames_never_enter_host_queue() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, mut requests) = publish(&control, epoch(1)).await.unwrap();
    let mut bad = Vec::new();
    for key in ["device", "type"] {
        let mut opts = options(true);
        opts.remove(key);
        bad.push(opts);
    }
    for (key, value) in [
        ("offset", OwnedValue::from(1u16)),
        ("offset", OwnedValue::from(0u32)),
        ("prepare-authorize", OwnedValue::from(true)),
        ("unknown", OwnedValue::from(false)),
        ("mtu", OwnedValue::from(22u16)),
        ("type", Value::from("command").try_into().unwrap()),
        ("link", Value::from("BR/EDR").try_into().unwrap()),
        (
            "device",
            object_value("/org/bluez/hci1/dev_AA_BB_CC_DD_EE_FF"),
        ),
    ] {
        let mut opts = options(true);
        opts.insert(key.into(), value);
        bad.push(opts);
    }
    for opts in bad {
        assert!(bounded(write(
            &bluez,
            &name,
            Characteristic::AuthExchange,
            frame(),
            opts
        ))
        .await
        .is_err());
    }
    for bytes in [vec![], vec![0; 12], vec![0; 21], vec![0; 245]] {
        assert!(bounded(write(
            &bluez,
            &name,
            Characteristic::AuthExchange,
            bytes,
            options(true)
        ))
        .await
        .is_err());
    }
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn unsupported_characteristic_methods_and_notifications_never_fake_success() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, mut requests) = publish(&control, epoch(1)).await.unwrap();
    for kind in [Characteristic::DeviceInfo, Characteristic::Result] {
        assert!(bounded(write(&bluez, &name, kind, frame(), options(true)))
            .await
            .is_err());
    }
    for kind in [Characteristic::AuthExchange, Characteristic::Command] {
        assert!(bounded(read(&bluez, &name, kind, options(false)))
            .await
            .is_err());
    }
    for kind in Characteristic::ALL {
        let proxy = Proxy::new(
            &bluez,
            name.as_str(),
            kind.path(),
            "org.bluez.GattCharacteristic1",
        )
        .await
        .unwrap();
        for method in ["StartNotify", "StopNotify"] {
            assert!(bounded(proxy.call::<_, _, ()>(method, &())).await.is_err());
        }
    }
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn absent_or_abandoned_host_has_no_write_success_fallback() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, mut requests) = publish(&control, epoch(1)).await.unwrap();
    let call = writing(&bluez, &name);
    drop(bounded(requests.recv()).await.unwrap());
    assert!(bounded(call).await.unwrap().is_err());
    drop(requests);
    assert!(bounded(writing(&bluez, &name)).await.unwrap().is_err());
}

#[tokio::test]
async fn one_inflight_request_blocks_other_peer_until_real_host_completion() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, mut requests) = publish(&control, epoch(1)).await.unwrap();
    let call = writing(&bluez, &name);
    let request = bounded(requests.recv()).await.unwrap();
    let mut opts = options(true);
    opts.insert(
        "device".into(),
        object_value("/org/bluez/hci0/dev_11_22_33_44_55_66"),
    );
    assert!(
        bounded(write(&bluez, &name, Characteristic::Command, frame(), opts))
            .await
            .is_err()
    );
    assert!(requests.try_recv().is_err());
    request.finish(Ok(CallbackReply::Accepted));
    bounded(call).await.unwrap().unwrap();
    let call = writing(&bluez, &name);
    bounded(requests.recv())
        .await
        .unwrap()
        .finish(Ok(CallbackReply::Accepted));
    bounded(call).await.unwrap().unwrap();
}

#[tokio::test]
async fn real_five_second_timeout_retains_busy_until_late_host_work_finishes() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, mut requests) = publish(&control, epoch(1)).await.unwrap();
    let call = writing(&bluez, &name);
    let request = bounded(requests.recv()).await.unwrap();
    assert!(tokio::time::timeout(Duration::from_secs(7), call)
        .await
        .unwrap()
        .unwrap()
        .is_err());
    assert!(bounded(writing(&bluez, &name)).await.unwrap().is_err());
    assert!(requests.try_recv().is_err());
    assert_eq!(request.check_current().await, Err(Error::StaleContext));
    request.finish(Ok(CallbackReply::Accepted));
    let call = writing(&bluez, &name);
    bounded(requests.recv())
        .await
        .unwrap()
        .finish(Ok(CallbackReply::Accepted));
    bounded(call).await.unwrap().unwrap();
}

#[tokio::test]
async fn retirement_fences_pending_reply_and_cannot_rebind_fixed_path() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, mut requests) = publish(&control, epoch(1)).await.unwrap();
    let call = writing(&bluez, &name);
    let request = bounded(requests.recv()).await.unwrap();
    export.retire_local();
    assert_eq!(request.check_current().await, Err(Error::StaleContext));
    request.finish(Ok(CallbackReply::Accepted));
    assert!(bounded(call).await.unwrap().is_err());
    assert!(matches!(
        publish(&control, epoch(2)).await,
        Err(Error::Busy)
    ));
    assert!(bounded(read(
        &bluez,
        &name,
        Characteristic::DeviceInfo,
        options(false)
    ))
    .await
    .is_err());
}

#[tokio::test]
async fn same_owner_gap_fences_queued_request_and_late_host_reply() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, mut requests) = publish(&control, epoch(1)).await.unwrap();
    let call = writing(&bluez, &name);
    let request = bounded(requests.recv()).await.unwrap();
    assert!(bluez.release_name("org.bluez").await.unwrap());
    bluez.request_name("org.bluez").await.unwrap();
    assert_eq!(request.check_current().await, Err(Error::StaleContext));
    request.finish(Ok(CallbackReply::Accepted));
    assert!(bounded(call).await.unwrap().is_err());
    assert!(bounded(writing(&bluez, &name)).await.unwrap().is_err());
}

#[tokio::test]
async fn wrong_host_reply_class_cannot_ack_write_or_result_read() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, mut requests) = publish(&control, epoch(1)).await.unwrap();
    let call = writing(&bluez, &name);
    bounded(requests.recv())
        .await
        .unwrap()
        .finish(Ok(CallbackReply::Value(OpaqueBytes::new(vec![1]).unwrap())));
    assert!(bounded(call).await.unwrap().is_err());
    let (sender, target) = (bluez.clone(), name.clone());
    let call = tokio::spawn(async move {
        read(&sender, &target, Characteristic::Result, options(false)).await
    });
    bounded(requests.recv())
        .await
        .unwrap()
        .finish(Ok(CallbackReply::Accepted));
    assert!(bounded(call).await.unwrap().is_err());
}

#[tokio::test]
async fn dropping_control_or_export_revokes_real_callbacks() {
    for drop_control in [false, true] {
        let bus = Bus::new().await;
        let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
        let (control, name) = setup(&bus, &bluez).await;
        let (export, mut requests) = publish(&control, epoch(1)).await.unwrap();
        if drop_control {
            drop(control);
        } else {
            drop(export);
        }
        assert!(bounded(writing(&bluez, &name)).await.unwrap().is_err());
        assert!(requests.try_recv().is_err());
    }
}

#[tokio::test]
async fn trusted_bluez_mtu_is_forwarded_as_budget_without_changing_opaque_frame() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, mut requests) = publish(&control, epoch(1)).await.unwrap();
    let (sender, target) = (bluez.clone(), name.clone());
    let call = tokio::spawn(async move {
        let mut opts = options(true);
        opts.insert("mtu".into(), OwnedValue::from(64u16));
        opts.insert("offset".into(), OwnedValue::from(0u16));
        opts.insert("prepare-authorize".into(), OwnedValue::from(false));
        opts.insert("link".into(), Value::from("LE").try_into().unwrap());
        write(&sender, &target, Characteristic::Command, vec![9; 31], opts).await
    });
    let request = bounded(requests.recv()).await.unwrap();
    assert_eq!(request.characteristic(), Characteristic::Command);
    match request.operation() {
        CallbackOperation::Write { frame, budget } => {
            assert_eq!(frame.as_slice(), vec![9; 31]);
            assert_eq!(budget.attribute_bytes(), 61);
        }
        _ => panic!("wrong operation"),
    }
    request.finish(Ok(CallbackReply::Accepted));
    bounded(call).await.unwrap().unwrap();
}

#[tokio::test]
async fn extra_method_body_fields_are_denied_before_enqueue() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, mut requests) = publish(&control, epoch(1)).await.unwrap();
    let proxy = Proxy::new(
        &bluez,
        name.as_str(),
        Characteristic::AuthExchange.path(),
        "org.bluez.GattCharacteristic1",
    )
    .await
    .unwrap();
    assert!(
        bounded(proxy.call::<_, _, ()>("WriteValue", &(frame(), options(true), true)))
            .await
            .is_err()
    );
    let proxy = Proxy::new(
        &bluez,
        name.as_str(),
        Characteristic::Result.path(),
        "org.bluez.GattCharacteristic1",
    )
    .await
    .unwrap();
    assert!(
        bounded(proxy.call::<_, _, Vec<u8>>("ReadValue", &(options(false), true)))
            .await
            .is_err()
    );
    let mut opts = options(false);
    opts.insert("offset".into(), OwnedValue::from(1u16));
    assert!(bounded(read(&bluez, &name, Characteristic::Result, opts))
        .await
        .is_err());
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn caller_cancellation_does_not_release_the_host_owned_work_slot() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, mut requests) = publish(&control, epoch(1)).await.unwrap();
    let call = writing(&bluez, &name);
    let request = bounded(requests.recv()).await.unwrap();
    call.abort();
    assert!(call.await.unwrap_err().is_cancelled());
    assert!(bounded(writing(&bluez, &name)).await.unwrap().is_err());
    assert!(requests.try_recv().is_err());
    request.finish(Ok(CallbackReply::Accepted));
    // The server still owns its response until it consumes the late completion.
    bounded(async {
        while _export.publication.slot.available_permits() != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await;
    let call = writing(&bluez, &name);
    bounded(requests.recv())
        .await
        .unwrap()
        .finish(Ok(CallbackReply::Accepted));
    bounded(call).await.unwrap().unwrap();
}

#[tokio::test]
async fn control_drop_during_host_work_fences_late_reply() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, mut requests) = publish(&control, epoch(1)).await.unwrap();
    let call = writing(&bluez, &name);
    let request = bounded(requests.recv()).await.unwrap();
    drop(control);
    assert_eq!(request.check_current().await, Err(Error::Unavailable));
    request.finish(Ok(CallbackReply::Accepted));
    assert!(bounded(call).await.unwrap().is_err());
}

#[tokio::test]
async fn same_connection_contains_fixed_ad_and_gatt_objects_and_registration_calls() {
    use crate::bluez_advertisement::AdvertisementExport;
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (mut control, name) = setup(&bus, &bluez).await;
    let (gatt, _requests) = publish(&control, epoch(1)).await.unwrap();
    let (ad, _release) = AdvertisementExport::publish(&control, epoch(1))
        .await
        .unwrap();
    let proxy = Proxy::new(
        &bluez,
        name.as_str(),
        manifest::ROOT,
        "org.freedesktop.DBus.ObjectManager",
    )
    .await
    .unwrap();
    let objects: HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>> =
        bounded(proxy.call("GetManagedObjects", &())).await.unwrap();
    assert_eq!(objects.len(), 6);
    assert!(
        objects[&OwnedObjectPath::try_from(manifest::ADVERTISEMENT_PATH).unwrap()]
            .contains_key("org.bluez.LEAdvertisement1")
    );
    bounded(control.register_application()).await.unwrap();
    bounded(control.register_advertisement()).await.unwrap();
    assert!(matches!(
        publish(&control, epoch(2)).await,
        Err(Error::Busy)
    ));
    bounded(control.unregister_advertisement()).await.unwrap();
    bounded(control.unregister_application()).await.unwrap();
    ad.retire_local();
    bounded(gatt.close_connection()).await.unwrap();
}

async fn publish(
    control: &BluezControl,
    epoch: Epoch,
) -> Result<(GattExport, mpsc::Receiver<CallbackRequest>), Error> {
    let lease = PeerLease::bind(control, epoch, DEVICE).await?;
    GattExport::publish_bound(control, lease).await
}

#[tokio::test]
async fn actual_device_gap_retires_queued_work_without_manual_export_retire() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, mut requests) = publish(&control, epoch(1)).await.unwrap();
    let call = writing(&bluez, &name);
    let request = bounded(requests.recv()).await.unwrap();
    crate::bluez_peer::tests::reconnect_same_path(&bluez).await;
    assert_eq!(
        bounded(export.check_peer_current()).await,
        Err(Error::StaleContext)
    );
    assert_eq!(
        bounded(request.check_current()).await,
        Err(Error::StaleContext)
    );
    request.finish(Ok(CallbackReply::Accepted));
    assert!(bounded(call).await.unwrap().is_err());
    assert!(bounded(writing(&bluez, &name)).await.unwrap().is_err());
}

#[tokio::test]
async fn disconnected_lease_is_rejected_before_object_export_and_fresh_epoch_can_publish() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let lease = PeerLease::bind(&control, epoch(1), DEVICE).await.unwrap();
    crate::bluez_peer::tests::reconnect_same_path(&bluez).await;
    assert!(matches!(
        GattExport::publish_bound(&control, lease).await,
        Err(Error::StaleContext)
    ));
    // Rejection happens before ObjectServer startup. Inspect object absence;
    // a remote call to a connection with no server has no immediate handler.
    assert!(control
        .connection()
        .object_server()
        .interface::<_, zbus::fdo::ObjectManager>(manifest::ROOT)
        .await
        .is_err());
    let (_export, _requests) = publish(&control, epoch(2)).await.unwrap();
    assert_eq!(
        bounded(read(
            &bluez,
            &name,
            Characteristic::DeviceInfo,
            options(false)
        ))
        .await
        .unwrap(),
        manifest::DEVICE_INFO
    );
}

#[tokio::test]
async fn lease_from_another_control_binding_cannot_be_relabelled_for_export() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, _name) = setup(&bus, &bluez).await;
    let other = BluezControl::bind(
        bus.connect().await,
        bluez.unique_name().unwrap().as_str(),
        Adapter::Hci0,
    )
    .await
    .unwrap();
    let lease = PeerLease::bind(&control, epoch(1), DEVICE).await.unwrap();
    assert!(matches!(
        GattExport::publish_bound(&other, lease).await,
        Err(Error::StaleContext)
    ));
    let (_export, _requests) = publish(&other, epoch(2)).await.unwrap();
}

#[tokio::test]
async fn public_device_info_still_requires_the_bound_actual_device_context() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (_export, mut requests) = publish(&control, epoch(1)).await.unwrap();
    let mut opts = options(false);
    opts.insert(
        "device".into(),
        object_value("/org/bluez/hci0/dev_11_22_33_44_55_66"),
    );
    assert!(
        bounded(read(&bluez, &name, Characteristic::DeviceInfo, opts))
            .await
            .is_err()
    );
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn metadata_can_register_before_connection_but_callbacks_wait_for_real_lease() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (mut control, name) = setup(&bus, &bluez).await;
    crate::bluez_peer::tests::set_connected(&bluez, false).await;
    let (export, mut requests) = GattExport::publish(&control, epoch(1)).await.unwrap();
    bounded(control.register_application()).await.unwrap();
    assert_eq!(export.check_peer_current().await, Err(Error::Unavailable));
    assert!(bounded(writing(&bluez, &name)).await.unwrap().is_err());
    assert!(requests.try_recv().is_err());
    crate::bluez_peer::tests::set_connected(&bluez, true).await;
    let lease = PeerLease::bind(&control, epoch(1), DEVICE).await.unwrap();
    bounded(export.bind_peer(lease)).await.unwrap();
    let call = writing(&bluez, &name);
    bounded(requests.recv())
        .await
        .unwrap()
        .finish(Ok(CallbackReply::Accepted));
    bounded(call).await.unwrap().unwrap();
}

#[tokio::test]
async fn peer_binding_is_once_only_and_checks_epoch_before_exposing_callbacks() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, mut requests) = GattExport::publish(&control, epoch(1)).await.unwrap();
    let wrong_epoch = PeerLease::bind(&control, epoch(2), DEVICE).await.unwrap();
    assert_eq!(
        export.bind_peer(wrong_epoch).await,
        Err(Error::StaleContext)
    );
    assert!(bounded(writing(&bluez, &name)).await.unwrap().is_err());
    export
        .bind_peer(PeerLease::bind(&control, epoch(1), DEVICE).await.unwrap())
        .await
        .unwrap();
    assert_eq!(
        export
            .bind_peer(PeerLease::bind(&control, epoch(1), DEVICE).await.unwrap())
            .await,
        Err(Error::Busy)
    );
    crate::bluez_peer::tests::reconnect_same_path(&bluez).await;
    assert_eq!(export.check_peer_current().await, Err(Error::StaleContext));
    assert_eq!(
        export
            .bind_peer(PeerLease::bind(&control, epoch(1), DEVICE).await.unwrap())
            .await,
        Err(Error::Busy)
    );
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn cancelled_binding_exposes_no_peer_and_can_retry_only_with_fresh_lease() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, mut requests) = GattExport::publish(&control, epoch(1)).await.unwrap();
    let lease = PeerLease::bind(&control, epoch(1), DEVICE).await.unwrap();
    let mut binding = Box::pin(export.bind_peer(lease));
    assert!(futures_lite::future::poll_once(&mut binding)
        .await
        .is_none());
    drop(binding);
    assert!(bounded(writing(&bluez, &name)).await.unwrap().is_err());
    assert!(requests.try_recv().is_err());
    export
        .bind_peer(PeerLease::bind(&control, epoch(1), DEVICE).await.unwrap())
        .await
        .unwrap();
    let call = writing(&bluez, &name);
    bounded(requests.recv())
        .await
        .unwrap()
        .finish(Ok(CallbackReply::Accepted));
    bounded(call).await.unwrap().unwrap();
}

#[tokio::test]
async fn concurrent_binding_commits_only_one_lease_without_revoking_the_winner() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, mut requests) = GattExport::publish(&control, epoch(1)).await.unwrap();
    let first = PeerLease::bind(&control, epoch(1), DEVICE).await.unwrap();
    let second = PeerLease::bind(&control, epoch(1), DEVICE).await.unwrap();
    let (a, b) =
        bounded(async { tokio::join!(export.bind_peer(first), export.bind_peer(second)) }).await;
    assert!(matches!(
        (a, b),
        (Ok(()), Err(Error::Busy)) | (Err(Error::Busy), Ok(()))
    ));
    let call = writing(&bluez, &name);
    bounded(requests.recv())
        .await
        .unwrap()
        .finish(Ok(CallbackReply::Accepted));
    bounded(call).await.unwrap().unwrap();
}

#[tokio::test]
async fn retirement_while_binding_is_pending_cannot_publish_a_live_peer() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, mut requests) = GattExport::publish(&control, epoch(1)).await.unwrap();
    let lease = PeerLease::bind(&control, epoch(1), DEVICE).await.unwrap();
    let mut binding = Box::pin(export.bind_peer(lease));
    assert!(futures_lite::future::poll_once(&mut binding)
        .await
        .is_none());
    export.retire_local();
    assert_eq!(bounded(binding).await, Err(Error::StaleContext));
    assert!(bounded(writing(&bluez, &name)).await.unwrap().is_err());
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn host_ack_before_deadline_does_not_allow_late_success_after_stalled_peer_recheck() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = setup(&bus, &bluez).await;
    let (export, mut requests) = publish(&control, epoch(1)).await.unwrap();
    let call = writing(&bluez, &name);
    let request = bounded(requests.recv()).await.unwrap();
    crate::bluez_peer::tests::hold_snapshot(&bluez, true).await;
    request.finish(Ok(CallbackReply::Accepted));
    let result = tokio::time::timeout(Duration::from_secs(7), call)
        .await
        .unwrap()
        .unwrap();
    assert!(result.is_err());
    crate::bluez_peer::tests::hold_snapshot(&bluez, false).await;
    assert_eq!(
        bounded(export.check_peer_current()).await,
        Err(Error::StaleContext)
    );
    assert!(bounded(writing(&bluez, &name)).await.unwrap().is_err());
    assert!(requests.try_recv().is_err());
}
