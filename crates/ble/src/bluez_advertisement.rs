// SPDX-License-Identifier: AGPL-3.0-only
//! One fixed local advertisement per connection epoch. No radio registration,
//! Core command, secret, arbitrary property, or implicit system-bus connection.
use crate::{
    bluez_control::{BluezControl, CallbackScope},
    manifest,
    ports::Epoch,
    Error,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::sync::oneshot;
use zbus::{message::Header, zvariant::Signature, Connection};

/// Sender-checked metadata, not a recovery capability or proof of business authority.
pub struct ReleaseEvent {
    epoch: Epoch,
    owner: String,
}
impl ReleaseEvent {
    pub fn epoch(&self) -> Epoch {
        self.epoch
    }
    pub fn owner(&self) -> &str {
        &self.owner
    }
}
struct Delivery {
    sender: Option<oneshot::Sender<ReleaseEvent>>,
    failed: bool,
}
struct Publication {
    live: AtomicBool,
    scope: CallbackScope,
    epoch: Epoch,
    delivery: Mutex<Delivery>,
}
impl Publication {
    fn retire(&self) {
        self.live.store(false, Ordering::Release);
        if let Ok(mut delivery) = self.delivery.lock() {
            delivery.sender.take();
        }
    }
}
struct Advertisement(Arc<Publication>);
#[zbus::interface(name = "org.bluez.LEAdvertisement1")]
impl Advertisement {
    #[zbus(property, name = "Type")]
    fn advertisement_type(&self) -> &str {
        "peripheral"
    }
    #[zbus(property, name = "ServiceUUIDs")]
    fn service_uuids(&self) -> Vec<&str> {
        vec![manifest::SERVICE_UUID]
    }
    async fn release(&self, #[zbus(header)] header: Header<'_>) -> zbus::fdo::Result<()> {
        if header.signature() != &Signature::Unit {
            return Err(zbus::fdo::Error::InvalidArgs(
                "empty callback required".into(),
            ));
        }
        let denied = || zbus::fdo::Error::AccessDenied("invalid callback context".into());
        if !self.0.live.load(Ordering::Acquire) {
            return Err(denied());
        }
        let owner = self
            .0
            .scope
            .check_sender(header.sender().map(|s| s.as_str()))
            .await
            .map_err(|_| denied())?;
        let mut delivery = self.0.delivery.lock().map_err(|_| denied())?;
        if !self.0.live.load(Ordering::Acquire) {
            return Err(denied());
        }
        if delivery.failed {
            return Err(zbus::fdo::Error::Failed("callback sink unavailable".into()));
        }
        if let Some(sender) = delivery.sender.take() {
            if sender
                .send(ReleaseEvent {
                    epoch: self.0.epoch,
                    owner: owner.to_string(),
                })
                .is_err()
            {
                delivery.failed = true;
                return Err(zbus::fdo::Error::Failed("callback sink unavailable".into()));
            }
        }
        Ok(())
    }
}
/// Retain through remote cleanup. Drop only fences local callbacks; it does not
/// unregister on BlueZ or claim quiescence. The fixed local object remains as a
/// tombstone until this connection closes, so it cannot be reused for a new epoch.
#[must_use = "retain until remote cleanup and connection shutdown complete"]
pub struct AdvertisementExport {
    connection: Connection,
    publication: Arc<Publication>,
}
impl AdvertisementExport {
    pub async fn publish(
        control: &BluezControl,
        epoch: Epoch,
    ) -> Result<(Self, oneshot::Receiver<ReleaseEvent>), Error> {
        control.check_current().await?;
        let (sender, receiver) = oneshot::channel();
        let publication = Arc::new(Publication {
            live: AtomicBool::new(true),
            scope: control.callback_scope(),
            epoch,
            delivery: Mutex::new(Delivery {
                sender: Some(sender),
                failed: false,
            }),
        });
        let mut guard = PublishGuard {
            publication: publication.clone(),
            armed: true,
        };
        let connection = control.connection();
        if !connection
            .object_server()
            .at(
                manifest::ADVERTISEMENT_PATH,
                Advertisement(publication.clone()),
            )
            .await
            .map_err(|_| Error::Backend)?
        {
            return Err(Error::Busy);
        }
        control.check_current().await?;
        guard.armed = false;
        Ok((
            Self {
                connection,
                publication,
            },
            receiver,
        ))
    }
    /// This is a local fence only. Unregister using the captured control port first.
    pub fn retire_local(&self) {
        self.publication.retire();
    }
    /// Close the whole supplied connection AFTER remote cleanup. Use a dedicated
    /// connection or wait until all objects on this epoch's connection are done.
    /// Local close is not proof that a physical advertisement has stopped.
    pub async fn close_connection(&self) -> Result<(), Error> {
        self.retire_local();
        self.connection
            .clone()
            .close()
            .await
            .map_err(|_| Error::Backend)
    }
}
impl Drop for AdvertisementExport {
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
