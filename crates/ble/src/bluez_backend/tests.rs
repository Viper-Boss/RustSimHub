// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::{
    bluez_control::{tests::Bus, Adapter},
    framing::InputKind,
    lifecycle::Lifecycle,
    ports::{SecurePortV1, SecureReply, State as LifecycleState},
};
use futures_lite::future::poll_fn;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    task::Poll,
    time::Duration,
};
use zbus::{
    zvariant::{OwnedObjectPath, OwnedValue},
    Connection, Proxy,
};
const DEVICE: &str = "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF";
fn epoch(n: u64) -> Epoch {
    Epoch::new([57; 16], n).unwrap()
}
#[derive(Default)]
struct Calls {
    rows: Mutex<Vec<&'static str>>,
    hold: AtomicBool,
    fail: AtomicBool,
    started: tokio::sync::Notify,
    release: tokio::sync::Notify,
}
impl Calls {
    async fn call(&self, action: &'static str) -> zbus::fdo::Result<()> {
        self.rows.lock().unwrap().push(action);
        self.started.notify_one();
        if self.hold.load(Ordering::Acquire) {
            self.release.notified().await;
        }
        if self.fail.load(Ordering::Acquire) {
            return Err(zbus::fdo::Error::Failed("synthetic failure".into()));
        }
        Ok(())
    }
    fn rows(&self) -> Vec<&'static str> {
        self.rows.lock().unwrap().clone()
    }
}
struct Gatt(Arc<Calls>);
#[zbus::interface(name = "org.bluez.GattManager1")]
impl Gatt {
    async fn register_application(
        &self,
        path: OwnedObjectPath,
        options: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<()> {
        assert_eq!(path.as_str(), crate::manifest::ROOT);
        assert!(options.is_empty());
        self.0.call("app+").await
    }
    async fn unregister_application(&self, path: OwnedObjectPath) -> zbus::fdo::Result<()> {
        assert_eq!(path.as_str(), crate::manifest::ROOT);
        self.0.call("app-").await
    }
}
struct Advertising(Arc<Calls>);
#[zbus::interface(name = "org.bluez.LEAdvertisingManager1")]
impl Advertising {
    async fn register_advertisement(
        &self,
        path: OwnedObjectPath,
        options: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<()> {
        assert_eq!(path.as_str(), crate::manifest::ADVERTISEMENT_PATH);
        assert!(options.is_empty());
        self.0.call("adv+").await
    }
    async fn unregister_advertisement(&self, path: OwnedObjectPath) -> zbus::fdo::Result<()> {
        assert_eq!(path.as_str(), crate::manifest::ADVERTISEMENT_PATH);
        self.0.call("adv-").await
    }
}
struct Device;
#[zbus::interface(name = "org.bluez.Device1")]
impl Device {
    #[zbus(property)]
    fn connected(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn adapter(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/org/bluez/hci0").unwrap()
    }
}
async fn bounded<T>(f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(3), f)
        .await
        .expect("actual private bus deadline")
}
async fn setup() -> (Bus, Connection, BluezBackend, BluezHost, Arc<Calls>) {
    let bus = Bus::new().await;
    let calls = Arc::new(Calls::default());
    let bluez = bus.connect().await;
    bluez.request_name("org.bluez").await.unwrap();
    bluez
        .object_server()
        .at("/org/bluez/hci0", Gatt(calls.clone()))
        .await
        .unwrap();
    bluez
        .object_server()
        .at("/org/bluez/hci0", Advertising(calls.clone()))
        .await
        .unwrap();
    bluez.object_server().at(DEVICE, Device).await.unwrap();
    let control = BluezControl::bind(
        bus.connect().await,
        bluez.unique_name().unwrap().as_str(),
        Adapter::Hci0,
    )
    .await
    .unwrap();
    let (backend, host) = BluezBackend::new(control);
    (bus, bluez, backend, host, calls)
}
#[cfg(feature = "product-host-candidate")]
#[tokio::test]
async fn product_driver_owns_real_registration_and_compensates_explicit_stop() {
    let (_bus, bluez, backend, host, calls) = setup().await;
    bluez
        .object_server()
        .at("/", zbus::fdo::ObjectManager)
        .await
        .unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let owner = bluez.unique_name().unwrap().to_string();
    let close_count = Arc::new(AtomicUsize::new(0));
    let driver = crate::product_host::run_epoch(
        backend,
        host,
        Security(close_count),
        &owner,
        [59; 16],
        async {
            let _ = stopped.await;
        },
    );
    let cancel = async {
        loop {
            if calls.rows().contains(&"adv+") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        stop.send(()).unwrap();
    };
    let (result, ()) = bounded(async { tokio::join!(driver, cancel) }).await;
    result.unwrap();
    assert_eq!(calls.rows(), ["app+", "adv+", "adv-", "app-"]);
}
#[cfg(feature = "product-host-candidate")]
#[tokio::test]
async fn product_driver_does_not_follow_replacement_bluez_owner() {
    let (bus, bluez, backend, host, calls) = setup().await;
    bluez
        .object_server()
        .at("/", zbus::fdo::ObjectManager)
        .await
        .unwrap();
    let owner = bluez.unique_name().unwrap().to_string();
    let replacement = bus.connect().await;
    let driver = crate::product_host::run_epoch(
        backend,
        host,
        Security(Arc::new(AtomicUsize::new(0))),
        &owner,
        [60; 16],
        std::future::pending(),
    );
    let replace = async {
        loop {
            if calls.rows().contains(&"adv+") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        bluez.release_name("org.bluez").await.unwrap();
        replacement.request_name("org.bluez").await.unwrap();
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(8), async {
        tokio::join!(driver, replace)
    })
    .await
    .unwrap();
    assert!(result.is_err());
    assert_eq!(calls.rows().iter().filter(|s| **s == "app+").count(), 1);
}
struct Security(Arc<AtomicUsize>);
impl SecurePortV1 for Security {
    fn receive(&mut self, _: Epoch, _: Peer, _: InputKind, _: OpaqueBytes) -> Pending<SecureReply> {
        Box::pin(async { Err(Error::Unavailable) })
    }
    fn disconnected(&mut self, _: Epoch, _: Peer) -> Pending<()> {
        let calls = self.0.clone();
        Box::pin(async move {
            calls.fetch_add(1, Ordering::AcqRel);
            Ok(())
        })
    }
}
async fn progress(lifecycle: &mut Lifecycle<BluezBackend, Security>, target: LifecycleState) {
    bounded(poll_fn(|cx| {
        let state = lifecycle.poll(cx, 0).unwrap();
        if state == target {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }))
    .await
}
use std::collections::HashMap;
#[tokio::test]
async fn actual_lifecycle_registers_four_fixed_rpcs_and_closes_dedicated_connection() {
    let (_bus, bluez, backend, host, calls) = setup().await;
    let security = Arc::new(AtomicUsize::new(0));
    let mut lifecycle = Lifecycle::new(
        backend,
        Some(Security(security.clone())),
        bluez.unique_name().unwrap().as_str(),
        [57; 16],
    )
    .unwrap();
    let e = lifecycle.start(0).unwrap();
    assert_eq!(e, epoch(1));
    progress(&mut lifecycle, LifecycleState::Advertising).await;
    let _callbacks = host.take_callbacks(e).unwrap();
    let _release = host.take_release(e).unwrap();
    let peer = host.bind_peer(e, DEVICE.into()).await.unwrap();
    lifecycle.connected(e, peer, 0).unwrap();
    lifecycle.stop(0).unwrap();
    progress(&mut lifecycle, LifecycleState::Disabled).await;
    assert!(lifecycle.is_quiescent());
    assert_eq!(security.load(Ordering::Acquire), 1);
    assert_eq!(calls.rows(), vec!["app+", "adv+", "adv-", "app-"]);
    assert_eq!(host.check_peer(e).await, Err(Error::StaleContext));
}
#[tokio::test]
async fn missing_security_rejects_before_any_export_or_manager_rpc() {
    let (_bus, bluez, backend, _host, calls) = setup().await;
    assert!(matches!(
        Lifecycle::<_, Security>::new(
            backend,
            None,
            bluez.unique_name().unwrap().as_str(),
            [57; 16]
        ),
        Err(Error::Unavailable)
    ));
    assert!(calls.rows().is_empty());
}
#[tokio::test]
async fn callback_and_release_sinks_are_once_only_and_wrong_epoch_cannot_consume() {
    let (_bus, _bluez, mut backend, host, _calls) = setup().await;
    backend
        .register_application(epoch(1), FixedManifest)
        .await
        .unwrap();
    assert!(matches!(
        host.take_callbacks(epoch(2)),
        Err(Error::StaleContext)
    ));
    let _callbacks = host.take_callbacks(epoch(1)).unwrap();
    assert!(matches!(host.take_callbacks(epoch(1)), Err(Error::Busy)));
    backend
        .register_advertisement(epoch(1), FixedAdvertisement)
        .await
        .unwrap();
    assert!(matches!(
        host.take_release(epoch(2)),
        Err(Error::StaleContext)
    ));
    let _release = host.take_release(epoch(1)).unwrap();
    assert!(matches!(host.take_release(epoch(1)), Err(Error::Busy)));
    backend.unregister_advertisement(epoch(1)).await.unwrap();
    backend.unregister_application(epoch(1)).await.unwrap();
}
#[tokio::test]
async fn one_epoch_cannot_republish_fixed_tombstones_after_clean_shutdown() {
    let (_bus, _bluez, mut backend, _host, calls) = setup().await;
    backend
        .register_application(epoch(1), FixedManifest)
        .await
        .unwrap();
    backend.unregister_application(epoch(1)).await.unwrap();
    assert_eq!(
        backend.register_application(epoch(2), FixedManifest).await,
        Err(Error::StaleContext)
    );
    assert_eq!(calls.rows(), vec!["app+", "app-"]);
}
#[tokio::test]
async fn wrong_epoch_cleanup_does_not_fence_the_live_application() {
    let (_bus, _bluez, mut backend, host, calls) = setup().await;
    backend
        .register_application(epoch(1), FixedManifest)
        .await
        .unwrap();
    assert_eq!(
        backend.unregister_application(epoch(2)).await,
        Err(Error::StaleContext)
    );
    let peer = host.bind_peer(epoch(1), DEVICE.into()).await.unwrap();
    assert_eq!(peer.device_path(), DEVICE);
    backend.unregister_application(epoch(1)).await.unwrap();
    assert_eq!(calls.rows(), vec!["app+", "app-"]);
}
#[tokio::test]
async fn concurrent_manager_attempt_is_busy_until_owned_late_future_finishes() {
    let (_bus, _bluez, mut backend, host, calls) = setup().await;
    calls.hold.store(true, Ordering::Release);
    let pending = tokio::spawn(backend.register_application(epoch(1), FixedManifest));
    bounded(calls.started.notified()).await;
    assert_eq!(
        backend.unregister_application(epoch(1)).await,
        Err(Error::Busy)
    );
    calls.hold.store(false, Ordering::Release);
    calls.release.notify_one();
    bounded(pending).await.unwrap().unwrap();
    let _callbacks = host.take_callbacks(epoch(1)).unwrap();
    backend.unregister_application(epoch(1)).await.unwrap();
}
#[tokio::test]
async fn cancelling_started_registration_latches_unknown_even_after_compensation() {
    let (_bus, _bluez, mut backend, host, calls) = setup().await;
    calls.hold.store(true, Ordering::Release);
    let pending = tokio::spawn(backend.register_application(epoch(1), FixedManifest));
    bounded(calls.started.notified()).await;
    pending.abort();
    assert!(pending.await.unwrap_err().is_cancelled());
    assert_eq!(host.check_peer(epoch(1)).await, Err(Error::StaleContext));
    calls.hold.store(false, Ordering::Release);
    calls.release.notify_one();
    assert_eq!(
        backend.unregister_application(epoch(1)).await,
        Err(Error::Unknown)
    );
}
#[tokio::test]
async fn failed_advertisement_cannot_be_reported_quiescent_after_failed_cleanup() {
    let (_bus, _bluez, mut backend, host, calls) = setup().await;
    backend
        .register_application(epoch(1), FixedManifest)
        .await
        .unwrap();
    calls.fail.store(true, Ordering::Release);
    assert_eq!(
        backend
            .register_advertisement(epoch(1), FixedAdvertisement)
            .await,
        Err(Error::Backend)
    );
    assert_eq!(host.check_peer(epoch(1)).await, Err(Error::StaleContext));
    assert_eq!(
        backend.unregister_advertisement(epoch(1)).await,
        Err(Error::Backend)
    );
    calls.fail.store(false, Ordering::Release);
    assert_eq!(
        backend.unregister_application(epoch(1)).await,
        Err(Error::Unknown)
    );
}
#[tokio::test]
async fn owner_loss_future_is_owned_once_and_ends_on_actual_broker_change() {
    let (_bus, bluez, _backend, mut host, _calls) = setup().await;
    let wait = host.take_owner_loss().unwrap();
    assert!(matches!(host.take_owner_loss(), Err(Error::Busy)));
    bluez.release_name("org.bluez").await.unwrap();
    assert_eq!(bounded(wait).await, Err(Error::StaleContext));
}
#[tokio::test]
async fn dropping_host_fences_bound_peer_before_backend_cleanup() {
    let (_bus, _bluez, mut backend, host, _calls) = setup().await;
    backend
        .register_application(epoch(1), FixedManifest)
        .await
        .unwrap();
    let peer = host.bind_peer(epoch(1), DEVICE.into()).await.unwrap();
    drop(host);
    assert_eq!(
        backend
            .notify(
                epoch(1),
                peer,
                Characteristic::Result,
                OpaqueBytes::new(vec![1]).unwrap()
            )
            .await,
        Err(Error::StaleContext)
    );
    backend.unregister_application(epoch(1)).await.unwrap();
}
#[tokio::test]
async fn unregister_application_without_advertisement_cleanup_is_unknown_not_success() {
    let (_bus, _bluez, mut backend, host, _calls) = setup().await;
    backend
        .register_application(epoch(1), FixedManifest)
        .await
        .unwrap();
    backend
        .register_advertisement(epoch(1), FixedAdvertisement)
        .await
        .unwrap();
    let _release = host.take_release(epoch(1)).unwrap();
    assert_eq!(
        backend.unregister_application(epoch(1)).await,
        Err(Error::Unknown)
    );
}
#[tokio::test]
async fn unpolled_registration_is_side_effect_free_and_does_not_consume_epoch() {
    let (_bus, _bluez, mut backend, _host, calls) = setup().await;
    drop(backend.register_application(epoch(2), FixedManifest));
    assert!(calls.rows().is_empty());
    backend
        .register_application(epoch(1), FixedManifest)
        .await
        .unwrap();
    backend.unregister_application(epoch(1)).await.unwrap();
}

#[tokio::test]
async fn actual_release_callback_reaches_once_owned_host_sink() {
    let (_bus, bluez, mut backend, host, _calls) = setup().await;
    backend
        .register_application(epoch(1), FixedManifest)
        .await
        .unwrap();
    backend
        .register_advertisement(epoch(1), FixedAdvertisement)
        .await
        .unwrap();
    let release = host.take_release(epoch(1)).unwrap();
    let target = backend.shared.connection.unique_name().unwrap().to_string();
    let _: () = Proxy::new(
        &bluez,
        target.as_str(),
        crate::manifest::ADVERTISEMENT_PATH,
        "org.bluez.LEAdvertisement1",
    )
    .await
    .unwrap()
    .call("Release", &())
    .await
    .unwrap();
    let event = bounded(release).await.unwrap();
    assert_eq!(event.epoch(), epoch(1));
    assert_eq!(event.owner(), bluez.unique_name().unwrap().as_str());
    backend.unregister_advertisement(epoch(1)).await.unwrap();
    backend.unregister_application(epoch(1)).await.unwrap();
}

#[tokio::test]
async fn lifecycle_deadline_retains_real_late_registration_until_compensated() {
    let (_bus, bluez, backend, _host, calls) = setup().await;
    calls.hold.store(true, Ordering::Release);
    let mut lifecycle = Lifecycle::new(
        backend,
        Some(Security(Arc::new(AtomicUsize::new(0)))),
        bluez.unique_name().unwrap().as_str(),
        [57; 16],
    )
    .unwrap();
    lifecycle.start(0).unwrap();
    let started = calls.started.notified();
    tokio::pin!(started);
    bounded(poll_fn(|cx| {
        lifecycle.poll(cx, 0).unwrap();
        std::future::Future::poll(started.as_mut(), cx)
    }))
    .await;
    assert_eq!(calls.rows(), vec!["app+"]);
    poll_fn(|cx| {
        assert_eq!(lifecycle.poll(cx, 5001).unwrap(), LifecycleState::Unknown);
        Poll::Ready(())
    })
    .await;
    assert!(lifecycle.has_pending_backend());
    assert!(!lifecycle.is_quiescent());
    calls.hold.store(false, Ordering::Release);
    calls.release.notify_one();
    bounded(poll_fn(|cx| {
        let state = lifecycle.poll(cx, 5001).unwrap();
        if state == LifecycleState::Disabled {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }))
    .await;
    assert!(lifecycle.is_quiescent());
    assert_eq!(calls.rows(), vec!["app+", "app-"]);
}
#[tokio::test]
async fn actual_owner_gap_during_register_never_unregisters_replacement_context() {
    let (_bus, bluez, mut backend, _host, calls) = setup().await;
    calls.hold.store(true, Ordering::Release);
    let pending = tokio::spawn(backend.register_application(epoch(1), FixedManifest));
    bounded(calls.started.notified()).await;
    bluez.release_name("org.bluez").await.unwrap();
    bluez.request_name("org.bluez").await.unwrap();
    calls.hold.store(false, Ordering::Release);
    calls.release.notify_one();
    assert_eq!(bounded(pending).await.unwrap(), Err(Error::StaleContext));
    assert_eq!(
        backend.unregister_application(epoch(1)).await,
        Err(Error::StaleContext)
    );
    assert_eq!(calls.rows(), vec!["app+"]);
}
#[tokio::test]
async fn host_drop_while_register_is_pending_cannot_reenable_application() {
    let (_bus, _bluez, mut backend, host, calls) = setup().await;
    calls.hold.store(true, Ordering::Release);
    let pending = tokio::spawn(backend.register_application(epoch(1), FixedManifest));
    bounded(calls.started.notified()).await;
    drop(host);
    calls.hold.store(false, Ordering::Release);
    calls.release.notify_one();
    assert_eq!(bounded(pending).await.unwrap(), Err(Error::StaleContext));
    assert_eq!(
        backend.unregister_application(epoch(1)).await,
        Err(Error::Unknown)
    );
}

#[tokio::test]
async fn stopping_fences_an_actual_owned_callback_before_host_can_ack() {
    let (_bus, bluez, mut backend, host, _calls) = setup().await;
    backend
        .register_application(epoch(1), FixedManifest)
        .await
        .unwrap();
    host.bind_peer(epoch(1), DEVICE.into()).await.unwrap();
    let mut callbacks = host.take_callbacks(epoch(1)).unwrap();
    let target = backend.shared.connection.unique_name().unwrap().to_string();
    let sender = bluez.clone();
    let writer = tokio::spawn(async move {
        let options = HashMap::<String, OwnedValue>::from([
            (
                "device".into(),
                zbus::zvariant::Value::from(OwnedObjectPath::try_from(DEVICE).unwrap())
                    .try_into()
                    .unwrap(),
            ),
            (
                "type".into(),
                zbus::zvariant::Value::from("request").try_into().unwrap(),
            ),
        ]);
        let bytes = vec![b'R', b'B', 1, 1, 1, 0, 0, 0, 1, 0, 3, 0, b'a', b'b', b'c'];
        Proxy::new(
            &sender,
            target.as_str(),
            Characteristic::AuthExchange.path(),
            "org.bluez.GattCharacteristic1",
        )
        .await
        .unwrap()
        .call::<_, _, ()>("WriteValue", &(bytes, options))
        .await
    });
    let request = bounded(callbacks.recv()).await.unwrap();
    request.check_current().await.unwrap();
    backend.unregister_advertisement(epoch(1)).await.unwrap();
    assert_eq!(request.check_current().await, Err(Error::StaleContext));
    request.finish(Ok(crate::bluez_gatt::CallbackReply::Accepted));
    assert!(bounded(writer).await.unwrap().is_err());
    backend.unregister_application(epoch(1)).await.unwrap();
}
