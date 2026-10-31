// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit container fixture. Actual Rust GATT, broker policy and kernel FDs;
//! only the BlueZ device/manager endpoint and opaque host completion are synthetic.
#![cfg(all(test, target_os = "linux"))]
use rustsimhub_ble::{
    bluez_control::{Adapter, BluezControl},
    bluez_gatt::{CallbackOperation, CallbackReply, GattExport},
    bluez_peer::PeerLease,
    framing::OpaqueBytes,
    manifest::{self, Characteristic},
    ports::Epoch,
    Error,
};
use socket2::Socket;
use std::{
    collections::HashMap,
    io::Read,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use zbus::{
    zvariant::{OwnedFd, OwnedObjectPath, OwnedValue, Value},
    Connection, Proxy,
};
const DEVICE: &str = "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF";
const IFACE: &str = "org.bluez.GattCharacteristic1";
type Objects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

fn epoch() -> Epoch {
    Epoch::new([43; 16], 1).unwrap()
}
fn frame(kind: Characteristic) -> Vec<u8> {
    vec![
        b'R',
        b'B',
        1,
        if kind == Characteristic::Result { 3 } else { 1 },
        1,
        0,
        0,
        0,
        1,
        0,
        3,
        0,
        b'a',
        b'b',
        b'c',
    ]
}
fn options(write: bool) -> HashMap<String, OwnedValue> {
    let mut values = HashMap::from([
        (
            "device".into(),
            Value::from(OwnedObjectPath::try_from(DEVICE).unwrap())
                .try_into()
                .unwrap(),
        ),
        ("mtu".into(), OwnedValue::from(23u16)),
        ("link".into(), Value::from("LE").try_into().unwrap()),
    ]);
    if write {
        values.insert("type".into(), Value::from("request").try_into().unwrap());
    }
    values
}
fn command() -> tokio::task::JoinHandle<String> {
    tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        assert!(
            std::io::stdin().read_line(&mut line).unwrap() > 0,
            "fixture command EOF"
        );
        line.trim().to_owned()
    })
}
async fn connect(address: &str) -> Connection {
    zbus::connection::Builder::address(address)
        .unwrap()
        .build()
        .await
        .unwrap()
}
struct Device(Arc<AtomicBool>);
#[zbus::interface(name = "org.bluez.Device1")]
impl Device {
    #[zbus(property)]
    fn connected(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    #[zbus(property)]
    fn adapter(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/org/bluez/hci0").unwrap()
    }
}
struct Manager(Arc<AtomicUsize>);
#[zbus::interface(name = "org.bluez.GattManager1")]
impl Manager {
    fn register_application(&self, path: OwnedObjectPath, options: HashMap<String, OwnedValue>) {
        assert_eq!(path.as_str(), manifest::ROOT);
        assert!(options.is_empty());
        assert_eq!(self.0.fetch_add(1, Ordering::AcqRel), 0);
    }
    fn unregister_application(&self, path: OwnedObjectPath) {
        assert_eq!(path.as_str(), manifest::ROOT);
        assert_eq!(self.0.fetch_add(1, Ordering::AcqRel), 1);
    }
}
async fn acquire(
    connection: &Connection,
    target: &str,
    kind: Characteristic,
) -> zbus::Result<(Socket, u16)> {
    let proxy = Proxy::new(connection, target, kind.path(), IFACE).await?;
    let (fd, mtu): (OwnedFd, u16) = proxy.call("AcquireNotify", &(options(false),)).await?;
    Ok((Socket::from(std::os::fd::OwnedFd::from(fd)), mtu))
}
async fn packet(socket: &Socket) -> Vec<u8> {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let mut data = [0; 256];
            let mut socket = socket;
            match socket.read(&mut data) {
                Ok(n) => return data[..n].to_vec(),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    tokio::time::sleep(Duration::from_millis(5)).await
                }
                Err(e) => panic!("real notify FD: {e}"),
            }
        }
    })
    .await
    .expect("packet deadline")
}
async fn bluez(address: &str) {
    let connected = Arc::new(AtomicBool::new(true));
    let calls = Arc::new(AtomicUsize::new(0));
    let connection = zbus::connection::Builder::address(address)
        .unwrap()
        .name("org.bluez")
        .unwrap()
        .serve_at("/org/bluez/hci0", Manager(calls.clone()))
        .unwrap()
        .serve_at(DEVICE, Device(connected.clone()))
        .unwrap()
        .build()
        .await
        .unwrap();
    println!("GATT_READY {}", connection.unique_name().unwrap());
    let mut sockets = Vec::new();
    loop {
        let line = command().await.unwrap();
        let (op, target) = line.split_once(' ').unwrap_or((&line, ""));
        match op {
            "ACQUIRE" => {
                assert_eq!(calls.load(Ordering::Acquire), 1);
                let objects: Objects = Proxy::new(
                    &connection,
                    target,
                    manifest::ROOT,
                    "org.freedesktop.DBus.ObjectManager",
                )
                .await
                .unwrap()
                .call("GetManagedObjects", &())
                .await
                .unwrap();
                assert_eq!(objects.len(), 5);
                // Valid broker headers still require the actual GATT body and
                // minimum MTU. Rejection must not consume the one-shot FD slot.
                let proxy = Proxy::new(
                    &connection,
                    target,
                    Characteristic::AuthExchange.path(),
                    IFACE,
                )
                .await
                .unwrap();
                let malformed: zbus::Result<(OwnedFd, u16)> =
                    proxy.call("AcquireNotify", &(vec![1u8],)).await;
                assert!(malformed.is_err());
                let mut too_small = options(false);
                too_small.insert("mtu".into(), OwnedValue::from(22u16));
                let rejected: zbus::Result<(OwnedFd, u16)> =
                    proxy.call("AcquireNotify", &(too_small,)).await;
                assert!(rejected.is_err());
                println!("GATT_PREFLIGHT_REJECTED");
                for kind in [Characteristic::AuthExchange, Characteristic::Result] {
                    let values: HashMap<String, OwnedValue> = Proxy::new(
                        &connection,
                        target,
                        kind.path(),
                        "org.freedesktop.DBus.Properties",
                    )
                    .await
                    .unwrap()
                    .call("GetAll", &("org.bluez.GattCharacteristic1",))
                    .await
                    .unwrap();
                    assert!(!values.contains_key("Value"));
                    let (fd, mtu) = acquire(&connection, target, kind).await.unwrap();
                    assert_eq!(mtu, 23);
                    assert_eq!(fd.r#type().unwrap(), socket2::Type::SEQPACKET);
                    sockets.push(fd);
                    assert!(acquire(&connection, target, kind).await.is_err());
                }
                let bytes: Vec<u8> = Proxy::new(
                    &connection,
                    target,
                    Characteristic::DeviceInfo.path(),
                    IFACE,
                )
                .await
                .unwrap()
                .call("ReadValue", &(options(false),))
                .await
                .unwrap();
                assert_eq!(bytes, manifest::DEVICE_INFO);
                println!("GATT_ACQUIRED");
            }
            "RECEIVE" => {
                assert_eq!(
                    packet(&sockets[0]).await,
                    frame(Characteristic::AuthExchange)
                );
                assert_eq!(packet(&sockets[1]).await, frame(Characteristic::Result));
                println!("GATT_PACKETS");
            }
            "WRITE" => {
                let _: () = Proxy::new(
                    &connection,
                    target,
                    Characteristic::AuthExchange.path(),
                    IFACE,
                )
                .await
                .unwrap()
                .call(
                    "WriteValue",
                    &(frame(Characteristic::AuthExchange), options(true)),
                )
                .await
                .unwrap();
                println!("GATT_WRITE_ACK");
            }
            "DISCONNECT" => {
                connected.store(false, Ordering::Release);
                let device = connection
                    .object_server()
                    .interface::<_, Device>(DEVICE)
                    .await
                    .unwrap();
                device
                    .get()
                    .await
                    .connected_changed(device.signal_emitter())
                    .await
                    .unwrap();
                println!("GATT_DISCONNECTED");
            }
            "CLOSED" => {
                for socket in &sockets {
                    assert!(packet(socket).await.is_empty());
                }
                println!("GATT_FDS_CLOSED");
            }
            "QUIT" => {
                assert_eq!(calls.load(Ordering::Acquire), 2);
                break;
            }
            _ => panic!("unknown test command"),
        }
    }
}
async fn ble(address: &str, owner: &str) {
    let connection = connect(address).await;
    let name = connection.unique_name().unwrap().to_string();
    let mut control = BluezControl::bind(connection, owner, Adapter::Hci0)
        .await
        .unwrap();
    let lease = PeerLease::bind(&control, epoch(), DEVICE).await.unwrap();
    let peer = lease.peer().clone();
    let (export, mut requests) = GattExport::publish_bound(&control, lease).await.unwrap();
    control.register_application().await.unwrap();
    println!("GATT_READY {name}");
    let mut input = command();
    let mut writes = 0;
    loop {
        tokio::select! {
            request=requests.recv()=>{
                let request=request.unwrap();
                assert_eq!(request.epoch(),epoch());assert_eq!(request.peer(),&peer);
                assert_eq!(request.characteristic(),Characteristic::AuthExchange);
                match request.operation() {
                    CallbackOperation::Write{frame:bytes,..}=>assert_eq!(bytes.as_slice(),frame(Characteristic::AuthExchange)),
                    _=>panic!("wrong opaque host operation"),
                }
                request.check_current().await.unwrap();
                writes+=1;request.finish(Ok(CallbackReply::Accepted));
            }
            line=&mut input=>{
                match line.unwrap().as_str() {
                    "NOTIFY"=>{
                        for kind in [Characteristic::AuthExchange,Characteristic::Result] {
                            export.notify(epoch(),peer.clone(),kind,OpaqueBytes::new(frame(kind)).unwrap()).await.unwrap();
                        }
                        println!("GATT_NOTIFY_ACCEPTED");
                    }
                    "FENCE"=>{
                        assert_eq!(writes,1);
                        assert_eq!(export.check_peer_current().await,Err(Error::StaleContext));
                        assert_eq!(export.notify(epoch(),peer.clone(),Characteristic::Result,OpaqueBytes::new(frame(Characteristic::Result)).unwrap()).await,Err(Error::StaleContext));
                        println!("GATT_FENCED");
                    }
                    "STOP"=>{
                        control.unregister_application().await.unwrap();
                        export.close_connection().await.unwrap();break;
                    }
                    _=>panic!("unknown test command"),
                }
                input=command();
            }
        }
    }
}
#[tokio::test(flavor = "current_thread")]
async fn gatt_kernel_uid_role() {
    assert!(
        std::path::Path::new("/.dockerenv").exists(),
        "explicit container runner required"
    );
    let role = std::env::var("GATT_FIXTURE_ROLE").expect("explicit role");
    let uid = match role.as_str() {
        "ble" => 1002,
        "bluez" => 1003,
        "foreign" => 1004,
        _ => panic!("unknown role"),
    };
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let actual: Vec<_> = status
        .lines()
        .find(|l| l.starts_with("Uid:"))
        .unwrap()
        .split_whitespace()
        .skip(1)
        .collect();
    assert_eq!(actual, vec![uid.to_string(); 4]);
    assert!(status.contains("CapEff:\t0000000000000000"));
    assert!(status.contains("NoNewPrivs:\t1"));
    let address = std::env::var("GATT_FIXTURE_BUS").unwrap();
    tokio::time::timeout(Duration::from_secs(20),async {
        match role.as_str(){
            "bluez"=>bluez(&address).await,
            "ble"=>ble(&address,&std::env::var("GATT_FIXTURE_OWNER").unwrap()).await,
            "foreign"=>{
                let connection=connect(&address).await;
                let target=std::env::var("GATT_FIXTURE_TARGET").unwrap();
                let error=acquire(&connection,&target,Characteristic::AuthExchange).await.unwrap_err();
                assert!(matches!(error,zbus::Error::MethodError(ref name,_,_) if name.as_str()=="org.freedesktop.DBus.Error.AccessDenied"),"{error:?}");
                println!("GATT_FOREIGN_DENIED");
            }
            _=>unreachable!(),
        }
    }).await.expect("bounded cross UID experiment");
}
