// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use sources::{BATCH, MAX_OBJECTS};
fn path(n: usize) -> OwnedObjectPath {
    format!("/org/freedesktop/ModemManager1/SMS/{n:04}")
        .try_into()
        .unwrap()
}
fn sms(n: usize) -> IncomingSms {
    IncomingSms {
        line: rustsimhub_domain::LineEpoch(2),
        source: path(n).to_string(),
        sender: "synthetic".into(),
        timestamp: "synthetic time".into(),
        text: "汉字\\n🙂".into(),
        data: vec![],
    }
}
#[test]
fn batch_fairness_exceeds_old_limit_and_reconciles_deletions() {
    let mut s = Sources::default();
    let paths: Vec<_> = (0..513).map(path).collect();
    let mut seen = std::collections::HashSet::new();
    loop {
        let (batch, more) = s.batch(paths.clone()).unwrap();
        assert!(batch.len() <= BATCH);
        for p in batch {
            seen.insert(p);
        }
        if !more {
            break;
        }
    }
    assert_eq!(seen.len(), 513);
    assert!(s.batch((0..MAX_OBJECTS + 1).map(path).collect()).is_err());
    let mut a = sms(0);
    s.bind(&mut a).unwrap();
    let old = a.source;
    s.batch(vec![path(1)]).unwrap();
    let mut b = sms(0);
    s.bind(&mut b).unwrap();
    assert_ne!(b.source, old);
}
#[test]
fn reused_source_and_content_change_preserve_new_incarnations() {
    let mut s = Sources::default();
    let mut a = sms(0);
    s.bind(&mut a).unwrap();
    let mut repeated = sms(0);
    s.bind(&mut repeated).unwrap();
    assert_eq!(a.source, repeated.source);
    s.retire(path(0).as_str());
    let mut fresh = sms(0);
    s.bind(&mut fresh).unwrap();
    assert_ne!(a.source, fresh.source);
    let old = fresh.source;
    let mut changed = sms(0);
    changed.text = "new synthetic text".into();
    s.bind(&mut changed).unwrap();
    assert_ne!(old, changed.source);
    s.incomplete(path(0).as_str());
    let mut complete = sms(0);
    s.bind(&mut complete).unwrap();
    assert_ne!(complete.source, changed.source);
}
#[test]
fn signal_classification_ignores_signal_quality_but_fences_identity_and_removal() {
    let msg = |iface: &str, props: HashMap<String, OwnedValue>, invalid: Vec<String>| {
        zbus::Message::signal(
            "/org/freedesktop/ModemManager1/Modem/0",
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
        )
        .unwrap()
        .build(&(iface, props, invalid))
        .unwrap()
    };
    let quality = msg(
        MODEM,
        HashMap::from([("SignalQuality".into(), OwnedValue::from(50u32))]),
        vec![],
    );
    assert!(matches!(
        watch::classify(&quality).unwrap(),
        watch::Change::Ignore
    ));
    let invalid = msg(MODEM, HashMap::new(), vec!["Sim".into()]);
    assert!(matches!(
        watch::classify(&invalid).unwrap(),
        watch::Change::Fence
    ));
    let state = msg(
        MODEM,
        HashMap::from([("State".into(), OwnedValue::from(8i32))]),
        vec![],
    );
    assert!(matches!(
        watch::classify(&state).unwrap(),
        watch::Change::Wake
    ));
    let unknown = msg(
        MODEM,
        HashMap::from([("State".into(), OwnedValue::from(99i32))]),
        vec![],
    );
    assert!(matches!(
        watch::classify(&unknown).unwrap(),
        watch::Change::Fence
    ));
    let deleted = zbus::Message::signal(
        "/org/freedesktop/ModemManager1/Modem/0",
        "org.freedesktop.ModemManager1.Modem.Messaging",
        "Deleted",
    )
    .unwrap()
    .build(&(path(0),))
    .unwrap();
    assert!(matches!(
        watch::classify(&deleted).unwrap(),
        watch::Change::Retire(_)
    ));
    let owner = zbus::Message::signal(
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameOwnerChanged",
    )
    .unwrap()
    .build(&(SERVICE, ":1.20", ":1.21"))
    .unwrap();
    assert!(matches!(
        watch::classify(&owner).unwrap(),
        watch::Change::Disconnected
    ));
}
#[test]
fn event_bursts_remain_bounded_and_force_source_reconciliation() {
    let mut n = watch::Notice::default();
    for i in 0..1000 {
        n.apply(watch::Change::Retire(path(i).to_string()));
    }
    assert!(n.retired.len() <= 64);
    assert!(n.reset_sources > 0);
    assert!(!n.ended);
    n.apply(watch::Change::Fence);
    assert_eq!(n.fence, 1);
    n.revision = u64::MAX;
    n.apply(watch::Change::Wake);
    assert!(n.ended);
}
#[tokio::test]
async fn critical_signal_cancels_an_inflight_read_and_disconnection_cannot_publish_success() {
    let (tx, mut rx) = tokio::sync::watch::channel(watch::Notice::default());
    let read = watch::checked(std::future::pending::<Result<()>>(), &mut rx, 0);
    let signal = async {
        tokio::task::yield_now().await;
        tx.send_modify(|n| n.apply(watch::Change::Fence));
    };
    let (result, ()) = tokio::join!(read, signal);
    assert!(result.is_err());
    tx.send_modify(|n| n.apply(watch::Change::Disconnected));
    assert!(watch::checked(async { Ok(()) }, &mut rx, 1).await.is_err());
}
fn directory() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let d = tempfile::tempdir().unwrap();
    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    d
}
struct FakeBus;
#[zbus::interface(name = "org.freedesktop.DBus")]
impl FakeBus {
    fn get_name_owner(&self, _name: &str) -> String {
        ":1.20".into()
    }
}
struct FakeModem;
#[zbus::interface(name = "org.freedesktop.ModemManager1.Modem")]
impl FakeModem {
    #[zbus(property)]
    fn sim(&self) -> OwnedObjectPath {
        "/org/freedesktop/ModemManager1/SIM/0".try_into().unwrap()
    }
    #[zbus(property)]
    fn state(&self) -> i32 {
        8
    }
    #[zbus(property)]
    fn unlock_required(&self) -> u32 {
        3
    }
}
struct FakeSim(Arc<Mutex<String>>);
#[zbus::interface(name = "org.freedesktop.ModemManager1.Sim")]
impl FakeSim {
    #[zbus(property)]
    fn sim_identifier(&self) -> String {
        self.0.lock().unwrap().clone()
    }
}
struct FakeMessaging;
#[zbus::interface(name = "org.freedesktop.ModemManager1.Modem.Messaging")]
impl FakeMessaging {
    fn list(&self) -> Vec<OwnedObjectPath> {
        (0..40).map(path).collect()
    }
}
struct FakeSms(usize);
#[zbus::interface(name = "org.freedesktop.ModemManager1.Sms")]
impl FakeSms {
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
    fn text(&self) -> String {
        if self.0 == 0 {
            "x".repeat(4097)
        } else {
            "汉字\\n🙂".into()
        }
    }
    #[zbus(property)]
    fn data(&self) -> Vec<u8> {
        vec![]
    }
}
async fn peers() -> (zbus::Connection, zbus::Connection, Arc<Mutex<String>>) {
    let (a, b) = tokio::net::UnixStream::pair().unwrap();
    let guid = zbus::Guid::generate();
    let identity = Arc::new(Mutex::new("8901234567890123456".to_owned()));
    let mut server = zbus::connection::Builder::unix_stream(a)
        .server(guid)
        .unwrap()
        .p2p()
        .serve_at("/org/freedesktop/DBus", FakeBus)
        .unwrap()
        .serve_at("/org/freedesktop/ModemManager1", zbus::fdo::ObjectManager)
        .unwrap()
        .serve_at("/org/freedesktop/ModemManager1/Modem/0", FakeModem)
        .unwrap()
        .serve_at("/org/freedesktop/ModemManager1/Modem/0", FakeMessaging)
        .unwrap()
        .serve_at(
            "/org/freedesktop/ModemManager1/SIM/0",
            FakeSim(identity.clone()),
        )
        .unwrap();
    for i in 0..40 {
        server = server.serve_at(path(i), FakeSms(i)).unwrap();
    }
    let client = zbus::connection::Builder::unix_stream(b)
        .p2p()
        .unique_name(":1.21")
        .unwrap()
        .method_timeout(Duration::from_secs(2));
    let (server, client) = tokio::join!(server.build(), client.build());
    (server.unwrap(), client.unwrap(), identity)
}
fn message_count(h: &Handle) -> usize {
    let mut after = None;
    let mut n = 0;
    loop {
        let p = h.inbox(after, 32).unwrap();
        n += p.messages.len();
        if p.messages.len() < 32 {
            return n;
        }
        after = Some(p.cursor);
    }
}
async fn emit<T: serde::Serialize + zbus::zvariant::DynamicType>(
    c: &zbus::Connection,
    path: &str,
    interface: &str,
    member: &str,
    body: &T,
) {
    // No unique name on the peer ObjectServer: it must accept the real well-known D-Bus destination.
    // Synthetic signals supply the unique sender so production match rules remain unchanged.
    let msg = zbus::Message::signal(path, interface, member)
        .unwrap()
        .sender(":1.20")
        .unwrap()
        .build(body)
        .unwrap();
    c.send(&msg).await.unwrap();
}
#[tokio::test]
async fn private_peer_reconciles_batches_bad_objects_path_reuse_and_sim_signal() {
    let (server, client, sim) = peers().await;
    let d = directory();
    let i = directory();
    let store = rustsimhub_storage::Store::open(d.path()).unwrap();
    let h = store.handle();
    let key = rustsimhub_auth::Identity::open(i.path())
        .unwrap()
        .sim_fingerprinter()
        .unwrap();
    let metrics = Arc::new(Mutex::new(Stats::default()));
    let observe = session(client, &h, "synthetic-instance", 1, &key, &metrics);
    let exercise = async {
        tokio::time::timeout(Duration::from_secs(15), async {
            while message_count(&h) < 39 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(message_count(&h), 39);
        assert!(metrics.lock().unwrap().object_read_errors > 0);
        let old_line = h.line_epoch().unwrap();
        emit(
            &server,
            "/org/freedesktop/ModemManager1/Modem/0",
            "org.freedesktop.ModemManager1.Modem.Messaging",
            "Deleted",
            &(path(1),),
        )
        .await;
        emit(
            &server,
            "/org/freedesktop/ModemManager1/Modem/0",
            "org.freedesktop.ModemManager1.Modem.Messaging",
            "Added",
            &(path(1), true),
        )
        .await;
        tokio::time::timeout(Duration::from_secs(5), async {
            while message_count(&h) < 40 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(h.line_epoch().unwrap(), old_line);
        *sim.lock().unwrap() = "8901234567890123457".into();
        let props = HashMap::from([(
            "SimIdentifier".to_owned(),
            OwnedValue::from(zbus::zvariant::Str::from("8901234567890123457")),
        )]);
        emit(
            &server,
            "/org/freedesktop/ModemManager1/SIM/0",
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &(
                "org.freedesktop.ModemManager1.Sim",
                props,
                Vec::<String>::new(),
            ),
        )
        .await;
        tokio::time::timeout(Duration::from_secs(8), async {
            while h.line_epoch().unwrap() == old_line
                || h.line_status().unwrap().status != LineStatus::Ready
                || message_count(&h) < 79
            {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
        assert!(metrics.lock().unwrap().identity_invalidations > 0);
        let page = h.inbox(None, 1).unwrap();
        assert_eq!(
            h.sms_detail(page.messages[0].id.clone()).unwrap().text,
            "汉字\\n🙂"
        );
        server.close().await.unwrap();
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::join!(observe, exercise)
    })
    .await
    .unwrap();
    assert!(result.is_err());
}
