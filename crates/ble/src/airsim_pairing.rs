// SPDX-License-Identifier: AGPL-3.0-only

mod backend;
mod control;
mod store;
mod tx_monitor;

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
const MAX_GATT_VALUE: usize = 512;
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
    links: [Option<Link>; 3],
    frames: Fragments,
    call_frames: Fragments,
    calls: Option<mpsc::Sender<Vec<u8>>>,
    microphone: Option<mpsc::Sender<control::Microphone>>,
    worker: Option<tokio::task::JoinHandle<()>>,
    pairing: Pairing,
    reply_id: u16,
}

impl Drop for Peer {
    fn drop(&mut self) {
        if let Some(worker) = &self.worker {
            worker.abort();
        }
    }
}

#[derive(Clone)]
struct Link {
    packet: usize,
    stream: Arc<AsyncFd<Socket>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Channel {
    Pairing = 0,
    Control = 1,
    Audio = 2,
}

impl Peer {
    fn link(&self, channel: Channel) -> Option<&Link> {
        self.links[channel as usize].as_ref()
    }

    fn primary(&self) -> Option<&Link> {
        self.links.iter().flatten().next()
    }

    fn matches(&self, link: &Link) -> bool {
        self.link(Channel::Pairing)
            .is_some_and(|current| Arc::ptr_eq(&current.stream, &link.stream))
    }
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
        self.0.acquired(Channel::Pairing)
    }

    async fn acquire_notify(
        &self,
        options: Options,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<(OwnedFd, u16), GattError> {
        verify(&self.0.scope, &header).await?;
        self.0.acquire(&options, Channel::Pairing)
    }

    async fn write_value(
        &self,
        value: Vec<u8>,
        options: Options,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<(), GattError> {
        verify(&self.0.scope, &header).await?;
        let device = device(&options)?;
        // BlueZ reassembles an ATT long write before delivering WriteValue.
        // The negotiated packet size limits outgoing indications only.
        if value.len() > MAX_GATT_VALUE
            || options
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
            .filter(|peer| peer.device == device)
            .ok_or_else(rejected)?;
        let link = peer.link(Channel::Pairing).cloned().ok_or_else(rejected)?;
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
        let frames = voble::fragment(&bytes, peer.reply_id, link.packet).map_err(|_| rejected())?;
        permit.send(Reply {
            stream: link.stream.clone(),
            frames,
            ready,
        });
        if let (true, Some(credential), Some(secret)) = (ready, disk.credential(), disk.bridge()) {
            let (tx, rx) = mpsc::channel(4);
            peer.calls = Some(tx);
            peer.worker = Some(tokio::spawn(control::run(
                Arc::downgrade(&self.0),
                link,
                backend::Backend(secret),
                zeroize::Zeroizing::new(*credential.keys().0),
                disk.identity().to_owned(),
                rx,
            )));
        }
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
    context: Arc<Context>,
    uuid: &'static str,
    flag: &'static str,
}

// iOS subscribes to the control and audio outputs before it authenticates.
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
        verify(&self.context.scope, &header).await?;
        if self.flag != "read" {
            return Err(GattError::NotSupported("pairing only".into()));
        }
        let offset = match options.get("offset") {
            Some(value) => usize::from(u16::try_from(value).map_err(|_| rejected())?),
            None => 0,
        };
        b"VoBLE/1;g711;control-auth-required"
            .get(offset..)
            .map(<[u8]>::to_vec)
            .ok_or_else(rejected)
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn notify_acquired(&self) -> bool {
        self.channel()
            .is_some_and(|channel| self.context.acquired(channel))
    }

    async fn acquire_notify(
        &self,
        options: Options,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<(OwnedFd, u16), GattError> {
        verify(&self.context.scope, &header).await?;
        self.context
            .acquire(&options, self.channel().ok_or_else(rejected)?)
    }

    async fn write_value(
        &self,
        value: Vec<u8>,
        options: Options,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<(), GattError> {
        verify(&self.context.scope, &header).await?;
        let device = device(&options)?;
        let kind = match self.flag {
            "write" => "request",
            "write-without-response" => "command",
            _ => return Err(rejected()),
        };
        if value.len() > MAX_GATT_VALUE
            || options.contains_key("prepare-authorize")
            || options
                .get("offset")
                .is_some_and(|v| u16::try_from(v) != Ok(0))
            || options
                .get("type")
                .is_some_and(|v| <&str>::try_from(v) != Ok(kind))
        {
            return Err(rejected());
        }
        let mut state = self.context.state.lock().map_err(|_| rejected())?;
        let peer = state
            .peer
            .as_mut()
            .filter(|peer| peer.device == device && peer.pairing.authenticated())
            .ok_or_else(rejected)?;
        let now = Instant::now();
        if kind == "command" {
            if let Some(microphone) = &peer.microphone {
                // A stalled radio/PCM consumer loses a frame rather than buffering speech.
                let _ = microphone.try_send(control::Microphone {
                    bytes: value,
                    received: now,
                });
            }
            return Ok(());
        }
        let calls = peer.calls.as_ref().ok_or_else(rejected)?;
        if let Some(data) = peer.call_frames.push(&value, now).map_err(|_| rejected())? {
            calls.try_send(data).map_err(|_| rejected())?;
        }
        Ok(())
    }
}

impl Characteristic {
    fn channel(&self) -> Option<Channel> {
        match self.flag {
            "notify" => Some(Channel::Audio),
            "indicate" => Some(Channel::Control),
            _ => None,
        }
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
                    context: context.clone(),
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
    fn acquired(&self, channel: Channel) -> bool {
        self.state.lock().is_ok_and(|state| {
            state
                .peer
                .as_ref()
                .and_then(|peer| peer.link(channel))
                .is_some()
        })
    }

    fn acquire(&self, options: &Options, channel: Channel) -> Result<(OwnedFd, u16), GattError> {
        let device = device(options)?;
        let mtu = options
            .get("mtu")
            .and_then(|value| u16::try_from(value).ok())
            .filter(|mtu| (23..=517).contains(mtu))
            .ok_or_else(rejected)?;
        let mut state = self.state.lock().map_err(|_| rejected())?;
        if !state.accepting {
            return Err(rejected());
        }
        let State {
            peer,
            disk,
            deadline,
            ..
        } = &mut *state;
        let peer = peer.get_or_insert_with(|| Peer {
            device: device.to_owned(),
            links: std::array::from_fn(|_| None),
            frames: Fragments::default(),
            call_frames: Fragments::control(),
            pairing: Pairing::new(disk.identity().to_owned(), *deadline),
            reply_id: 0,
            calls: None,
            microphone: None,
            worker: None,
        });
        if peer.device != device || peer.link(channel).is_some() {
            return Err(rejected());
        }
        let (local, remote) =
            Socket::pair(Domain::UNIX, Type::SEQPACKET, None).map_err(|_| rejected())?;
        if channel == Channel::Audio {
            local.set_send_buffer_size(2048).map_err(|_| rejected())?;
        }
        local.set_nonblocking(true).map_err(|_| rejected())?;
        let stream = Arc::new(AsyncFd::new(local).map_err(|_| rejected())?);
        peer.links[channel as usize] = Some(Link {
            packet: usize::from(mtu - 3).min(MAX_GATT_VALUE),
            stream,
        });
        self.changed.notify_one();
        Ok((OwnedFd::from(std::os::fd::OwnedFd::from(remote)), mtu))
    }

    fn output(&self, origin: &Link, channel: Channel) -> Option<Link> {
        let state = self.state.lock().ok()?;
        let peer = state.peer.as_ref().filter(|peer| peer.matches(origin))?;
        peer.link(channel).cloned()
    }

    fn microphone(
        &self,
        origin: &Link,
        value: Option<mpsc::Sender<control::Microphone>>,
    ) -> Option<()> {
        let mut state = self.state.lock().ok()?;
        let peer = state.peer.as_mut().filter(|peer| peer.matches(origin))?;
        peer.microphone = value;
        Some(())
    }

    fn revoke(&self, origin: &Link) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("VoBLE state unavailable"))?;
        if !state.peer.as_ref().is_some_and(|peer| peer.matches(origin)) {
            return Err(io::ErrorKind::NotConnected.into());
        }
        state.disk.revoke()
    }

    fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.accepting = false;
            state.peer = None;
        }
    }

    fn remove(&self, stream: &Arc<AsyncFd<Socket>>) {
        if let Ok(mut state) = self.state.lock() {
            if state.peer.as_ref().is_some_and(|peer| {
                peer.links
                    .iter()
                    .flatten()
                    .any(|link| Arc::ptr_eq(&link.stream, stream))
            }) {
                state.peer = None;
            }
        }
    }

    fn stream(&self) -> io::Result<Option<Arc<AsyncFd<Socket>>>> {
        let state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("VoBLE state unavailable"))?;
        Ok(state
            .peer
            .as_ref()
            .and_then(Peer::primary)
            .map(|link| link.stream.clone()))
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
    if !matches!(command, "pair" | "serve" | "status" | "revoke" | "link") {
        return Err(io::Error::other("expected pair, serve, status, revoke or link").into());
    }
    if command == "pair" && !(io::stdin().is_terminal() && io::stderr().is_terminal()) {
        return Err(io::Error::other("pair requires an interactive terminal").into());
    }
    let mut disk = store::Disk::open(path)?;
    if command == "link" {
        if !io::stdin().is_terminal() {
            return Err(io::Error::other("link requires an interactive terminal").into());
        }
        if disk.bridge().is_some() {
            // The previous bridge may already have been revoked in the Owner UI.
            if !matches!(
                backend::Backend(disk.bridge().unwrap())
                    .call("GET", "/api/health", serde_json::Value::Null)
                    .await,
                Err(backend::Status::Revoked)
            ) {
                return Err(io::Error::other(
                    "revoke the old VoBLE bridge in the AirSIM web panel first",
                )
                .into());
            }
        }
        eprintln!("Open a pairing window in the RustSimHub AirSIM web panel, then enter its six-digit code:");
        let mut code = zeroize::Zeroizing::new(String::new());
        io::stdin().read_line(&mut code)?;
        disk.link(backend::enroll(code.trim()).await?)?;
        println!("VoBLE bridge linked to this device.");
        return Ok(());
    }
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
