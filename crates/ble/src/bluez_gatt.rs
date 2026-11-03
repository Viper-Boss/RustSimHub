// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed local GATT tree and bounded opaque callbacks. The injected host must
//! supply the actual Transport/security lifecycle. No notification success stub.
use crate::{
    bluez_control::{BluezControl, CallbackScope},
    bluez_peer::{PeerLease, PeerScope},
    framing::{Budget, OpaqueBytes, HEADER_BYTES},
    manifest::{self, Characteristic},
    ports::{Epoch, Peer, DBUS_CALL_MS},
    Error,
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot, OwnedSemaphorePermit, Semaphore};
use zbus::{
    message::Header,
    zvariant::{OwnedObjectPath, OwnedValue, Type},
    Connection,
};
#[cfg(feature = "bluez-notify-fd")]
mod notify_fd;

pub enum CallbackOperation {
    /// Opt-in owning host must bind a real Device1 lease before acknowledging.
    BindPeer,
    Write {
        frame: OpaqueBytes,
        budget: Budget,
    },
    ReadResult {
        budget: Budget,
    },
}
pub enum CallbackReply {
    Accepted,
    Value(OpaqueBytes),
}
/// Opaque trusted-adapter input, not authorization. Retain while any work based
/// on it is in flight, including after the D-Bus caller times out or disappears.
#[must_use = "retain through actual work; finish or drop when it truly ends"]
pub struct CallbackRequest {
    publication: Arc<Publication>,
    peer: Peer,
    characteristic: Characteristic,
    operation: CallbackOperation,
    reply: oneshot::Sender<Completion>,
    permit: OwnedSemaphorePermit,
    work_live: Arc<AtomicBool>,
    deadline: tokio::time::Instant,
}
impl CallbackRequest {
    #[cfg(feature = "service")]
    pub(crate) fn deadline(&self) -> tokio::time::Instant {
        self.deadline
    }
    pub fn epoch(&self) -> Epoch {
        self.publication.epoch
    }
    pub fn peer(&self) -> &Peer {
        &self.peer
    }
    pub fn characteristic(&self) -> Characteristic {
        self.characteristic
    }
    pub fn operation(&self) -> &CallbackOperation {
        &self.operation
    }
    /// Host pre/post-await fence. Core must still use its own private capability
    /// and connection fence; this check cannot authorize a business operation.
    pub async fn check_current(&self) -> Result<(), Error> {
        if !self.work_live.load(Ordering::Acquire) || tokio::time::Instant::now() >= self.deadline {
            return Err(Error::StaleContext);
        }
        if matches!(self.operation, CallbackOperation::BindPeer) {
            self.publication
                .check_owner(Some(self.peer.owner()))
                .await?;
        } else {
            self.publication.check(Some(self.peer.owner())).await?;
        }
        if !self.work_live.load(Ordering::Acquire) || tokio::time::Instant::now() >= self.deadline {
            return Err(Error::StaleContext);
        }
        Ok(())
    }
    pub fn finish(self, result: Result<CallbackReply, Error>) {
        let Self { reply, permit, .. } = self;
        // Completed output keeps ownership until the callback consumes/discards it.
        let _ = reply.send(Completion {
            result,
            _permit: permit,
        });
    }
}
struct Completion {
    result: Result<CallbackReply, Error>,
    _permit: OwnedSemaphorePermit,
}
struct RequestFence(Arc<AtomicBool>);
impl Drop for RequestFence {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
struct Publication {
    host_binding: AtomicBool,
    live: AtomicBool,
    scope: CallbackScope,
    epoch: Epoch,
    requests: mpsc::Sender<CallbackRequest>,
    slot: Arc<Semaphore>,
    peer_scope: OnceLock<PeerScope>,
    #[cfg(feature = "bluez-notify-fd")]
    notifications: notify_fd::Notifications,
}
impl Publication {
    async fn check_owner(&self, sender: Option<&str>) -> Result<String, Error> {
        if !self.live.load(Ordering::Acquire) {
            return Err(Error::StaleContext);
        }
        let owner = self.scope.check_sender(sender).await?;
        if !self.live.load(Ordering::Acquire) {
            return Err(Error::StaleContext);
        }
        Ok(owner.to_string())
    }
    async fn ensure_peer(self: &Arc<Self>, peer: &Peer, kind: Characteristic) -> Result<(), Error> {
        if self.peer_scope.get().is_none() {
            if !self.host_binding.load(Ordering::Acquire) {
                return Err(Error::Unavailable);
            }
            let completion = self
                .dispatch(peer.clone(), kind, CallbackOperation::BindPeer)
                .await?;
            if !matches!(completion.result, Ok(CallbackReply::Accepted)) {
                return Err(Error::Unavailable);
            }
            // Release the one-slot completion before enqueueing the actual RPC.
        }
        if !self.matches_peer(peer) {
            return Err(Error::StaleContext);
        }
        self.check(Some(peer.owner())).await?;
        Ok(())
    }
    async fn check(&self, sender: Option<&str>) -> Result<String, Error> {
        if !self.live.load(Ordering::Acquire) {
            return Err(Error::StaleContext);
        }
        let owner = self.scope.check_sender(sender).await?;
        self.peer_scope
            .get()
            .ok_or(Error::Unavailable)?
            .check_current()
            .await?;
        #[cfg(feature = "bluez-notify-fd")]
        self.notifications.health()?;
        if !self.live.load(Ordering::Acquire) {
            return Err(Error::StaleContext);
        }
        Ok(owner.to_string())
    }
    fn retire(&self) {
        self.live.store(false, Ordering::Release);
        #[cfg(feature = "bluez-notify-fd")]
        self.notifications.retire();
    }
    fn matches_peer(&self, peer: &Peer) -> bool {
        self.peer_scope
            .get()
            .is_some_and(|scope| scope.matches_peer(peer))
    }
    async fn dispatch(
        self: &Arc<Self>,
        peer: Peer,
        characteristic: Characteristic,
        operation: CallbackOperation,
    ) -> Result<Completion, Error> {
        if !self.matches_peer(&peer)
            && !(matches!(&operation, CallbackOperation::BindPeer)
                && self.host_binding.load(Ordering::Acquire)
                && self.peer_scope.get().is_none())
        {
            return Err(Error::StaleContext);
        }
        let permit = self
            .slot
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::Busy)?;
        let (reply, receiver) = oneshot::channel();
        let work_live = Arc::new(AtomicBool::new(true));
        let _fence = RequestFence(work_live.clone());
        let deadline = tokio::time::Instant::now()
            .checked_add(Duration::from_millis(DBUS_CALL_MS))
            .ok_or(Error::Deadline)?;
        self.requests
            .try_send(CallbackRequest {
                publication: self.clone(),
                peer: peer.clone(),
                characteristic,
                operation,
                reply,
                permit,
                work_live,
                deadline,
            })
            .map_err(|_| Error::Unavailable)?;
        let completion = tokio::time::timeout_at(deadline, receiver)
            .await
            .map_err(|_| Error::Deadline)?
            .map_err(|_| Error::Unavailable)?;
        tokio::time::timeout_at(deadline, self.check(Some(peer.owner())))
            .await
            .map_err(|_| Error::Deadline)??;
        if tokio::time::Instant::now() >= deadline {
            return Err(Error::Deadline);
        }
        Ok(completion)
    }
}
fn denied() -> zbus::fdo::Error {
    zbus::fdo::Error::AccessDenied("invalid callback context".into())
}
fn invalid() -> zbus::fdo::Error {
    zbus::fdo::Error::InvalidArgs("invalid fixed GATT request".into())
}
fn unavailable() -> zbus::fdo::Error {
    zbus::fdo::Error::Failed("callback unavailable".into())
}

struct Service;
#[zbus::interface(name = "org.bluez.GattService1")]
impl Service {
    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> &str {
        manifest::SERVICE_UUID
    }
    #[zbus(property)]
    fn primary(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn includes(&self) -> Vec<OwnedObjectPath> {
        Vec::new()
    }
}
struct CharacteristicObject {
    publication: Arc<Publication>,
    kind: Characteristic,
}
#[zbus::interface(name = "org.bluez.GattCharacteristic1")]
impl CharacteristicObject {
    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> &str {
        self.kind.uuid()
    }
    #[zbus(property)]
    fn service(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(manifest::SERVICE_PATH).expect("fixed manifest")
    }
    #[zbus(property)]
    fn flags(&self) -> Vec<&str> {
        self.kind.flags().to_vec()
    }
    async fn read_value(
        &self,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<Vec<u8>> {
        tokio::time::timeout(
            Duration::from_millis(DBUS_CALL_MS),
            self.read_value_inner(options, header),
        )
        .await
        .unwrap_or_else(|_| Err(unavailable()))
    }
    async fn write_value(
        &self,
        bytes: Vec<u8>,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        tokio::time::timeout(
            Duration::from_millis(DBUS_CALL_MS),
            self.write_value_inner(bytes, options, header),
        )
        .await
        .unwrap_or_else(|_| Err(unavailable()))
    }
    async fn start_notify(&self, #[zbus(header)] header: Header<'_>) -> zbus::fdo::Result<()> {
        tokio::time::timeout(
            Duration::from_millis(DBUS_CALL_MS),
            self.unsupported_notify(header),
        )
        .await
        .unwrap_or_else(|_| Err(unavailable()))
    }
    async fn stop_notify(&self, #[zbus(header)] header: Header<'_>) -> zbus::fdo::Result<()> {
        tokio::time::timeout(
            Duration::from_millis(DBUS_CALL_MS),
            self.unsupported_notify(header),
        )
        .await
        .unwrap_or_else(|_| Err(unavailable()))
    }
}
impl CharacteristicObject {
    async fn read_value_inner(
        &self,
        options: HashMap<String, OwnedValue>,
        header: Header<'_>,
    ) -> zbus::fdo::Result<Vec<u8>> {
        if header.signature().to_string() != "a{sv}" {
            return Err(invalid());
        }
        let owner = self
            .publication
            .check_owner(header.sender().map(|s| s.as_str()))
            .await
            .map_err(|_| denied())?;
        if !matches!(
            self.kind,
            Characteristic::DeviceInfo | Characteristic::Result
        ) {
            return Err(denied());
        }
        let options = Options::parse(options, false).map_err(|_| invalid())?;
        let peer =
            Peer::from_backend_callback(&owner, options.device.as_str()).map_err(|_| invalid())?;
        self.publication
            .ensure_peer(&peer, self.kind)
            .await
            .map_err(|_| denied())?;
        if !self.publication.matches_peer(&peer) {
            return Err(denied());
        }
        if self.kind == Characteristic::DeviceInfo {
            let bytes = manifest::DEVICE_INFO
                .get(usize::from(options.offset)..)
                .ok_or_else(invalid)?;
            self.publication
                .check(Some(&owner))
                .await
                .map_err(|_| denied())?;
            return Ok(bytes.to_vec());
        }
        if options.offset != 0 {
            return Err(invalid());
        }
        let completion = self
            .publication
            .dispatch(
                peer,
                self.kind,
                CallbackOperation::ReadResult {
                    budget: options.budget,
                },
            )
            .await
            .map_err(|_| unavailable())?;
        match completion.result {
            Ok(CallbackReply::Value(bytes))
                if bytes.as_slice().len() <= options.budget.attribute_bytes() =>
            {
                Ok(bytes.into_vec())
            }
            _ => Err(unavailable()),
        }
    }
    async fn write_value_inner(
        &self,
        bytes: Vec<u8>,
        options: HashMap<String, OwnedValue>,
        header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        if header.signature() != <(Vec<u8>, HashMap<String, OwnedValue>) as Type>::SIGNATURE {
            return Err(invalid());
        }
        let owner = self
            .publication
            .check_owner(header.sender().map(|s| s.as_str()))
            .await
            .map_err(|_| denied())?;
        if !matches!(
            self.kind,
            Characteristic::AuthExchange | Characteristic::Command
        ) {
            return Err(denied());
        }
        let options = Options::parse(options, true).map_err(|_| invalid())?;
        if options.offset != 0
            || bytes.len() <= HEADER_BYTES
            || bytes.len() > options.budget.attribute_bytes()
        {
            return Err(invalid());
        }
        let peer =
            Peer::from_backend_callback(&owner, options.device.as_str()).map_err(|_| invalid())?;
        self.publication
            .ensure_peer(&peer, self.kind)
            .await
            .map_err(|_| denied())?;
        if !self.publication.matches_peer(&peer) {
            return Err(denied());
        }
        let operation = CallbackOperation::Write {
            frame: OpaqueBytes::new(bytes).map_err(|_| invalid())?,
            budget: options.budget,
        };
        let completion = self
            .publication
            .dispatch(peer, self.kind, operation)
            .await
            .map_err(|_| unavailable())?;
        match completion.result {
            Ok(CallbackReply::Accepted) => Ok(()),
            _ => Err(unavailable()),
        }
    }
    async fn unsupported_notify(&self, header: Header<'_>) -> zbus::fdo::Result<()> {
        if header.signature().to_string() != "" {
            return Err(invalid());
        }
        self.publication
            .check(header.sender().map(|s| s.as_str()))
            .await
            .map_err(|_| denied())?;
        Err(zbus::fdo::Error::NotSupported(
            "notification integration unavailable".into(),
        ))
    }
}
struct Options {
    device: OwnedObjectPath,
    offset: u16,
    budget: Budget,
}
impl Options {
    fn parse(mut values: HashMap<String, OwnedValue>, write: bool) -> Result<Self, Error> {
        let device = OwnedObjectPath::try_from(values.remove("device").ok_or(Error::InvalidInput)?)
            .map_err(|_| Error::InvalidInput)?;
        let suffix = device
            .as_str()
            .strip_prefix("/org/bluez/hci0/dev_")
            .ok_or(Error::InvalidInput)?;
        let parts: Vec<_> = suffix.split('_').collect();
        if parts.len() != 6
            || parts
                .iter()
                .any(|s| s.len() != 2 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(Error::InvalidInput);
        }
        let offset = values
            .remove("offset")
            .map(u16::try_from)
            .transpose()
            .map_err(|_| Error::InvalidInput)?
            .unwrap_or(0);
        let budget = values
            .remove("mtu")
            .map(u16::try_from)
            .transpose()
            .map_err(|_| Error::InvalidInput)?
            .map(Budget::negotiated_att_mtu)
            .transpose()?
            .unwrap_or(Budget::INITIAL);
        if let Some(link) = values.remove("link") {
            if String::try_from(link).map_err(|_| Error::InvalidInput)? != "LE" {
                return Err(Error::InvalidInput);
            }
        }
        if write {
            let kind = String::try_from(values.remove("type").ok_or(Error::InvalidInput)?)
                .map_err(|_| Error::InvalidInput)?;
            if kind != "request" {
                return Err(Error::Protocol);
            }
            if let Some(prepare) = values.remove("prepare-authorize") {
                if bool::try_from(prepare).map_err(|_| Error::InvalidInput)? {
                    return Err(Error::Protocol);
                }
            }
        }
        if !values.is_empty() {
            return Err(Error::InvalidInput);
        }
        Ok(Self {
            device,
            offset,
            budget,
        })
    }
}

/// Partial GATT edge, without service activation. Explicit bluez-notify-fd adds
/// per-peer local FD submission; it does not prove remote or business receipt.
/// Drop fences callbacks but keeps fixed object tombstones until connection close.
#[must_use = "retain until remote cleanup and all opaque callback work ends"]
pub struct GattExport {
    connection: Connection,
    publication: Arc<Publication>,
    lease: OnceLock<PeerLease>,
}
impl GattExport {
    pub(crate) async fn publish_with_peer_requests(
        control: &BluezControl,
        epoch: Epoch,
    ) -> Result<(Self, mpsc::Receiver<CallbackRequest>), Error> {
        let (export, receiver) = Self::publish(control, epoch).await?;
        export
            .publication
            .host_binding
            .store(true, Ordering::Release);
        Ok((export, receiver))
    }
    pub async fn publish(
        control: &BluezControl,
        epoch: Epoch,
    ) -> Result<(Self, mpsc::Receiver<CallbackRequest>), Error> {
        control.check_current().await?;
        let (requests, receiver) = mpsc::channel(1);
        let publication = Arc::new(Publication {
            host_binding: AtomicBool::new(false),
            live: AtomicBool::new(true),
            scope: control.callback_scope(),
            epoch,
            requests,
            slot: Arc::new(Semaphore::new(1)),
            peer_scope: OnceLock::new(),
            #[cfg(feature = "bluez-notify-fd")]
            notifications: notify_fd::Notifications::new(),
        });
        let mut guard = PublishGuard {
            publication: publication.clone(),
            armed: true,
        };
        let connection = control.connection();
        if !connection
            .object_server()
            .at(manifest::ROOT, zbus::fdo::ObjectManager)
            .await
            .map_err(|_| Error::Backend)?
        {
            return Err(Error::Busy);
        }
        if !connection
            .object_server()
            .at(manifest::SERVICE_PATH, Service)
            .await
            .map_err(|_| Error::Backend)?
        {
            return Err(Error::Busy);
        }
        for kind in Characteristic::ALL {
            let object = CharacteristicObject {
                publication: publication.clone(),
                kind,
            };
            #[cfg(feature = "bluez-notify-fd")]
            let inserted = if notify_fd::supports(kind) {
                connection
                    .object_server()
                    .at(
                        kind.path(),
                        notify_fd::NativeCharacteristic { base: object },
                    )
                    .await
            } else {
                connection.object_server().at(kind.path(), object).await
            };
            #[cfg(not(feature = "bluez-notify-fd"))]
            let inserted = connection.object_server().at(kind.path(), object).await;
            if !inserted.map_err(|_| Error::Backend)? {
                return Err(Error::Busy);
            }
        }
        control.check_current().await?;
        guard.armed = false;
        Ok((
            Self {
                connection,
                publication,
                lease: OnceLock::new(),
            },
            receiver,
        ))
    }
    /// Convenience for an already connected peer. Normal startup exports static
    /// metadata first, then binds once after the real Device1 connection event.
    pub async fn publish_bound(
        control: &BluezControl,
        lease: PeerLease,
    ) -> Result<(Self, mpsc::Receiver<CallbackRequest>), Error> {
        if !lease.matches_control(control) {
            return Err(Error::StaleContext);
        }
        lease.check_current().await?;
        let (export, receiver) = Self::publish(control, lease.epoch()).await?;
        export.bind_peer(lease).await?;
        Ok((export, receiver))
    }
    /// At most one successful peer binding per exported tree. Before this
    /// commits every method callback fails; readonly service metadata is public.
    pub async fn bind_peer(&self, lease: PeerLease) -> Result<(), Error> {
        if self.lease.get().is_some() {
            return Err(Error::Busy);
        }
        if !self.publication.live.load(Ordering::Acquire)
            || lease.epoch() != self.publication.epoch
            || !lease.matches_scope(&self.publication.scope)
        {
            return Err(Error::StaleContext);
        }
        lease.check_current().await?;
        if !self.publication.live.load(Ordering::Acquire) {
            return Err(Error::StaleContext);
        }
        let scope = lease.scope();
        self.lease.set(lease).map_err(|_| Error::Busy)?;
        if self.publication.peer_scope.set(scope).is_err() {
            self.retire_local();
            return Err(Error::Unknown);
        }
        if !self.publication.live.load(Ordering::Acquire) {
            self.retire_local();
            return Err(Error::StaleContext);
        }
        Ok(())
    }
    pub fn retire_local(&self) {
        self.publication.retire();
        if let Some(lease) = self.lease.get() {
            lease.retire();
        }
    }
    /// Host idle/pre/post-work check. On error the host must drive original
    /// lifecycle/security cleanup; this local fence cannot do that for it.
    pub async fn check_peer_current(&self) -> Result<(), Error> {
        if !self.publication.live.load(Ordering::Acquire) {
            return Err(Error::StaleContext);
        }
        let result = self
            .lease
            .get()
            .ok_or(Error::Unavailable)?
            .check_current()
            .await;
        #[cfg(feature = "bluez-notify-fd")]
        let result = result.and_then(|()| self.publication.notifications.health());
        if result.is_err() {
            self.publication.retire();
        }
        result
    }
    pub(crate) fn wait_peer_lost(&self) -> crate::ports::Pending<()> {
        match self.lease.get() {
            Some(peer) => peer.wait_lost(),
            None => Box::pin(async { Err(Error::Unavailable) }),
        }
    }
    /// Local per-peer FD submission only; never ATT/browser/business receipt.
    /// Host must retain the owned future through actual completion/cleanup.
    #[cfg(feature = "bluez-notify-fd")]
    pub fn notify(
        &self,
        epoch: Epoch,
        peer: Peer,
        kind: Characteristic,
        frame: OpaqueBytes,
    ) -> crate::ports::Pending<()> {
        notify_fd::notify(self.publication.clone(), epoch, peer, kind, frame)
    }
    pub async fn close_connection(&self) -> Result<(), Error> {
        self.retire_local();
        self.connection
            .clone()
            .close()
            .await
            .map_err(|_| Error::Backend)
    }
}
impl Drop for GattExport {
    fn drop(&mut self) {
        self.retire_local();
    }
}
struct PublishGuard {
    publication: Arc<Publication>,
    armed: bool,
}
impl Drop for PublishGuard {
    fn drop(&mut self) {
        if self.armed {
            self.publication.retire();
        }
    }
}

#[cfg(test)]
mod tests;
