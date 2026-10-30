// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed BlueZ control RPCs on a caller-supplied connection. No bus connection,
//! local GATT object export, notification, cryptography, or service is started here.
use crate::{manifest, ports::Pending, Error};
use futures_lite::StreamExt;
#[cfg(feature = "bluez-advertisement")]
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
    names::OwnedUniqueName,
    zvariant::{ObjectPath, OwnedValue},
    Connection,
};

const BLUEZ: &str = "org.bluez";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Adapter {
    Hci0,
}
impl Adapter {
    fn path(self) -> &'static str {
        match self {
            Self::Hci0 => "/org/bluez/hci0",
        }
    }
}

/// Bound to one captured unique owner, never automatically follows org.bluez.
/// This is the four-RPC control portion, not a complete GattBackendV1 or proof
/// that the caller has exported the required fixed local object tree.
pub struct BluezControl {
    binding: Arc<Binding>,
}
struct Binding {
    connection: Connection,
    owner: OwnedUniqueName,
    adapter: Adapter,
    watch: Arc<OwnerWatch>,
}
struct OwnerWatch {
    live: AtomicBool,
    changes: Mutex<zbus::MessageStream>,
}
impl BluezControl {
    pub async fn bind(
        connection: Connection,
        owner: &str,
        adapter: Adapter,
    ) -> Result<Self, Error> {
        let owner = OwnedUniqueName::try_from(owner.to_owned()).map_err(|_| Error::InvalidInput)?;
        // Subscribe BEFORE the initial snapshot. A release/reacquire by the same
        // unique connection is still a new context, even if two snapshots agree.
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.DBus")
            .map_err(|_| Error::InvalidInput)?
            .path("/org/freedesktop/DBus")
            .map_err(|_| Error::InvalidInput)?
            .interface("org.freedesktop.DBus")
            .map_err(|_| Error::InvalidInput)?
            .member("NameOwnerChanged")
            .map_err(|_| Error::InvalidInput)?
            .arg(0, BLUEZ)
            .map_err(|_| Error::InvalidInput)?
            .build();
        let changes = zbus::MessageStream::for_match_rule(rule, &connection, Some(32))
            .await
            .map_err(|_| Error::Backend)?;
        let binding = Arc::new(Binding {
            connection,
            owner,
            adapter,
            watch: Arc::new(OwnerWatch {
                live: AtomicBool::new(true),
                changes: Mutex::new(changes),
            }),
        });
        binding.check_owner().await?;
        Ok(Self { binding })
    }
    /// Local monotonic fence, not a claim that remote cleanup finished. Ordinary
    /// cleanup must precede retirement; a replacement owner must never be cleaned.
    pub fn retire(&self) {
        self.binding.retire();
    }
    pub fn register_application(&mut self) -> Pending<()> {
        self.operation(Operation::RegisterApplication)
    }
    pub fn unregister_application(&mut self) -> Pending<()> {
        self.operation(Operation::UnregisterApplication)
    }
    pub fn register_advertisement(&mut self) -> Pending<()> {
        self.operation(Operation::RegisterAdvertisement)
    }
    pub fn unregister_advertisement(&mut self) -> Pending<()> {
        self.operation(Operation::UnregisterAdvertisement)
    }
    fn operation(&self, operation: Operation) -> Pending<()> {
        let bound = self.binding.clone();
        Box::pin(async move {
            bound.check_owner().await?;
            let (interface, method, path, register) = match operation {
                Operation::RegisterApplication => (
                    "org.bluez.GattManager1",
                    "RegisterApplication",
                    manifest::ROOT,
                    true,
                ),
                Operation::UnregisterApplication => (
                    "org.bluez.GattManager1",
                    "UnregisterApplication",
                    manifest::ROOT,
                    false,
                ),
                Operation::RegisterAdvertisement => (
                    "org.bluez.LEAdvertisingManager1",
                    "RegisterAdvertisement",
                    manifest::ADVERTISEMENT_PATH,
                    true,
                ),
                Operation::UnregisterAdvertisement => (
                    "org.bluez.LEAdvertisingManager1",
                    "UnregisterAdvertisement",
                    manifest::ADVERTISEMENT_PATH,
                    false,
                ),
            };
            let proxy = zbus::Proxy::new(
                &bound.connection,
                bound.owner.as_str(),
                bound.adapter.path(),
                interface,
            )
            .await
            .map_err(|_| Error::Backend)?;
            let path = ObjectPath::try_from(path).map_err(|_| Error::InvalidInput)?;
            let result = if register {
                proxy
                    .call::<_, _, ()>(method, &(path, HashMap::<String, OwnedValue>::new()))
                    .await
            } else {
                proxy.call::<_, _, ()>(method, &(path,)).await
            };
            // Recheck even when the RPC errored. Its remote side effects may already
            // have happened; never retry or clean a replacement owner's objects.
            bound.check_owner().await?;
            result.map_err(|_| Error::Backend)
        })
    }
    #[cfg(feature = "bluez-advertisement")]
    pub(crate) fn connection(&self) -> Connection {
        self.binding.connection.clone()
    }
    #[cfg(feature = "bluez-advertisement")]
    pub(crate) fn callback_scope(&self) -> CallbackScope {
        CallbackScope {
            binding: Arc::downgrade(&self.binding),
        }
    }
    #[cfg(feature = "bluez-advertisement")]
    pub(crate) async fn check_current(&self) -> Result<(), Error> {
        self.binding.check_owner().await
    }
}

#[derive(Clone)]
#[cfg(feature = "bluez-advertisement")]
pub(crate) struct CallbackScope {
    binding: Weak<Binding>,
}
#[cfg(feature = "bluez-advertisement")]
impl CallbackScope {
    pub(crate) async fn check_sender(
        &self,
        sender: Option<&str>,
    ) -> Result<OwnedUniqueName, Error> {
        let bound = self.binding.upgrade().ok_or(Error::Unavailable)?;
        if sender != Some(bound.owner.as_str()) {
            return Err(Error::StaleContext);
        }
        bound.check_owner().await?;
        Ok(bound.owner.clone())
    }
}
impl Binding {
    fn retire(&self) {
        self.watch.live.store(false, Ordering::Release);
    }
    async fn check_owner(&self) -> Result<(), Error> {
        self.check_changes().await?;
        let bus = zbus::fdo::DBusProxy::new(&self.connection)
            .await
            .map_err(|_| Error::Backend)?;
        let name = BLUEZ.try_into().map_err(|_| Error::InvalidInput)?;
        let current = bus.get_name_owner(name).await.map_err(|e| {
            self.retire();
            match e {
                zbus::fdo::Error::NameHasNoOwner(_) => Error::Unavailable,
                _ => Error::Backend,
            }
        })?;
        self.check_changes().await?;
        if current == self.owner && self.watch.live.load(Ordering::Acquire) {
            Ok(())
        } else {
            self.retire();
            Err(Error::StaleContext)
        }
    }
    async fn check_changes(&self) -> Result<(), Error> {
        if !self.watch.live.load(Ordering::Acquire) {
            return Err(Error::StaleContext);
        }
        let mut changes = self.watch.changes.lock().await;
        if !self.watch.live.load(Ordering::Acquire) {
            return Err(Error::StaleContext);
        }
        match futures_lite::future::poll_once(changes.next()).await {
            None if self.watch.live.load(Ordering::Acquire) => Ok(()),
            None => Err(Error::StaleContext),
            Some(Some(Ok(_))) => {
                self.retire();
                Err(Error::StaleContext)
            }
            Some(Some(Err(_))) => {
                self.retire();
                Err(Error::Backend)
            }
            Some(None) => {
                self.retire();
                Err(Error::Unavailable)
            }
        }
    }
}
enum Operation {
    RegisterApplication,
    UnregisterApplication,
    RegisterAdvertisement,
    UnregisterAdvertisement,
}

#[cfg(test)]
pub(crate) mod tests;
