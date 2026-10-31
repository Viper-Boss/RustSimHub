// SPDX-License-Identifier: AGPL-3.0-only
//! Per-peer, packet-boundary local submission. No global Value signal and no
//! claim of ATT/browser/business receipt. Uses standard AcquireNotify only.
use super::*;
use crate::ports::Pending;
use socket2::{Domain, Socket, Type};
use std::{
    io::{self, Read},
    net::Shutdown,
    sync::atomic::AtomicU8,
};
use tokio::io::unix::AsyncFd;
use zbus::zvariant::{OwnedFd, Type as VariantType};

const IDLE: u8 = 0;
const CREATING: u8 = 1;
const ACTIVE: u8 = 2;
const RETIRED: u8 = 3;
const SOCKET_BUFFER: usize = 4096;
fn index(kind: Characteristic) -> Option<usize> {
    match kind {
        Characteristic::AuthExchange => Some(0),
        Characteristic::Result => Some(1),
        _ => None,
    }
}
pub(super) fn supports(kind: Characteristic) -> bool {
    index(kind).is_some()
}

struct Channel {
    socket: AsyncFd<Socket>,
    live: AtomicBool,
    budget: Budget,
}
impl Channel {
    fn retire(&self) {
        self.live.store(false, Ordering::Release);
        let _ = self.socket.get_ref().shutdown(Shutdown::Both);
    }
    fn probe(&self) -> Result<(), Error> {
        if !self.live.load(Ordering::Acquire) {
            return Err(Error::Unavailable);
        }
        let mut socket = self.socket.get_ref();
        match socket.read(&mut [0; 1]) {
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(()),
            Ok(0) => {
                self.retire();
                Err(Error::Unavailable)
            }
            // Frozen flags are notify, not indicate; no reverse command/Confirm
            // byte is admitted as authority or input to the security backend.
            Ok(_) => {
                self.retire();
                Err(Error::Protocol)
            }
            Err(_) => {
                self.retire();
                Err(Error::Backend)
            }
        }
    }
}
struct Slot {
    stage: AtomicU8,
    channel: OnceLock<Arc<Channel>>,
}
impl Slot {
    fn new() -> Self {
        Self {
            stage: AtomicU8::new(IDLE),
            channel: OnceLock::new(),
        }
    }
    fn retire(&self) {
        self.stage.store(RETIRED, Ordering::Release);
        if let Some(channel) = self.channel.get() {
            channel.retire();
        }
    }
}
pub(super) struct Notifications {
    live: AtomicBool,
    slots: [Slot; 2],
    sending: Arc<Semaphore>,
}
impl Notifications {
    pub(super) fn new() -> Self {
        Self {
            live: AtomicBool::new(true),
            slots: std::array::from_fn(|_| Slot::new()),
            sending: Arc::new(Semaphore::new(1)),
        }
    }
    pub(super) fn retire(&self) {
        self.live.store(false, Ordering::Release);
        for slot in &self.slots {
            slot.retire();
        }
    }
    pub(super) fn health(&self) -> Result<(), Error> {
        if !self.live.load(Ordering::Acquire) {
            return Err(Error::StaleContext);
        }
        for slot in &self.slots {
            match slot.stage.load(Ordering::Acquire) {
                RETIRED => return Err(Error::Unavailable),
                ACTIVE => slot.channel.get().ok_or(Error::Unknown)?.probe()?,
                _ => (),
            }
        }
        Ok(())
    }
    fn acquired(&self, kind: Characteristic) -> bool {
        if !self.live.load(Ordering::Acquire) {
            return false;
        }
        let Some(i) = index(kind) else {
            return false;
        };
        let slot = &self.slots[i];
        slot.stage.load(Ordering::Acquire) == ACTIVE
            && slot
                .channel
                .get()
                .is_some_and(|channel| channel.probe().is_ok())
    }
    fn reserve(&self, kind: Characteristic) -> Result<Acquisition<'_>, Error> {
        if !self.live.load(Ordering::Acquire) {
            return Err(Error::StaleContext);
        }
        let slot = &self.slots[index(kind).ok_or(Error::Protocol)?];
        slot.stage
            .compare_exchange(IDLE, CREATING, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::Busy)?;
        let guard = Acquisition {
            parent: self,
            slot,
            complete: false,
        };
        if !self.live.load(Ordering::Acquire) {
            return Err(Error::StaleContext);
        }
        Ok(guard)
    }
    fn channel(&self, kind: Characteristic) -> Result<Arc<Channel>, Error> {
        if !self.live.load(Ordering::Acquire) {
            return Err(Error::StaleContext);
        }
        let slot = &self.slots[index(kind).ok_or(Error::Protocol)?];
        if slot.stage.load(Ordering::Acquire) != ACTIVE {
            return Err(Error::Unavailable);
        }
        let channel = slot.channel.get().ok_or(Error::Unknown)?.clone();
        channel.probe()?;
        Ok(channel)
    }
}
struct Acquisition<'a> {
    parent: &'a Notifications,
    slot: &'a Slot,
    complete: bool,
}
impl Acquisition<'_> {
    fn commit(mut self, channel: Arc<Channel>) -> Result<(), Error> {
        if !self.parent.live.load(Ordering::Acquire) {
            channel.retire();
            return Err(Error::StaleContext);
        }
        self.slot
            .channel
            .set(channel.clone())
            .map_err(|_| Error::Unknown)?;
        self.slot.stage.store(ACTIVE, Ordering::Release);
        if !self.parent.live.load(Ordering::Acquire) {
            channel.retire();
            return Err(Error::StaleContext);
        }
        self.complete = true;
        Ok(())
    }
}
impl Drop for Acquisition<'_> {
    fn drop(&mut self) {
        if !self.complete {
            self.slot.retire();
        }
    }
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
pub(super) enum AcquireError {
    Failed(String),
}
fn failed() -> AcquireError {
    AcquireError::Failed("notification channel unavailable".into())
}

pub(super) struct NativeCharacteristic {
    pub(super) base: CharacteristicObject,
}
#[zbus::interface(name = "org.bluez.GattCharacteristic1")]
impl NativeCharacteristic {
    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> &str {
        self.base.kind.uuid()
    }
    #[zbus(property)]
    fn service(&self) -> OwnedObjectPath {
        self.base.service()
    }
    #[zbus(property)]
    fn flags(&self) -> Vec<&str> {
        self.base.kind.flags().to_vec()
    }
    #[zbus(property)]
    fn notify_acquired(&self) -> bool {
        self.base.publication.live.load(Ordering::Acquire)
            && self.base.publication.notifications.acquired(self.base.kind)
    }
    #[zbus(property)]
    fn notifying(&self) -> bool {
        self.notify_acquired()
    }
    async fn read_value(
        &self,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<Vec<u8>> {
        self.base.read_value(options, header).await
    }
    async fn write_value(
        &self,
        bytes: Vec<u8>,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        self.base.write_value(bytes, options, header).await
    }
    async fn start_notify(&self, #[zbus(header)] header: Header<'_>) -> zbus::fdo::Result<()> {
        self.base.start_notify(header).await
    }
    async fn stop_notify(&self, #[zbus(header)] header: Header<'_>) -> zbus::fdo::Result<()> {
        self.base.stop_notify(header).await
    }
    async fn acquire_notify(
        &self,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<(OwnedFd, u16), AcquireError> {
        let deadline = tokio::time::Instant::now()
            .checked_add(Duration::from_millis(DBUS_CALL_MS))
            .ok_or_else(failed)?;
        tokio::time::timeout_at(
            deadline,
            acquire(
                &self.base.publication,
                self.base.kind,
                options,
                header,
                deadline,
            ),
        )
        .await
        .map_err(|_| failed())?
        .map_err(|_| failed())
    }
}
async fn acquire(
    publication: &Arc<Publication>,
    kind: Characteristic,
    mut options: HashMap<String, OwnedValue>,
    header: Header<'_>,
    deadline: tokio::time::Instant,
) -> Result<(OwnedFd, u16), Error> {
    if header.signature() != <HashMap<String, OwnedValue> as VariantType>::SIGNATURE {
        return Err(Error::Protocol);
    }
    let device = OwnedObjectPath::try_from(options.remove("device").ok_or(Error::InvalidInput)?)
        .map_err(|_| Error::InvalidInput)?;
    let mtu = u16::try_from(options.remove("mtu").ok_or(Error::InvalidInput)?)
        .map_err(|_| Error::InvalidInput)?;
    let budget = Budget::negotiated_att_mtu(mtu)?;
    if let Some(link) = options.remove("link") {
        if String::try_from(link).map_err(|_| Error::InvalidInput)? != "LE" {
            return Err(Error::Protocol);
        }
    }
    if !options.is_empty() {
        return Err(Error::InvalidInput);
    }
    let sender = header.sender().ok_or(Error::StaleContext)?.as_str();
    let peer = Peer::from_backend_callback(sender, device.as_str())?;
    if !publication.live.load(Ordering::Acquire) || !publication.matches_peer(&peer) {
        return Err(Error::StaleContext);
    }
    let reservation = publication.notifications.reserve(kind)?;
    publication.check(Some(sender)).await?;
    let (writer, reader) =
        Socket::pair(Domain::UNIX, Type::SEQPACKET, None).map_err(|_| Error::Backend)?;
    for socket in [&writer, &reader] {
        socket.set_nonblocking(true).map_err(|_| Error::Backend)?;
        socket
            .set_send_buffer_size(SOCKET_BUFFER)
            .map_err(|_| Error::Backend)?;
        socket
            .set_recv_buffer_size(SOCKET_BUFFER)
            .map_err(|_| Error::Backend)?;
    }
    let channel = Arc::new(Channel {
        socket: AsyncFd::new(writer).map_err(|_| Error::Backend)?,
        live: AtomicBool::new(true),
        budget,
    });
    publication.check(Some(sender)).await?;
    if tokio::time::Instant::now() >= deadline {
        return Err(Error::Deadline);
    }
    reservation.commit(channel)?;
    Ok((std::os::fd::OwnedFd::from(reader).into(), mtu.min(247)))
}

struct SendGuard {
    channel: Arc<Channel>,
    complete: bool,
}
impl Drop for SendGuard {
    fn drop(&mut self) {
        if !self.complete {
            self.channel.retire();
        }
    }
}
pub(super) fn notify(
    publication: Arc<Publication>,
    epoch: Epoch,
    peer: Peer,
    kind: Characteristic,
    frame: OpaqueBytes,
) -> Pending<()> {
    Box::pin(async move {
        let expected = match kind {
            Characteristic::AuthExchange => 1,
            Characteristic::Result => 3,
            _ => return Err(Error::Protocol),
        };
        let bytes = frame.as_slice();
        if epoch != publication.epoch || !publication.matches_peer(&peer) {
            return Err(Error::StaleContext);
        }
        if bytes.len() <= HEADER_BYTES
            || bytes.len() > 244
            || bytes[..4] != [b'R', b'B', 1, expected]
        {
            return Err(Error::Protocol);
        }
        let _permit = publication
            .notifications
            .sending
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::Busy)?;
        let channel = publication.notifications.channel(kind)?;
        if bytes.len() > channel.budget.attribute_bytes() {
            return Err(Error::Protocol);
        }
        let mut guard = SendGuard {
            channel: channel.clone(),
            complete: false,
        };
        let deadline = tokio::time::Instant::now()
            .checked_add(Duration::from_millis(DBUS_CALL_MS))
            .ok_or(Error::Deadline)?;
        let sending = async {
            publication.check(Some(peer.owner())).await?;
            loop {
                let mut ready = channel
                    .socket
                    .writable()
                    .await
                    .map_err(|_| Error::Backend)?;
                if tokio::time::Instant::now() >= deadline {
                    return Err(Error::Deadline);
                }
                if !publication.live.load(Ordering::Acquire) || !publication.matches_peer(&peer) {
                    return Err(Error::StaleContext);
                }
                channel.probe()?;
                match ready
                    .try_io(|socket| socket.get_ref().send_with_flags(bytes, libc::MSG_NOSIGNAL))
                {
                    Err(_) => continue, // EAGAIN before submission, not a packet resend.
                    Ok(Err(_)) => return Err(Error::Backend),
                    Ok(Ok(count)) if count != bytes.len() => return Err(Error::Protocol),
                    Ok(Ok(_)) => break,
                }
            }
            publication.check(Some(peer.owner())).await?;
            channel.probe()?;
            if tokio::time::Instant::now() >= deadline {
                return Err(Error::Deadline);
            }
            Ok(())
        };
        let result = tokio::time::timeout_at(deadline, sending)
            .await
            .unwrap_or(Err(Error::Deadline));
        if result.is_ok() {
            guard.complete = true;
        }
        result
    })
}
