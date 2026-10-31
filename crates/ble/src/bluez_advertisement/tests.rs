// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::{
    bluez_control::{
        tests::{Bus, Calls},
        Adapter,
    },
    manifest,
};
use std::{collections::HashMap, future::Future, time::Duration};
use zbus::{
    zvariant::{OwnedValue, Value},
    Proxy,
};

async fn bounded<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(3), future)
        .await
        .expect("private advertisement deadline")
}
fn epoch(start: u64) -> Epoch {
    Epoch::new([7; 16], start).unwrap()
}
async fn control(bus: &Bus, bluez: &Connection) -> (BluezControl, String) {
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
async fn release(sender: &Connection, target: &str) -> zbus::Result<()> {
    let proxy = Proxy::new(
        sender,
        target,
        manifest::ADVERTISEMENT_PATH,
        "org.bluez.LEAdvertisement1",
    )
    .await?;
    proxy.call("Release", &()).await
}
async fn properties(sender: &Connection, target: &str) -> HashMap<String, OwnedValue> {
    let proxy = Proxy::new(
        sender,
        target,
        manifest::ADVERTISEMENT_PATH,
        "org.freedesktop.DBus.Properties",
    )
    .await
    .unwrap();
    bounded(proxy.call("GetAll", &("org.bluez.LEAdvertisement1",)))
        .await
        .unwrap()
}

#[tokio::test]
async fn fixed_properties_are_read_only_and_contain_no_optional_advertisement_data() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = control(&bus, &bluez).await;
    let (_export, _events) = bounded(AdvertisementExport::publish(&control, epoch(1)))
        .await
        .unwrap();
    let mut values = properties(&bluez, &name).await;
    assert_eq!(values.len(), 2);
    assert_eq!(
        String::try_from(values.remove("Type").unwrap()).unwrap(),
        "peripheral"
    );
    assert_eq!(
        Vec::<String>::try_from(values.remove("ServiceUUIDs").unwrap()).unwrap(),
        vec![manifest::SERVICE_UUID.to_owned()]
    );
    let proxy = Proxy::new(
        &bluez,
        name.as_str(),
        manifest::ADVERTISEMENT_PATH,
        "org.freedesktop.DBus.Properties",
    )
    .await
    .unwrap();
    assert!(bounded(proxy.call::<_, _, ()>(
        "Set",
        &(
            "org.bluez.LEAdvertisement1",
            "Type",
            Value::from("broadcast")
        )
    ))
    .await
    .is_err());
    assert_eq!(
        String::try_from(properties(&bluez, &name).await.remove("Type").unwrap()).unwrap(),
        "peripheral"
    );
}

#[tokio::test]
async fn real_bluez_sender_delivers_exact_epoch_once() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = control(&bus, &bluez).await;
    let (_export, mut events) = bounded(AdvertisementExport::publish(&control, epoch(9)))
        .await
        .unwrap();
    bounded(release(&bluez, &name)).await.unwrap();
    let event = events.try_recv().unwrap();
    assert_eq!(event.epoch(), epoch(9));
    assert_eq!(event.owner(), bluez.unique_name().unwrap().as_str());
    bounded(release(&bluez, &name)).await.unwrap();
    assert!(matches!(
        events.try_recv(),
        Err(oneshot::error::TryRecvError::Closed)
    ));
}

#[tokio::test]
async fn foreign_actual_sender_cannot_claim_a_bluez_release() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = control(&bus, &bluez).await;
    let stranger = bus.connect().await;
    let (_export, mut events) = bounded(AdvertisementExport::publish(&control, epoch(1)))
        .await
        .unwrap();
    assert!(bounded(release(&stranger, &name)).await.is_err());
    assert!(matches!(
        events.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
    bounded(release(&bluez, &name)).await.unwrap();
    assert_eq!(events.try_recv().unwrap().epoch(), epoch(1));
}

#[tokio::test]
async fn owner_replacement_rejects_both_retired_and_replacement_sender() {
    let bus = Bus::new().await;
    let old = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = control(&bus, &old).await;
    let (_export, mut events) = bounded(AdvertisementExport::publish(&control, epoch(1)))
        .await
        .unwrap();
    assert!(bounded(old.release_name("org.bluez")).await.unwrap());
    let new = bus.bluez(Arc::new(Calls::default()), true).await;
    assert!(bounded(release(&old, &name)).await.is_err());
    assert!(bounded(release(&new, &name)).await.is_err());
    assert!(matches!(
        events.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn same_unique_owner_reacquisition_does_not_restore_callback_context() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = control(&bus, &bluez).await;
    let (_export, mut events) = bounded(AdvertisementExport::publish(&control, epoch(1)))
        .await
        .unwrap();
    assert!(bounded(bluez.release_name("org.bluez")).await.unwrap());
    bounded(bluez.request_name("org.bluez")).await.unwrap();
    assert!(bounded(release(&bluez, &name)).await.is_err());
    assert!(matches!(
        events.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn retirement_fences_callbacks_closes_sink_and_keeps_a_connection_tombstone() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = control(&bus, &bluez).await;
    let (export, mut events) = bounded(AdvertisementExport::publish(&control, epoch(1)))
        .await
        .unwrap();
    export.retire_local();
    export.retire_local();
    assert!(bounded(release(&bluez, &name)).await.is_err());
    assert!(matches!(
        events.try_recv(),
        Err(oneshot::error::TryRecvError::Closed)
    ));
    assert!(matches!(
        bounded(AdvertisementExport::publish(&control, epoch(2))).await,
        Err(Error::Busy)
    ));
}

#[tokio::test]
async fn dropping_export_does_not_allow_its_callbacks_or_reuse_its_object() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = control(&bus, &bluez).await;
    let (export, mut events) = bounded(AdvertisementExport::publish(&control, epoch(1)))
        .await
        .unwrap();
    drop(export);
    assert!(bounded(release(&bluez, &name)).await.is_err());
    assert!(matches!(
        events.try_recv(),
        Err(oneshot::error::TryRecvError::Closed)
    ));
    assert!(matches!(
        bounded(AdvertisementExport::publish(&control, epoch(2))).await,
        Err(Error::Busy)
    ));
}

#[tokio::test]
async fn missing_sink_never_returns_a_successful_release_ack() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = control(&bus, &bluez).await;
    let (_export, events) = bounded(AdvertisementExport::publish(&control, epoch(1)))
        .await
        .unwrap();
    drop(events);
    assert!(bounded(release(&bluez, &name)).await.is_err());
    assert!(bounded(release(&bluez, &name)).await.is_err());
}

#[tokio::test]
async fn release_with_extra_payload_is_rejected_even_from_bluez() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = control(&bus, &bluez).await;
    let (_export, mut events) = bounded(AdvertisementExport::publish(&control, epoch(1)))
        .await
        .unwrap();
    let proxy = Proxy::new(
        &bluez,
        name.as_str(),
        manifest::ADVERTISEMENT_PATH,
        "org.bluez.LEAdvertisement1",
    )
    .await
    .unwrap();
    assert!(
        bounded(proxy.call::<_, _, ()>("Release", &("claimed-epoch",)))
            .await
            .is_err()
    );
    assert!(matches!(
        events.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn objects_do_not_keep_the_control_authority_alive_through_a_reference_cycle() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = control(&bus, &bluez).await;
    let (_export, mut events) = bounded(AdvertisementExport::publish(&control, epoch(1)))
        .await
        .unwrap();
    drop(control);
    assert!(bounded(release(&bluez, &name)).await.is_err());
    assert!(matches!(
        events.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn a_late_release_to_old_connection_cannot_reach_new_epoch() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (old_control, old_name) = control(&bus, &bluez).await;
    let (old_export, _old_events) = bounded(AdvertisementExport::publish(&old_control, epoch(1)))
        .await
        .unwrap();
    bounded(old_export.close_connection()).await.unwrap();
    let (new_control, new_name) = control(&bus, &bluez).await;
    assert_ne!(new_name, old_name);
    let (_new_export, mut events) = bounded(AdvertisementExport::publish(&new_control, epoch(2)))
        .await
        .unwrap();
    assert!(bounded(release(&bluez, &old_name)).await.is_err());
    assert!(matches!(
        events.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
    bounded(release(&bluez, &new_name)).await.unwrap();
    assert_eq!(events.try_recv().unwrap().epoch(), epoch(2));
}

#[tokio::test]
async fn failed_duplicate_export_cannot_replace_the_first_epoch() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = control(&bus, &bluez).await;
    let (_export, mut events) = bounded(AdvertisementExport::publish(&control, epoch(1)))
        .await
        .unwrap();
    assert!(matches!(
        bounded(AdvertisementExport::publish(&control, epoch(2))).await,
        Err(Error::Busy)
    ));
    bounded(release(&bluez, &name)).await.unwrap();
    assert_eq!(events.try_recv().unwrap().epoch(), epoch(1));
}

#[tokio::test]
async fn an_unpolled_borrowed_publish_has_no_export_side_effect() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (control, name) = control(&bus, &bluez).await;
    drop(AdvertisementExport::publish(&control, epoch(1)));
    let (_export, mut events) = bounded(AdvertisementExport::publish(&control, epoch(2)))
        .await
        .unwrap();
    bounded(release(&bluez, &name)).await.unwrap();
    assert_eq!(events.try_recv().unwrap().epoch(), epoch(2));
}

#[tokio::test]
async fn closing_connection_is_terminal_for_all_old_control_clones() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (mut control, name) = control(&bus, &bluez).await;
    let (export, _events) = bounded(AdvertisementExport::publish(&control, epoch(1)))
        .await
        .unwrap();
    bounded(export.close_connection()).await.unwrap();
    assert!(bounded(release(&bluez, &name)).await.is_err());
    assert!(bounded(control.register_advertisement()).await.is_err());
    assert!(bounded(AdvertisementExport::publish(&control, epoch(2)))
        .await
        .is_err());
}

#[tokio::test]
async fn fixed_control_and_local_object_share_the_actual_registration_connection() {
    let bus = Bus::new().await;
    let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
    let (mut control, name) = control(&bus, &bluez).await;
    let (export, mut events) = bounded(AdvertisementExport::publish(&control, epoch(3)))
        .await
        .unwrap();
    bounded(control.register_advertisement()).await.unwrap();
    assert_eq!(properties(&bluez, &name).await.len(), 2);
    bounded(release(&bluez, &name)).await.unwrap();
    assert_eq!(events.try_recv().unwrap().epoch(), epoch(3));
    bounded(control.unregister_advertisement()).await.unwrap();
    bounded(export.close_connection()).await.unwrap();
}
