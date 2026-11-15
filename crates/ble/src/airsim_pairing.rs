// SPDX-License-Identifier: AGPL-3.0-only

mod store;

use crate::bluez_control::{Adapter, BluezControl, CallbackScope};
use rustsimhub_airsim::voble::{self, Fragments, Pairing, Store};
use socket2::{Domain, Socket, Type};
use std::{
    collections::HashMap,
    future,
    io::{self, BufRead, IsTerminal, Read, Write},
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::unix::AsyncFd,
    sync::{mpsc, Notify},
    time::timeout,
};
use zbus::{
    fdo::ObjectManager,
    message::Header,
    names::BusName,
    zvariant::{ObjectPath, OwnedFd, OwnedValue},
    Connection, Proxy,
};

type Options = HashMap<String, OwnedValue>;
type DynError = Box<dyn std::error::Error>;
const ADAPTER: &str = "/org/bluez/hci0";
const ROOT: &str = "/org/rustsimhub/airsim";
const SERVICE: &str = "/org/rustsimhub/airsim/service";
const ADVERTISEMENT: &str = "/org/rustsimhub/airsim_advert";
const SERVICE_UUID: &str = "7e57a000-7a17-4d9f-9a3c-41a17b1e0001";
const PAIRING_UUID: &str = "7e57a002-7a17-4d9f-9a3c-41a17b1e0001";
const BUS_TIMEOUT: Duration = Duration::from_secs(3);
const REPLY_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
enum GattError {
    Failed(String),
    NotSupported(String),
}

fn rejected() -> GattError {
    // BlueZ must not fall back to characteristic-wide notifications.
    GattError::Failed("VoBLE callback unavailable".into())
}

async fn verify(scope: &CallbackScope, header: &Header<'_>) -> Result<(), GattError> {
    timeout(
        BUS_TIMEOUT,
        scope.check_sender(header.sender().map(|sender| sender.as_str())),
    )
    .await
    .map_err(|_| rejected())?
    .map_err(|_| rejected())?;
    Ok(())
}

struct State {
    disk: store::Disk,
    deadline: Option<Instant>,
    accepting: bool,
    peer: Option<Peer>,
}

struct Peer {
    device: String,
    packet: usize,
    stream: Arc<AsyncFd<Socket>>,
    frames: Fragments,
    pairing: Pairing,
    reply_id: u16,
}

struct Reply {
    stream: Arc<AsyncFd<Socket>>,
    frames: Vec<Vec<u8>>,
    ready: bool,
}

struct Context {
    scope: CallbackScope,
    state: Mutex<State>,
    changed: Notify,
    replies: mpsc::Sender<Reply>,
}

struct PairingCharacteristic(Arc<Context>);

#[zbus::interface(name = "org.bluez.GattCharacteristic1")]
impl PairingCharacteristic {
    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> &str {
        PAIRING_UUID
    }

    #[zbus(property)]
    fn service(&self) -> ObjectPath<'static> {
        ObjectPath::from_static_str_unchecked(SERVICE)
    }

    #[zbus(property)]
    fn flags(&self) -> Vec<&str> {
        vec!["write", "indicate"]
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn notify_acquired(&self) -> bool {
        self.0.state.lock().is_ok_and(|state| state.peer.is_some())
    }

    async fn acquire_notify(
        &self,
        options: Options,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<(OwnedFd, u16), GattError> {
        verify(&self.0.scope, &header).await?;
        let device = device(&options)?;
        let mtu = options
            .get("mtu")
            .and_then(|value| u16::try_from(value).ok())
            .filter(|mtu| (23..=517).contains(mtu))
            .ok_or_else(rejected)?;
        let mut state = self.0.state.lock().map_err(|_| rejected())?;
        if !state.accepting || state.peer.is_some() {
            return Err(rejected());
        }
        let (local, remote) =
            Socket::pair(Domain::UNIX, Type::SEQPACKET, None).map_err(|_| rejected())?;
        local.set_nonblocking(true).map_err(|_| rejected())?;
        let stream = Arc::new(AsyncFd::new(local).map_err(|_| rejected())?);
        state.peer = Some(Peer {
            device: device.to_owned(),
            packet: usize::from(mtu - 3).min(512),
            stream,
            frames: Fragments::default(),
            pairing: Pairing::new(state.disk.identity().to_owned(), state.deadline),
            reply_id: 0,
        });
        self.0.changed.notify_one();
        Ok((OwnedFd::from(std::os::fd::OwnedFd::from(remote)), mtu))
    }

    async fn write_value(
        &self,
        value: Vec<u8>,
        options: Options,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<(), GattError> {
        verify(&self.0.scope, &header).await?;
        let device = device(&options)?;
        if options
            .get("offset")
            .is_some_and(|v| u16::try_from(v) != Ok(0))
            || options.contains_key("prepare-authorize")
            || options
                .get("type")
                .is_some_and(|v| <&str>::try_from(v) != Ok("request"))
        {
            return Err(rejected());
        }
        let mut guard = self.0.state.lock().map_err(|_| rejected())?;
        if !guard.accepting {
            return Err(rejected());
        }
        let State { peer, disk, .. } = &mut *guard;
        let peer = peer
            .as_mut()
            .filter(|peer| peer.device == device && value.len() <= peer.packet)
            .ok_or_else(rejected)?;
        let now = Instant::now();
        let Some(data) = peer.frames.push(&value, now).map_err(|_| rejected())? else {
            return Ok(());
        };
        let permit = self.0.replies.try_reserve().map_err(|_| rejected())?;
        let result = peer.pairing.request(&data, now, disk);
        let response = match result {
            Ok(response) => {
                if response["op"] == "prepare" {
                    if let Some(code) = peer.pairing.code() {
                        eprintln!("VoBLE code: {code}. Compare it with the iPhone, then enter the code here before confirming on the iPhone.");
                    }
                }
                response
            }
            Err(error) => error.response(),
        };
        peer.reply_id = peer.reply_id.wrapping_add(1);
        let ready = response["ready"] == true;
        let bytes = serde_json::to_vec(&response).map_err(|_| rejected())?;
        let frames = voble::fragment(&bytes, peer.reply_id, peer.packet).map_err(|_| rejected())?;
        permit.send(Reply {
            stream: Arc::clone(&peer.stream),
            frames,
            ready,
        });
        Ok(())
    }
}

fn device(options: &Options) -> Result<&str, GattError> {
    if options
        .get("link")
        .and_then(|value| <&str>::try_from(value).ok())
        != Some("LE")
    {
        return Err(rejected());
    }
    let path = options
        .get("device")
        .and_then(|value| <&ObjectPath<'_>>::try_from(value).ok())
        .ok_or_else(rejected)?;
    let suffix = path
        .as_str()
        .strip_prefix("/org/bluez/hci0/dev_")
        .ok_or_else(rejected)?;
    if suffix.len() != 17
        || !suffix
            .split('_')
            .all(|octet| octet.len() == 2 && octet.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(rejected());
    }
    Ok(path.as_str())
}

struct Service;
#[zbus::interface(name = "org.bluez.GattService1")]
impl Service {
    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> &str {
        SERVICE_UUID
    }

    #[zbus(property)]
    fn primary(&self) -> bool {
        true
    }
}

struct Characteristic {
    scope: CallbackScope,
    uuid: &'static str,
    flag: &'static str,
}

// Current iOS requires these characteristic declarations before it sends begin.
// Control and media are disabled in this pairing service.
#[zbus::interface(name = "org.bluez.GattCharacteristic1")]
impl Characteristic {
    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> &str {
        self.uuid
    }

    #[zbus(property)]
    fn service(&self) -> ObjectPath<'static> {
        ObjectPath::from_static_str_unchecked(SERVICE)
    }

    #[zbus(property)]
    fn flags(&self) -> Vec<&str> {
        vec![self.flag]
    }

    async fn read_value(
        &self,
        options: Options,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<Vec<u8>, GattError> {
        verify(&self.scope, &header).await?;
        if self.flag != "read" {
            return Err(GattError::NotSupported("pairing only".into()));
        }
        let offset = match options.get("offset") {
            Some(value) => usize::from(u16::try_from(value).map_err(|_| rejected())?),
            None => 0,
        };
        b"VoBLE/1;pairing-only"
            .get(offset..)
            .map(<[u8]>::to_vec)
            .ok_or_else(rejected)
    }

    async fn write_value(
        &self,
        _value: Vec<u8>,
        _options: Options,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<(), GattError> {
        verify(&self.scope, &header).await?;
        Err(GattError::NotSupported("pairing only".into()))
    }

    async fn start_notify(&self, #[zbus(header)] header: Header<'_>) -> Result<(), GattError> {
        verify(&self.scope, &header).await
    }

    async fn stop_notify(&self, #[zbus(header)] header: Header<'_>) -> Result<(), GattError> {
        verify(&self.scope, &header).await
    }
}

struct Advert {
    scope: CallbackScope,
    released: Arc<Notify>,
}

#[zbus::interface(name = "org.bluez.LEAdvertisement1")]
impl Advert {
    #[zbus(property, name = "Type")]
    fn kind(&self) -> &str {
        "peripheral"
    }

    #[zbus(property, name = "ServiceUUIDs")]
    fn uuids(&self) -> Vec<&str> {
        vec![SERVICE_UUID]
    }

    #[zbus(property)]
    fn local_name(&self) -> &str {
        "RustSIM"
    }

    async fn release(&self, #[zbus(header)] header: Header<'_>) -> Result<(), GattError> {
        verify(&self.scope, &header).await?;
        self.released.notify_one();
        Ok(())
    }
}

async fn export(
    bus: &Connection,
    context: &Arc<Context>,
    released: Arc<Notify>,
) -> Result<(), DynError> {
    let objects = bus.object_server();
    objects.at(ROOT, ObjectManager).await?;
    objects.at(SERVICE, Service).await?;
    objects
        .at(
            format!("{SERVICE}/pairing"),
            PairingCharacteristic(context.clone()),
        )
        .await?;
    for (name, uuid, flag) in [
        ("capability", "7e57a001-7a17-4d9f-9a3c-41a17b1e0001", "read"),
        (
            "control_rx",
            "7e57a003-7a17-4d9f-9a3c-41a17b1e0001",
            "write",
        ),
        (
            "control_tx",
            "7e57a004-7a17-4d9f-9a3c-41a17b1e0001",
            "indicate",
        ),
        (
            "audio_up",
            "7e57a005-7a17-4d9f-9a3c-41a17b1e0001",
            "write-without-response",
        ),
        (
            "audio_down",
            "7e57a006-7a17-4d9f-9a3c-41a17b1e0001",
            "notify",
        ),
    ] {
        objects
            .at(
                format!("{SERVICE}/{name}"),
                Characteristic {
                    scope: context.scope.clone(),
                    uuid,
                    flag,
                },
            )
            .await?;
    }
    objects
        .at(
            ADVERTISEMENT,
            Advert {
                scope: context.scope.clone(),
                released,
            },
        )
        .await?;
    Ok(())
}

async fn send(reply: &Reply) -> io::Result<()> {
    timeout(REPLY_TIMEOUT, async {
        for frame in &reply.frames {
            loop {
                let mut guard = reply.stream.writable().await?;
                match guard.try_io(|fd| fd.get_ref().write(frame)) {
                    Ok(Ok(n)) if n == frame.len() => break,
                    Ok(Ok(_)) => return Err(io::ErrorKind::WriteZero.into()),
                    Ok(Err(error)) => return Err(error),
                    Err(_) => {}
                }
            }
            let mut confirmation = [0; 2];
            loop {
                let mut guard = reply.stream.readable().await?;
                match guard.try_io(|fd| fd.get_ref().read(&mut confirmation)) {
                    Ok(Ok(1)) if confirmation[0] == 1 => break,
                    Ok(Ok(_)) => return Err(io::ErrorKind::InvalidData.into()),
                    Ok(Err(error)) => return Err(error),
                    Err(_) => {}
                }
            }
        }
        Ok(())
    })
    .await?
}

impl Context {
    fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.accepting = false;
            state.peer = None;
        }
    }

    fn remove(&self, stream: &Arc<AsyncFd<Socket>>) {
        if let Ok(mut state) = self.state.lock() {
            if state
                .peer
                .as_ref()
                .is_some_and(|peer| Arc::ptr_eq(&peer.stream, stream))
            {
                state.peer = None;
            }
        }
    }

    fn stream(&self) -> io::Result<Option<Arc<AsyncFd<Socket>>>> {
        let state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("VoBLE state unavailable"))?;
        Ok(state.peer.as_ref().map(|peer| peer.stream.clone()))
    }

    fn approve(&self, code: &str) {
        let result = self.state.lock().ok().and_then(|mut state| {
            state
                .peer
                .as_mut()
                .map(|peer| peer.pairing.approve(code.trim(), Instant::now()))
        });
        if matches!(result, Some(Ok(()))) {
            eprintln!("Code confirmed on RustSimHub. Finish pairing on the iPhone.");
        } else {
            eprintln!("Code rejected. Reconnect and compare a new code.");
        }
    }
}

async fn wait_idle_end(stream: Option<&Arc<AsyncFd<Socket>>>) {
    let Some(stream) = stream else {
        return future::pending::<()>().await;
    };
    // An earlier successful read can leave a cached Tokio readiness event.
    // Stay connected on WouldBlock. EOF or unsolicited reverse data ends the peer.
    let _ = stream
        .async_io(tokio::io::Interest::READABLE, |mut fd| fd.read(&mut [0; 1]))
        .await;
}

async fn pairs(
    context: &Context,
    mut replies: mpsc::Receiver<Reply>,
    mut input: mpsc::Receiver<String>,
    deadline: Option<Instant>,
    released: &Notify,
    control: &BluezControl,
) -> Result<(), DynError> {
    let loss = control.wait_owner_loss();
    tokio::pin!(loss);
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);
    loop {
        let stream = context.stream()?;
        tokio::select! {
            biased;
            _ = &mut loss => return Err(io::Error::other("BlueZ owner changed").into()),
            _ = released.notified() => return Err(io::Error::other("BlueZ withdrew the advertisement").into()),
            result = &mut interrupt => { result?; return Ok(()); }
            _ = sigterm.recv() => return Ok(()),
            _ = async {
                if let Some(deadline) = deadline {
                    tokio::time::sleep_until(deadline.into()).await;
                } else {
                    future::pending::<()>().await;
                }
            } => return Ok(()),
            Some(reply) = replies.recv() => {
                if stream.as_ref().is_some_and(|stream| Arc::ptr_eq(stream, &reply.stream)) {
                    if send(&reply).await.is_err() {
                        context.remove(&reply.stream);
                    } else if reply.ready {
                        eprintln!("AirSIM pairing reply acknowledged over BLE.");
                    }
                }
            }
            Some(code) = input.recv() => context.approve(&code),
            _ = context.changed.notified() => {}
            _ = wait_idle_end(stream.as_ref()) => {
                if let Some(stream) = stream {
                    context.remove(&stream);
                }
            }
        }
    }
}

async fn registration(
    control: &BluezControl,
    advert: bool,
    register: bool,
) -> Result<(), DynError> {
    let (interface, path, method) = match (advert, register) {
        (false, true) => ("org.bluez.GattManager1", ROOT, "RegisterApplication"),
        (false, false) => ("org.bluez.GattManager1", ROOT, "UnregisterApplication"),
        (true, true) => (
            "org.bluez.LEAdvertisingManager1",
            ADVERTISEMENT,
            "RegisterAdvertisement",
        ),
        (true, false) => (
            "org.bluez.LEAdvertisingManager1",
            ADVERTISEMENT,
            "UnregisterAdvertisement",
        ),
    };
    let bus = control.connection();
    timeout(BUS_TIMEOUT, async {
        control
            .check_current()
            .await
            .map_err(|_| io::Error::other("BlueZ owner changed"))?;
        let proxy = Proxy::new(&bus, control.owner(), ADAPTER, interface).await?;
        let path = ObjectPath::try_from(path)?;
        if register {
            proxy
                .call::<_, _, ()>(method, &(path, Options::new()))
                .await?;
        } else {
            proxy.call::<_, _, ()>(method, &(path,)).await?;
        }
        Ok(())
    })
    .await?
}

async fn compatible_bluez(bus: &zbus::fdo::DBusProxy<'_>, owner: &str) -> Result<(), DynError> {
    let pid = bus
        .get_connection_unix_process_id(BusName::try_from(owner)?)
        .await?;
    let daemon = std::fs::read_link(format!("/proc/{pid}/exe"))?;
    // Require a distribution BlueZ binary with per-connection indication
    // delivery and the fd confirmation byte. Never execute an arbitrary path.
    if ![
        "/usr/libexec/bluetooth/bluetoothd",
        "/usr/lib/bluetooth/bluetoothd",
        "/usr/sbin/bluetoothd",
    ]
    .iter()
    .any(|path| daemon == Path::new(path))
    {
        return Err(io::Error::other("unrecognized bluetoothd executable").into());
    }
    let output = Command::new(daemon).arg("--version").output()?;
    let version = std::str::from_utf8(&output.stdout)?.trim();
    let (major, minor) = version
        .split_once('.')
        .ok_or_else(|| io::Error::other("unknown BlueZ version"))?;
    let version = (major.parse::<u32>()?, minor.parse::<u32>()?);
    if !output.status.success() || version < (5, 82) {
        return Err(io::Error::other("VoBLE pairing requires BlueZ 5.82 or newer").into());
    }
    Ok(())
}

pub async fn run(command: &str, path: &Path) -> Result<(), DynError> {
    if !matches!(command, "pair" | "serve" | "status" | "revoke") {
        return Err(io::Error::other("expected pair, serve, status or revoke").into());
    }
    if command == "pair" && !(io::stdin().is_terminal() && io::stderr().is_terminal()) {
        return Err(io::Error::other("pair requires an interactive terminal").into());
    }
    let mut disk = store::Disk::open(path)?;
    let paired = disk.credential().is_some();
    if command == "status" {
        println!("{}", if paired { "paired" } else { "unpaired" });
        return Ok(());
    }
    if command == "revoke" {
        disk.revoke()?;
        println!("VoBLE pairing revoked.");
        return Ok(());
    }
    if (command == "pair") == paired {
        let message = if paired {
            "VoBLE is already paired; use serve or revoke first"
        } else {
            "VoBLE is unpaired; open a local pairing window first"
        };
        return Err(io::Error::other(message).into());
    }
    let (bus, control) = timeout(BUS_TIMEOUT, async {
        let bus = Connection::system().await?;
        let proxy = zbus::fdo::DBusProxy::new(&bus).await?;
        let owner = proxy
            .get_name_owner(BusName::try_from("org.bluez")?)
            .await?;
        compatible_bluez(&proxy, owner.as_str()).await?;
        let control = BluezControl::bind(bus.clone(), owner.as_str(), Adapter::Hci0)
            .await
            .map_err(|_| io::Error::other("cannot bind BlueZ"))?;
        Ok::<_, DynError>((bus, control))
    })
    .await??;
    let (sender, replies) = mpsc::channel(1);
    let (input_tx, input_rx) = mpsc::channel(1);
    let deadline = (command == "pair").then(|| Instant::now() + voble::PAIRING_WINDOW);
    let context = Arc::new(Context {
        scope: control.callback_scope(),
        state: Mutex::new(State {
            disk,
            deadline,
            accepting: true,
            peer: None,
        }),
        changed: Notify::new(),
        replies: sender,
    });
    let released = Arc::new(Notify::new());
    export(&bus, &context, released.clone()).await?;
    registration(&control, false, true).await?;
    let result = match registration(&control, true, true).await {
        Ok(()) => {
            if command == "pair" {
                eprintln!("VoBLE pairing open for 120 seconds. Select RustSIM in AirSIM’s Bluetooth Agent screen.");
                std::thread::spawn(move || {
                    for line in io::stdin().lock().lines().map_while(Result::ok) {
                        if input_tx.blocking_send(line).is_err() {
                            break;
                        }
                    }
                });
            }
            let result = pairs(&context, replies, input_rx, deadline, &released, &control).await;
            context.close();
            let cleanup = registration(&control, true, false).await;
            result.and(cleanup)
        }
        Err(error) => {
            context.close();
            Err(error)
        }
    };
    let cleanup = registration(&control, false, false).await;
    control.retire();
    drop(context);
    let close = bus.close().await.map_err(Into::into);
    result.and(cleanup).and(close)
}

#[cfg(test)]
mod tests;
