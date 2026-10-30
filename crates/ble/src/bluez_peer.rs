// SPDX-License-Identifier: AGPL-3.0-only
//! One observed Device1 connection. No pairing, authorization or reconnection.
use crate::{
    bluez_control::{BluezControl, CallbackScope},
    ports::{Epoch, Peer, DBUS_CALL_MS},
    Error,
};
use futures_lite::StreamExt;
#[cfg(feature = "bluez-gatt")]
use std::sync::Weak;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::Mutex;
use zbus::{
    message::Message,
    zvariant::{OwnedObjectPath, OwnedValue, Type},
    Connection, MessageStream,
};

const DEVICE_INTERFACE: &str = "org.bluez.Device1";
struct Changes {
    properties: MessageStream,
    removed: MessageStream,
}
/// Pins a single observed connected device/adapter/owner and its change history.
/// check_current must be polled by the host pre/post work and while idle. There
/// is no background listener; this is not a Core capability or secure session.
#[must_use = "retain and check for one peer lifetime; never reuse after disconnect"]
pub struct PeerLease {
    binding: Arc<LeaseBinding>,
}
struct LeaseBinding {
    connection: Connection,
    scope: CallbackScope,
    peer: Peer,
    epoch: Epoch,
    live: AtomicBool,
    changes: Mutex<Changes>,
}
impl PeerLease {
    pub async fn bind(control: &BluezControl, epoch: Epoch, device: &str) -> Result<Self, Error> {
        tokio::time::timeout(
            std::time::Duration::from_millis(DBUS_CALL_MS),
            Self::bind_inner(control, epoch, device),
        )
        .await
        .unwrap_or(Err(Error::Deadline))
    }
    async fn bind_inner(control: &BluezControl, epoch: Epoch, device: &str) -> Result<Self, Error> {
        validate_device(device)?;
        control.check_current().await?;
        let peer = Peer::from_backend_callback(control.owner(), device)?;
        let connection = control.connection();
        // Both subscriptions precede the snapshot, including a false->true gap
        // on the same object path which two Connected snapshots cannot detect.
        let properties = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(peer.owner())
            .map_err(|_| Error::InvalidInput)?
            .path(device)
            .map_err(|_| Error::InvalidInput)?
            .interface("org.freedesktop.DBus.Properties")
            .map_err(|_| Error::InvalidInput)?
            .member("PropertiesChanged")
            .map_err(|_| Error::InvalidInput)?
            .arg(0, DEVICE_INTERFACE)
            .map_err(|_| Error::InvalidInput)?
            .build();
        let removed = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(peer.owner())
            .map_err(|_| Error::InvalidInput)?
            .path("/")
            .map_err(|_| Error::InvalidInput)?
            .interface("org.freedesktop.DBus.ObjectManager")
            .map_err(|_| Error::InvalidInput)?
            .member("InterfacesRemoved")
            .map_err(|_| Error::InvalidInput)?
            .build();
        let properties = MessageStream::for_match_rule(properties, &connection, Some(32))
            .await
            .map_err(|_| Error::Backend)?;
        let removed = MessageStream::for_match_rule(removed, &connection, Some(32))
            .await
            .map_err(|_| Error::Backend)?;
        let lease = Self {
            binding: Arc::new(LeaseBinding {
                connection,
                scope: control.callback_scope(),
                peer,
                epoch,
                live: AtomicBool::new(true),
                changes: Mutex::new(Changes {
                    properties,
                    removed,
                }),
            }),
        };
        lease.check_current().await?;
        Ok(lease)
    }
    pub fn peer(&self) -> &Peer {
        &self.binding.peer
    }
    pub fn epoch(&self) -> Epoch {
        self.binding.epoch
    }
    pub fn retire(&self) {
        self.binding.retire();
    }
    pub async fn check_current(&self) -> Result<(), Error> {
        self.binding.check_current().await
    }
    #[cfg(feature = "bluez-gatt")]
    pub(crate) fn matches_control(&self, control: &BluezControl) -> bool {
        self.matches_scope(&control.callback_scope())
    }
    #[cfg(feature = "bluez-gatt")]
    pub(crate) fn matches_scope(&self, scope: &CallbackScope) -> bool {
        self.binding.scope.same_binding(scope)
    }
    #[cfg(feature = "bluez-gatt")]
    pub(crate) fn scope(&self) -> PeerScope {
        PeerScope {
            binding: Arc::downgrade(&self.binding),
        }
    }
}
impl LeaseBinding {
    fn retire(&self) {
        self.live.store(false, Ordering::Release);
    }
    async fn check_current(&self) -> Result<(), Error> {
        let mut guard = CheckGuard {
            lease: self,
            complete: false,
        };
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(DBUS_CALL_MS),
            self.check_inner(),
        )
        .await
        .unwrap_or(Err(Error::Deadline));
        if result.is_err() {
            self.retire();
        }
        guard.complete = true;
        result
    }
    async fn check_inner(&self) -> Result<(), Error> {
        self.check_live()?;
        self.scope.check_sender(Some(self.peer.owner())).await?;
        self.drain().await?;
        let proxy = zbus::Proxy::new(
            &self.connection,
            self.peer.owner(),
            self.peer.device_path(),
            "org.freedesktop.DBus.Properties",
        )
        .await
        .map_err(|_| Error::Backend)?;
        let mut values: HashMap<String, OwnedValue> = proxy
            .call("GetAll", &(DEVICE_INTERFACE,))
            .await
            .map_err(|_| Error::Backend)?;
        let connected = bool::try_from(values.remove("Connected").ok_or(Error::Protocol)?)
            .map_err(|_| Error::Protocol)?;
        let adapter = OwnedObjectPath::try_from(values.remove("Adapter").ok_or(Error::Protocol)?)
            .map_err(|_| Error::Protocol)?;
        if !connected || adapter.as_str() != "/org/bluez/hci0" {
            return Err(Error::StaleContext);
        }
        self.drain().await?;
        self.scope.check_sender(Some(self.peer.owner())).await?;
        self.check_live()
    }
    fn check_live(&self) -> Result<(), Error> {
        if self.live.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err(Error::StaleContext)
        }
    }
    async fn drain(&self) -> Result<(), Error> {
        let mut changes = self.changes.lock().await;
        self.check_live()?;
        // Bound work even if the trusted service floods unrelated property data.
        for _ in 0..64 {
            let Changes {
                properties,
                removed,
            } = &mut *changes;
            let next = futures_lite::future::poll_once(futures_lite::future::or(
                async { (true, properties.next().await) },
                async { (false, removed.next().await) },
            ))
            .await;
            let Some((properties, message)) = next else {
                return self.check_live();
            };
            let message = message
                .ok_or(Error::Unavailable)?
                .map_err(|_| Error::Backend)?;
            if self.invalidates(properties, message)? {
                return Err(Error::StaleContext);
            }
        }
        Err(Error::Exhausted)
    }
    fn invalidates(&self, properties: bool, message: Message) -> Result<bool, Error> {
        if properties {
            type Body = (String, HashMap<String, OwnedValue>, Vec<String>);
            if message.header().signature() != Body::SIGNATURE {
                return Err(Error::Protocol);
            }
            let (interface, changed, invalidated): Body =
                message.body().deserialize().map_err(|_| Error::Protocol)?;
            if interface != DEVICE_INTERFACE {
                return Err(Error::Protocol);
            }
            Ok(["Connected", "Adapter"]
                .iter()
                .any(|name| changed.contains_key(*name) || invalidated.iter().any(|s| s == name)))
        } else {
            type Body = (OwnedObjectPath, Vec<String>);
            if message.header().signature() != Body::SIGNATURE {
                return Err(Error::Protocol);
            }
            let (path, interfaces): Body =
                message.body().deserialize().map_err(|_| Error::Protocol)?;
            Ok(path.as_str() == self.peer.device_path()
                && interfaces.iter().any(|s| s == DEVICE_INTERFACE))
        }
    }
}
impl Drop for PeerLease {
    fn drop(&mut self) {
        self.retire();
    }
}
struct CheckGuard<'a> {
    lease: &'a LeaseBinding,
    complete: bool,
}
#[cfg(feature = "bluez-gatt")]
pub(crate) struct PeerScope {
    binding: Weak<LeaseBinding>,
}
#[cfg(feature = "bluez-gatt")]
impl PeerScope {
    pub(crate) async fn check_current(&self) -> Result<(), Error> {
        self.binding
            .upgrade()
            .ok_or(Error::Unavailable)?
            .check_current()
            .await
    }
    pub(crate) fn matches_peer(&self, peer: &Peer) -> bool {
        self.binding
            .upgrade()
            .is_some_and(|bound| &bound.peer == peer && bound.live.load(Ordering::Acquire))
    }
}
impl Drop for CheckGuard<'_> {
    fn drop(&mut self) {
        if !self.complete {
            self.lease.retire();
        }
    }
}
fn validate_device(device: &str) -> Result<(), Error> {
    let suffix = device
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
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
