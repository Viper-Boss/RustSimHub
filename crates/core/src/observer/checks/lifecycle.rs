// SPDX-License-Identifier: AGPL-3.0-only
//! Real reconciliation over a private peer socket; no system bus or modem.
use super::*;
use tokio::sync::Semaphore;

struct HeldSms {
    entered: Arc<Semaphore>,
    release: Arc<Semaphore>,
}
#[zbus::interface(name = "org.freedesktop.ModemManager1.Sms")]
impl HeldSms {
    #[zbus(property)]
    fn state(&self) -> u32 {
        3
    }
    #[zbus(property)]
    fn pdu_type(&self) -> u32 {
        1
    }
    #[zbus(property)]
    fn number(&self) -> String {
        "synthetic".into()
    }
    #[zbus(property)]
    fn timestamp(&self) -> String {
        "synthetic time".into()
    }
    #[zbus(property)]
    async fn text(&self) -> String {
        self.entered.add_permits(1);
        self.release.acquire().await.unwrap().forget();
        "synthetic held reply".into()
    }
    #[zbus(property)]
    fn data(&self) -> Vec<u8> {
        vec![]
    }
}

fn ready(key: &SimFingerprinter) -> Stable {
    Stable { candidate: Some((
        "synthetic-prefix:/org/freedesktop/ModemManager1/Modem/0:/org/freedesktop/ModemManager1/SIM/0".into(),
        key.fingerprint("8901234567890123456").unwrap(),
        Instant::now() - Duration::from_secs(3),
    )) }
}

#[tokio::test]
async fn retired_object_cannot_publish_a_late_reply_and_fresh_read_recovers() {
    let entered = Arc::new(Semaphore::new(0));
    let release = Arc::new(Semaphore::new(0));
    // No ObjectManager: its unsolicited InterfacesAdded property dump would
    // invoke the held getter as well, obscuring which read is under test.
    let (a, b) = tokio::net::UnixStream::pair().unwrap();
    let guid = zbus::Guid::generate();
    let builder = zbus::connection::Builder::unix_stream(a)
        .server(guid.clone())
        .unwrap()
        .p2p()
        .serve_at("/org/freedesktop/DBus", FakeBus(guid.to_string()))
        .unwrap()
        .serve_at("/org/freedesktop/ModemManager1/Modem/0", FakeModem)
        .unwrap()
        .serve_at(
            "/org/freedesktop/ModemManager1/SIM/0",
            FakeSim(Arc::new(Mutex::new("8901234567890123456".into()))),
        )
        .unwrap()
        .serve_at(
            path(1),
            HeldSms {
                entered: entered.clone(),
                release: release.clone(),
            },
        )
        .unwrap();
    let client_builder = zbus::connection::Builder::unix_stream(b)
        .p2p()
        .unique_name(":1.21")
        .unwrap()
        .method_timeout(Duration::from_secs(2));
    let (server, client) = tokio::join!(builder.build(), client_builder.build());
    let (_server, client) = (server.unwrap(), client.unwrap());
    let d = directory();
    let identity_dir = directory();
    let store = rustsimhub_storage::Store::open(d.path()).unwrap();
    let h = store.handle();
    let key = rustsimhub_auth::Identity::open(identity_dir.path())
        .unwrap()
        .sim_fingerprinter()
        .unwrap();
    let metrics = Arc::new(Mutex::new(Stats::default()));
    let context = Context {
        owner: ":1.20".into(),
        prefix: "synthetic-prefix".into(),
        fingerprinter: &key,
        store: &h,
        metrics: &metrics,
        bindings: None,
    };
    let mut stable = ready(&key);
    let mut sources = Sources::default();
    sources.refresh(vec![path(1)]).unwrap();
    let mut cached = Some("/org/freedesktop/ModemManager1/Modem/0".try_into().unwrap());
    let (tx, rx) = tokio::sync::watch::channel(watch::Notice::default());
    let read = reconcile(
        &client,
        &context,
        &mut stable,
        &mut sources,
        &mut cached,
        rx.clone(),
        (0, 0),
    );
    let replace = async {
        entered.acquire().await.unwrap().forget();
        tx.send_modify(|n| n.apply(watch::Change::Retire(path(1).to_string())));
        release.add_permits(1);
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(read, replace)
    })
    .await
    .unwrap();
    result.unwrap();
    assert_eq!(
        message_count(&h),
        0,
        "a reply retired during GetAll must not become a confirmed SMS"
    );
    assert_eq!(
        h.inbox_events(None, 32)
            .unwrap()
            .events
            .iter()
            .filter(|e| e.kind == "sms_received")
            .count(),
        0
    );
    // Apply the same retirement consumed by session(), then read the new incarnation.
    sources.retire(path(1).as_str());
    sources.refresh(vec![path(1)]).unwrap();
    release.add_permits(1);
    let revision = rx.borrow().revision;
    tokio::time::timeout(
        Duration::from_secs(5),
        reconcile(
            &client,
            &context,
            &mut stable,
            &mut sources,
            &mut cached,
            rx,
            (0, revision),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(message_count(&h), 1);
}

#[test]
fn object_guard_distinguishes_unrelated_notices_replacement_overflow_and_disconnect() {
    let (tx, rx) = tokio::sync::watch::channel(watch::Notice::default());
    assert!(watch::sms_valid(&rx, 0, 0, path(1).as_str()));
    tx.send_modify(|n| n.apply(watch::Change::Wake));
    tx.send_modify(|n| n.apply(watch::Change::Retire(path(2).to_string())));
    assert!(watch::sms_valid(&rx, 0, 0, path(1).as_str()));
    tx.send_modify(|n| n.apply(watch::Change::Retire(path(1).to_string())));
    assert!(!watch::sms_valid(&rx, 0, 0, path(1).as_str()));
    let consumed = rx.borrow().revision;
    assert!(watch::sms_valid(&rx, 0, consumed, path(1).as_str()));
    for n in 3..70 {
        tx.send_modify(|notice| notice.apply(watch::Change::Retire(path(n).to_string())));
    }
    assert!(!watch::sms_valid(&rx, 0, consumed, path(1).as_str()));
    let consumed = rx.borrow().revision;
    assert!(watch::sms_valid(&rx, 0, consumed, path(1).as_str()));
    tx.send_modify(|n| n.apply(watch::Change::Fence));
    assert!(!watch::sms_valid(&rx, 0, consumed, path(1).as_str()));
    tx.send_modify(|n| n.apply(watch::Change::Disconnected));
    assert!(!watch::sms_valid(
        &rx,
        1,
        rx.borrow().revision,
        path(1).as_str()
    ));
    drop(tx);
    assert!(!watch::sms_valid(&rx, 1, u64::MAX, path(1).as_str()));
}

struct FaultMessaging(Arc<std::sync::atomic::AtomicBool>);
#[zbus::interface(name = "org.freedesktop.ModemManager1.Modem.Messaging")]
impl FaultMessaging {
    fn list(&self) -> zbus::fdo::Result<Vec<OwnedObjectPath>> {
        if self.0.load(std::sync::atomic::Ordering::SeqCst) {
            Err(zbus::fdo::Error::Failed("synthetic list failure".into()))
        } else {
            Ok(vec![path(1), path(2), path(999)])
        }
    }
}

#[tokio::test]
async fn list_failure_and_missing_object_preserve_confirmed_rows_and_recover_source_ids() {
    let (server, client, _sim) = peers().await;
    let failure = Arc::new(std::sync::atomic::AtomicBool::new(true));
    server
        .object_server()
        .remove::<FakeMessaging, _>("/org/freedesktop/ModemManager1/Modem/0")
        .await
        .unwrap();
    server
        .object_server()
        .at(
            "/org/freedesktop/ModemManager1/Modem/0",
            FaultMessaging(failure.clone()),
        )
        .await
        .unwrap();
    let d = directory();
    let identity_dir = directory();
    let store = rustsimhub_storage::Store::open(d.path()).unwrap();
    let h = store.handle();
    let key = rustsimhub_auth::Identity::open(identity_dir.path())
        .unwrap()
        .sim_fingerprinter()
        .unwrap();
    let metrics = Arc::new(Mutex::new(Stats::default()));
    let context = Context {
        owner: ":1.20".into(),
        prefix: "synthetic-prefix".into(),
        fingerprinter: &key,
        store: &h,
        metrics: &metrics,
        bindings: None,
    };
    let mut stable = ready(&key);
    let mut sources = Sources::default();
    sources.refresh(vec![path(1), path(2)]).unwrap();
    let mut cached = None;
    let (_tx, rx) = tokio::sync::watch::channel(watch::Notice::default());
    reconcile(
        &client,
        &context,
        &mut stable,
        &mut sources,
        &mut cached,
        rx.clone(),
        (0, 0),
    )
    .await
    .unwrap();
    let ids: Vec<_> = h
        .inbox(None, 32)
        .unwrap()
        .messages
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(ids.len(), 2);
    let head = h.inbox_event_head().unwrap();
    let mut reconnect = Sources::default();
    assert!(reconcile(
        &client,
        &context,
        &mut stable,
        &mut reconnect,
        &mut cached,
        rx.clone(),
        (0, 0)
    )
    .await
    .is_err());
    assert_eq!(h.inbox_event_head().unwrap().seq, head.seq);
    assert_eq!(h.inbox_event_head().unwrap().db_epoch, head.db_epoch);
    assert_eq!(message_count(&h), 2);
    failure.store(false, std::sync::atomic::Ordering::SeqCst);
    reconcile(
        &client,
        &context,
        &mut stable,
        &mut reconnect,
        &mut cached,
        rx,
        (0, 0),
    )
    .await
    .unwrap();
    assert_eq!(
        h.inbox(None, 32)
            .unwrap()
            .messages
            .into_iter()
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        ids
    );
    assert_eq!(h.inbox_event_head().unwrap().seq, head.seq);
    assert_eq!(h.inbox_event_head().unwrap().db_epoch, head.db_epoch);
    assert_eq!(metrics.lock().unwrap().object_read_errors, 1);
    assert_eq!(metrics.lock().unwrap().storage_errors, 0);
}
