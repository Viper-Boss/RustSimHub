// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use futures_lite::future::poll_once;

#[tokio::test]
async fn idle_listener_observes_owner_loss_without_another_control_or_callback() {
    let bus = Bus::new().await;
    let server = bus.bluez(Arc::new(Calls::default()), true).await;
    let port = control(&bus, &server).await;
    let mut waiting = port.wait_owner_loss();
    assert!(poll_once(&mut waiting).await.is_none());
    assert!(bounded(server.release_name(BLUEZ)).await.unwrap());
    assert_eq!(bounded(waiting).await, Err(Error::StaleContext));
    assert!(!port.binding.watch.live.load(Ordering::Acquire));
}

#[tokio::test]
async fn listener_started_after_same_unique_owner_gap_cannot_miss_it() {
    let bus = Bus::new().await;
    let server = bus.bluez(Arc::new(Calls::default()), true).await;
    let mut port = control(&bus, &server).await;
    let waiting = port.wait_owner_loss();
    assert!(bounded(server.release_name(BLUEZ)).await.unwrap());
    bounded(server.request_name(BLUEZ)).await.unwrap();
    assert_eq!(bounded(waiting).await, Err(Error::StaleContext));
    assert_eq!(
        bounded(port.register_application()).await,
        Err(Error::StaleContext)
    );
}

#[tokio::test]
async fn idle_wait_does_not_hold_the_rpc_or_callback_checker_lock() {
    let bus = Bus::new().await;
    let calls = Arc::new(Calls::default());
    let server = bus.bluez(calls.clone(), true).await;
    let mut port = control(&bus, &server).await;
    let mut waiting = port.wait_owner_loss();
    assert!(poll_once(&mut waiting).await.is_none());
    bounded(port.register_application()).await.unwrap();
    assert!(poll_once(&mut waiting).await.is_none());
    bounded(port.unregister_application()).await.unwrap();
    assert_eq!(calls.rows.lock().unwrap().len(), 2);
    port.retire();
    assert_eq!(bounded(waiting).await, Err(Error::StaleContext));
}

#[tokio::test]
async fn only_one_waiter_is_polled_and_explicit_retire_wakes_it() {
    let bus = Bus::new().await;
    let server = bus.bluez(Arc::new(Calls::default()), true).await;
    let port = control(&bus, &server).await;
    let mut waiting = port.wait_owner_loss();
    assert!(poll_once(&mut waiting).await.is_none());
    assert_eq!(bounded(port.wait_owner_loss()).await, Err(Error::Busy));
    port.retire();
    port.retire();
    assert_eq!(bounded(waiting).await, Err(Error::StaleContext));
    assert_eq!(
        bounded(port.wait_owner_loss()).await,
        Err(Error::StaleContext)
    );
}

#[tokio::test]
async fn cancelled_wait_releases_its_slot_without_retiring_the_live_binding() {
    let bus = Bus::new().await;
    let server = bus.bluez(Arc::new(Calls::default()), true).await;
    let mut port = control(&bus, &server).await;
    let mut waiting = port.wait_owner_loss();
    assert!(poll_once(&mut waiting).await.is_none());
    drop(waiting);
    assert!(!port.binding.watch.waiting.load(Ordering::Acquire));
    bounded(port.register_application()).await.unwrap();
    let mut replacement = port.wait_owner_loss();
    assert!(poll_once(&mut replacement).await.is_none());
    assert!(bounded(server.release_name(BLUEZ)).await.unwrap());
    assert_eq!(bounded(replacement).await, Err(Error::StaleContext));
}

#[tokio::test]
async fn foreign_owner_signal_cannot_wake_a_listener_as_bluez_loss() {
    let bus = Bus::new().await;
    let server = bus.bluez(Arc::new(Calls::default()), true).await;
    let mut port = control(&bus, &server).await;
    let stranger = bus.connect().await;
    let mut waiting = port.wait_owner_loss();
    assert!(poll_once(&mut waiting).await.is_none());
    bounded(stranger.emit_signal(
        None::<&str>,
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameOwnerChanged",
        &(BLUEZ, server.unique_name().unwrap().as_str(), ""),
    ))
    .await
    .unwrap();
    bounded(port.register_application()).await.unwrap();
    assert!(poll_once(&mut waiting).await.is_none());
    assert!(bounded(server.release_name(BLUEZ)).await.unwrap());
    assert_eq!(bounded(waiting).await, Err(Error::StaleContext));
}

#[tokio::test]
async fn unpolled_listener_does_not_reserve_the_only_wait_slot() {
    let bus = Bus::new().await;
    let server = bus.bluez(Arc::new(Calls::default()), true).await;
    let port = control(&bus, &server).await;
    let unpolled = port.wait_owner_loss();
    let mut waiting = port.wait_owner_loss();
    assert!(poll_once(&mut waiting).await.is_none());
    drop(unpolled);
    assert_eq!(bounded(port.wait_owner_loss()).await, Err(Error::Busy));
    port.retire();
    assert_eq!(bounded(waiting).await, Err(Error::StaleContext));
}

#[tokio::test]
async fn real_bus_connection_close_ends_idle_wait_with_a_finite_failure() {
    let bus = Bus::new().await;
    let server = bus.bluez(Arc::new(Calls::default()), true).await;
    let port = control(&bus, &server).await;
    let mut waiting = port.wait_owner_loss();
    assert!(poll_once(&mut waiting).await.is_none());
    bounded(port.binding.connection.clone().close())
        .await
        .unwrap();
    assert!(matches!(
        bounded(waiting).await,
        Err(Error::Backend | Error::Unavailable | Error::StaleContext)
    ));
    assert!(!port.binding.watch.live.load(Ordering::Acquire));
}
