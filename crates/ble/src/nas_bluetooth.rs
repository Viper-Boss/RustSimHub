// SPDX-License-Identifier: AGPL-3.0-only
//! Two Classic Bluetooth application streams to fixed root-only Edge endpoints.
use crate::bluez_control::{Adapter, BluezControl, CallbackScope};
use rustsimhub_airsim::voble::{LOCAL_CONTROL, LOCAL_PAIRING, LOCAL_PCM};
use std::{
    collections::HashMap, io, os::fd::OwnedFd as StdFd, os::linux::net::SocketAddrExt, sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{unix::SocketAddr, UnixStream},
    sync::{mpsc, Notify},
    task::JoinSet,
    time::timeout,
};
use zbus::{
    fdo::DBusProxy,
    message::Header,
    names::BusName,
    zvariant::{ObjectPath, OwnedFd, OwnedValue, Value},
    Connection, Proxy,
};

const CONTROL_UUID: &str = "0beeef82-8c73-4fc4-a420-28ae354ccee2";
const AUDIO_UUID: &str = "0beeef82-8c73-4fc4-a420-28ae354ccee3";
const CONTROL_PATH: &str = "/org/rustsimhub/nas/control";
const AUDIO_PATH: &str = "/org/rustsimhub/nas/audio";
const BUS_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Link {
    Control,
    Audio,
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
enum ProfileError {
    Rejected(String),
}

fn rejected() -> ProfileError {
    ProfileError::Rejected("approved bonded NAS required".into())
}

struct Profile {
    scope: CallbackScope,
    peer: String,
    bluez: String,
    bus: Connection,
    link: Link,
    accept: mpsc::Sender<(Link, StdFd)>,
    stop: Arc<Notify>,
}

impl Profile {
    async fn verify(&self, header: &Header<'_>) -> Result<(), ProfileError> {
        timeout(
            BUS_TIMEOUT,
            self.scope
                .check_sender(header.sender().map(|name| name.as_str())),
        )
        .await
        .map_err(|_| rejected())?
        .map(|_| ())
        .map_err(|_| rejected())
    }
}

#[zbus::interface(name = "org.bluez.Profile1")]
impl Profile {
    async fn release(&self, #[zbus(header)] header: Header<'_>) {
        if self.verify(&header).await.is_ok() {
            self.stop.notify_one();
        }
    }

    async fn new_connection(
        &self,
        device: ObjectPath<'_>,
        fd: OwnedFd,
        _properties: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<(), ProfileError> {
        self.verify(&header).await?;
        if device.as_str() != self.peer {
            return Err(rejected());
        }
        let bonded = timeout(BUS_TIMEOUT, async {
            let proxy =
                Proxy::new(&self.bus, self.bluez.as_str(), device, "org.bluez.Device1").await?;
            proxy.get_property::<bool>("Paired").await
        })
        .await;
        if !matches!(bonded, Ok(Ok(true))) {
            return Err(rejected());
        }
        self.accept
            .try_send((self.link, StdFd::from(fd)))
            .map_err(|_| rejected())
    }

    async fn request_disconnection(
        &self,
        device: ObjectPath<'_>,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<(), ProfileError> {
        self.verify(&header).await?;
        if device.as_str() != self.peer {
            return Err(rejected());
        }
        self.stop.notify_one();
        Ok(())
    }
}

pub(crate) fn peer_path(address: &str) -> Option<String> {
    let bytes = address.split(':').collect::<Vec<_>>();
    if bytes.len() != 6
        || bytes
            .iter()
            .any(|byte| byte.len() != 2 || !byte.bytes().all(|c| c.is_ascii_hexdigit()))
        || bytes.iter().all(|byte| *byte == "00")
        || bytes.iter().all(|byte| byte.eq_ignore_ascii_case("FF"))
    {
        return None;
    }
    Some(format!(
        "/org/bluez/hci0/dev_{}",
        bytes.join("_").to_ascii_uppercase()
    ))
}

async fn connect_local(name: &[u8]) -> io::Result<UnixStream> {
    let address = std::os::unix::net::SocketAddr::from_abstract_name(name)?;
    UnixStream::connect_addr(&SocketAddr::from(address)).await
}

async fn relay(
    mut remote: UnixStream,
    link: Link,
    connect: impl AsyncFnOnce(&[u8]) -> io::Result<UnixStream>,
) -> io::Result<()> {
    let route = if link == Link::Audio {
        LOCAL_PCM
    } else {
        match timeout(BUS_TIMEOUT, remote.read_u8()).await?? {
            1 => LOCAL_CONTROL,
            2 => LOCAL_PAIRING,
            _ => return Err(io::ErrorKind::InvalidData.into()),
        }
    };
    if link == Link::Control {
        let line = timeout(
            BUS_TIMEOUT,
            read_request_line(&mut remote, route == LOCAL_PAIRING),
        )
        .await??;
        let mut local = timeout(BUS_TIMEOUT, connect(route)).await??;
        timeout(BUS_TIMEOUT, local.write_all(&line)).await??;
        timeout(
            Duration::from_secs(42),
            tokio::io::copy_bidirectional(&mut remote, &mut local),
        )
        .await??;
        return Ok(());
    }
    let mut local = timeout(BUS_TIMEOUT, connect(route)).await??;
    let (mut remote_r, mut remote_w) = remote.split();
    let (mut local_r, mut local_w) = local.split();
    tokio::select! {
        result = audio_leg(&mut remote_r, &mut local_w) => result,
        result = audio_leg(&mut local_r, &mut remote_w) => result,
    }
}

async fn read_request_line(remote: &mut UnixStream, pairing: bool) -> io::Result<Vec<u8>> {
    let invalid = || io::Error::from(io::ErrorKind::InvalidData);
    let mut line = Vec::with_capacity(128);
    for _ in 0..640 {
        line.push(remote.read_u8().await?);
        if !line.ends_with(b"\r\n") {
            continue;
        }
        let value = std::str::from_utf8(&line[..line.len() - 2]).map_err(|_| invalid())?;
        let mut fields = value.split(' ');
        let (Some(method), Some(target), Some(version)) =
            (fields.next(), fields.next(), fields.next())
        else {
            return Err(invalid());
        };
        if fields.next().is_some() || version != "HTTP/1.1" {
            return Err(invalid());
        }
        let allowed = if pairing {
            matches!(
                (method, target),
                ("GET", "/pair/v1/info") | ("POST", "/pair/v1/claim")
            )
        } else {
            match (method, target) {
                (
                    "GET",
                    "/v1/health"
                    | "/api/status"
                    | "/api/sms"
                    | "/api/calls/status?observe=1&history=1",
                ) => true,
                ("GET", target) => target
                    .strip_prefix("/api/calls/status?observe=1&own=")
                    .is_some_and(|id| {
                        !id.is_empty()
                            && id
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b"%-._".contains(&b))
                    }),
                (
                    "POST",
                    "/api/calls/dial" | "/api/calls/answer" | "/api/calls/reject"
                    | "/api/calls/hangup" | "/api/calls/dtmf",
                ) => true,
                _ => false,
            }
        };
        return allowed.then_some(line).ok_or_else(invalid);
    }
    Err(invalid())
}

async fn audio_leg(
    reader: &mut (impl tokio::io::AsyncRead + Unpin),
    writer: &mut (impl tokio::io::AsyncWrite + Unpin),
) -> io::Result<()> {
    let mut buffer = [0; 640];
    loop {
        let bytes = timeout(Duration::from_secs(3), reader.read(&mut buffer)).await??;
        if bytes == 0 {
            return Ok(());
        }
        // If RFCOMM stalls, close this stream rather than waiting on old speech.
        timeout(
            Duration::from_millis(60),
            writer.write_all(&buffer[..bytes]),
        )
        .await??;
    }
}

pub async fn run(address: &str) -> Result<(), Box<dyn std::error::Error>> {
    let peer = peer_path(address).ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let bus = timeout(BUS_TIMEOUT, Connection::system()).await??;
    let dbus = DBusProxy::new(&bus).await?;
    let owner = timeout(
        BUS_TIMEOUT,
        dbus.get_name_owner(BusName::try_from("org.bluez")?),
    )
    .await??;
    let control = BluezControl::bind(bus.clone(), owner.as_str(), Adapter::Hci0)
        .await
        .map_err(|_| io::Error::other("BlueZ unavailable"))?;
    let manager = Proxy::new(
        &bus,
        owner.as_str(),
        "/org/bluez",
        "org.bluez.ProfileManager1",
    )
    .await?;
    let (accept, mut incoming) = mpsc::channel(4);
    let stop = Arc::new(Notify::new());
    for (path, uuid, channel, link) in [
        (CONTROL_PATH, CONTROL_UUID, 23u16, Link::Control),
        (AUDIO_PATH, AUDIO_UUID, 24u16, Link::Audio),
    ] {
        let path = ObjectPath::try_from(path)?;
        bus.object_server()
            .at(
                path.clone(),
                Profile {
                    scope: control.callback_scope(),
                    peer: peer.clone(),
                    bluez: owner.to_string(),
                    bus: bus.clone(),
                    link,
                    accept: accept.clone(),
                    stop: stop.clone(),
                },
            )
            .await?;
        let options = HashMap::from([
            ("Name", Value::from("RustSimHub NAS")),
            ("Role", Value::from("server")),
            ("Channel", Value::from(channel)),
            ("RequireAuthentication", Value::from(true)),
        ]);
        timeout(
            BUS_TIMEOUT,
            manager.call::<_, _, ()>("RegisterProfile", &(path, uuid, options)),
        )
        .await??;
    }
    notify_ready()?;
    let mut tasks = JoinSet::new();
    let mut media_open = false;
    let mut control_open = 0usize;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let lost = control.wait_owner_loss();
    tokio::pin!(lost);
    loop {
        tokio::select! {
            biased;
            _ = &mut lost => return Err(io::Error::other("BlueZ owner lost").into()),
            _ = stop.notified() => return Err(io::Error::other("Bluetooth profile disconnected").into()),
            _ = terminate.recv() => break,
            _ = tokio::signal::ctrl_c() => break,
            Some(result) = tasks.join_next(), if !tasks.is_empty() => match result? {
                Link::Audio => media_open = false,
                Link::Control => control_open -= 1,
            },
            Some((link, fd)) = incoming.recv() => {
                let stream = std::os::unix::net::UnixStream::from(fd);
                stream.set_nonblocking(true)?;
                let stream = UnixStream::from_std(stream)?;
                match link {
                    Link::Audio if media_open => continue,
                    Link::Audio => media_open = true,
                    Link::Control if control_open >= 3 => continue,
                    Link::Control => control_open += 1,
                }
                tasks.spawn(async move {
                    let _ = relay(stream, link, connect_local).await;
                    link
                });
            }
        }
    }
    tasks.shutdown().await;
    for path in [AUDIO_PATH, CONTROL_PATH] {
        let path = ObjectPath::try_from(path)?;
        let _ = timeout(
            BUS_TIMEOUT,
            manager.call::<_, _, ()>("UnregisterProfile", &(path,)),
        )
        .await;
    }
    bus.close().await?;
    Ok(())
}

fn notify_ready() -> io::Result<()> {
    let Some(socket) = std::env::var_os("NOTIFY_SOCKET") else {
        return Ok(());
    };
    use std::{os::unix::ffi::OsStrExt, os::unix::net::UnixDatagram};
    let address = if let Some(name) = socket.as_bytes().strip_prefix(b"@") {
        std::os::unix::net::SocketAddr::from_abstract_name(name)?
    } else {
        std::os::unix::net::SocketAddr::from_pathname(socket)?
    };
    UnixDatagram::unbound()?.send_to_addr(b"READY=1", &address)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn classic_control_keeps_the_signed_path_and_rejects_other_protocols() {
        let line = b"GET /api/calls/status?observe=1&own=call%2F1 HTTP/1.1\r\n";
        let (mut nas, mut stick) = UnixStream::pair().unwrap();
        nas.write_all(line).await.unwrap();
        assert_eq!(read_request_line(&mut stick, false).await.unwrap(), line);
        for request in [
            "CONNECT example.com:443 HTTP/1.1\r\n",
            "GET http://example.com/ HTTP/1.1\r\n",
            "POST /api/sms/send HTTP/1.1\r\n",
            "POST /api/calls/dial?extra=true HTTP/1.1\r\n",
            "POST /pair/v1/claim HTTP/1.1\r\n",
            "GET /api/calls/status?observe=1&own=x&history=1 HTTP/1.1\r\n",
        ] {
            nas.write_all(request.as_bytes()).await.unwrap();
            assert!(read_request_line(&mut stick, false).await.is_err());
        }
        nas.write_all(b"POST /pair/v1/claim HTTP/1.1\r\n")
            .await
            .unwrap();
        assert!(read_request_line(&mut stick, true).await.is_ok());
    }

    #[tokio::test]
    async fn separate_media_route_passes_both_directions_and_closes_at_eof() {
        let (mut nas, stick) = UnixStream::pair().unwrap();
        let (core, mut media) = UnixStream::pair().unwrap();
        let task = tokio::spawn(relay(stick, Link::Audio, async move |name: &[u8]| {
            assert_eq!(name, LOCAL_PCM);
            Ok(core)
        }));
        nas.write_all(b"microphone").await.unwrap();
        media.write_all(b"speaker").await.unwrap();
        let mut mic = [0; 10];
        let mut spk = [0; 7];
        media.read_exact(&mut mic).await.unwrap();
        nas.read_exact(&mut spk).await.unwrap();
        assert_eq!(&mic, b"microphone");
        assert_eq!(&spk, b"speaker");
        drop(nas);
        task.await.unwrap().unwrap();
        assert_eq!(media.read(&mut mic).await.unwrap(), 0);
    }
}
