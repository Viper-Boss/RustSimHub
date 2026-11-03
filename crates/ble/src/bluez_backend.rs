// SPDX-License-Identifier: AGPL-3.0-only
//! One epoch on a dedicated injected connection. Actual GattBackendV1 adapter;
//! the host retains callbacks, owner-loss wait and approved security/Core ports.
use crate::{
    bluez_advertisement::{AdvertisementExport, ReleaseEvent},
    bluez_control::BluezControl,
    bluez_gatt::{CallbackRequest, GattExport},
    bluez_peer::PeerLease,
    framing::OpaqueBytes,
    manifest::{Characteristic, FixedAdvertisement, FixedManifest},
    ports::{Epoch, GattBackendV1, Peer, Pending},
    Error,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::sync::{mpsc, oneshot, Mutex as AsyncMutex};

#[derive(Default)]
struct State {
    epoch: Option<Epoch>,
    app: Option<Arc<GattExport>>,
    callbacks: Option<mpsc::Receiver<CallbackRequest>>,
    advertisement: Option<Arc<AdvertisementExport>>,
    release: Option<oneshot::Receiver<ReleaseEvent>>,
    app_maybe: bool,
    app_ready: bool,
    adv_maybe: bool,
    stopping: bool,
    closed: bool,
    uncertain: bool,
}
struct Shared {
    control: AsyncMutex<BluezControl>,
    connection: zbus::Connection,
    state: Mutex<State>,
    operation: Arc<AsyncMutex<()>>,
    dead: AtomicBool,
}
impl Shared {
    fn state(&self) -> Result<std::sync::MutexGuard<'_, State>, Error> {
        self.state.lock().map_err(|_| Error::Unknown)
    }
    fn epoch(&self, epoch: Epoch) -> Result<(), Error> {
        if self.state()?.epoch != Some(epoch) {
            return Err(Error::StaleContext);
        }
        Ok(())
    }
    fn fence(&self) {
        self.dead.store(true, Ordering::Release);
        if let Ok(state) = self.state.lock() {
            if let Some(app) = &state.app {
                app.retire_local();
            }
            if let Some(advertisement) = &state.advertisement {
                advertisement.retire_local();
            }
        }
    }
    fn app(&self, epoch: Epoch) -> Result<Arc<GattExport>, Error> {
        self.epoch(epoch)?;
        let state = self.state()?;
        if self.dead.load(Ordering::Acquire) || state.stopping || !state.app_ready {
            return Err(Error::StaleContext);
        }
        state.app.clone().ok_or(Error::Unavailable)
    }
    fn stop(&self) -> Result<(), Error> {
        let mut state = self.state()?;
        state.stopping = true;
        if let Some(app) = &state.app {
            app.retire_local();
        }
        Ok(())
    }
}
struct Incomplete(Arc<Shared>, bool);
impl Drop for Incomplete {
    fn drop(&mut self) {
        if !self.1 {
            if let Ok(mut state) = self.0.state.lock() {
                state.uncertain = true;
            }
            self.0.fence();
        }
    }
}
/// One epoch only. Create a new dedicated connection and fresh lifecycle domain
/// for another instance. No service, bus connection or security fallback starts.
#[must_use = "retain backend/lifecycle through late operation compensation"]
pub struct BluezBackend {
    shared: Arc<Shared>,
}
/// Trusted host integration: take each sink once, poll owner loss and peer
/// health, bind only a real Device1 lease, and keep Core/security authorization.
#[must_use = "retain and poll events; dropping this handle fences local I/O"]
pub struct BluezHost {
    shared: Arc<Shared>,
    owner_loss: Option<Pending<()>>,
}
impl BluezBackend {
    pub fn new(control: BluezControl) -> (Self, BluezHost) {
        let owner_loss = control.wait_owner_loss();
        let shared = Arc::new(Shared {
            connection: control.connection(),
            control: AsyncMutex::new(control),
            state: Mutex::new(State::default()),
            operation: Arc::new(AsyncMutex::new(())),
            dead: AtomicBool::new(false),
        });
        (
            Self {
                shared: shared.clone(),
            },
            BluezHost {
                shared,
                owner_loss: Some(owner_loss),
            },
        )
    }
}
impl Drop for BluezBackend {
    fn drop(&mut self) {
        self.shared.fence();
    }
}
impl Drop for BluezHost {
    fn drop(&mut self) {
        self.shared.fence();
    }
}
impl BluezHost {
    /// Select exactly one connected Device1 from the pinned BlueZ object tree.
    /// No request payload may select a peer, and the subsequent lease performs
    /// its original subscribe/snapshot/current-owner checks.
    pub async fn bind_connected_peer(&self, epoch: Epoch) -> Result<Option<Peer>, Error> {
        self.shared.app(epoch)?;
        let control = self.shared.control.lock().await;
        let proxy = zbus::Proxy::new(
            &self.shared.connection,
            control.owner(),
            "/",
            "org.freedesktop.DBus.ObjectManager",
        )
        .await
        .map_err(|_| Error::Backend)?;
        type Objects = std::collections::HashMap<
            zbus::zvariant::OwnedObjectPath,
            std::collections::HashMap<
                String,
                std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
            >,
        >;
        let objects: Objects = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            proxy.call("GetManagedObjects", &()),
        )
        .await
        .map_err(|_| Error::Deadline)?
        .map_err(|_| Error::Backend)?;
        if objects.len() > 256 {
            return Err(Error::Exhausted);
        }
        let mut selected = None;
        for (path, interfaces) in objects {
            if !path.as_str().starts_with("/org/bluez/hci0/dev_") {
                continue;
            }
            if interfaces
                .get("org.bluez.Device1")
                .and_then(|p| p.get("Connected"))
                .is_some_and(|v| bool::try_from(v).unwrap_or(false))
                && selected.replace(path.to_string()).is_some()
            {
                return Err(Error::Busy);
            }
        }
        drop(control);
        match selected {
            Some(path) => self.bind_peer(epoch, path).await.map(Some),
            None => Ok(None),
        }
    }
    pub fn take_owner_loss(&mut self) -> Result<Pending<()>, Error> {
        self.owner_loss.take().ok_or(Error::Busy)
    }
    pub fn take_callbacks(&self, epoch: Epoch) -> Result<mpsc::Receiver<CallbackRequest>, Error> {
        self.shared.app(epoch)?;
        self.shared.state()?.callbacks.take().ok_or(Error::Busy)
    }
    pub fn take_release(&self, epoch: Epoch) -> Result<oneshot::Receiver<ReleaseEvent>, Error> {
        self.shared.epoch(epoch)?;
        if self.shared.dead.load(Ordering::Acquire) {
            return Err(Error::StaleContext);
        }
        self.shared.state()?.release.take().ok_or(Error::Busy)
    }
    /// A verified transport peer, never an application/Core authorization.
    pub fn bind_peer(&self, epoch: Epoch, device: String) -> Pending<Peer> {
        let shared = self.shared.clone();
        Box::pin(async move {
            let app = shared.app(epoch)?;
            let lease = {
                let control = shared.control.lock().await;
                PeerLease::bind(&control, epoch, &device).await?
            };
            let peer = lease.peer().clone();
            app.bind_peer(lease).await?;
            shared.app(epoch)?;
            Ok(peer)
        })
    }
    pub fn check_peer(&self, epoch: Epoch) -> Pending<()> {
        let shared = self.shared.clone();
        Box::pin(async move {
            let app = shared.app(epoch)?;
            app.check_peer_current().await
        })
    }
    /// Emergency local fence/close. This does not confirm remote unregister or
    /// RF cleanup and never makes a failed lifecycle report quiescence.
    pub fn close_connection(&self) -> Pending<()> {
        let shared = self.shared.clone();
        Box::pin(async move {
            shared.fence();
            shared
                .connection
                .clone()
                .close()
                .await
                .map_err(|_| Error::Backend)
        })
    }
}
impl GattBackendV1 for BluezBackend {
    fn register_application(&mut self, epoch: Epoch, _: FixedManifest) -> Pending<()> {
        let shared = self.shared.clone();
        Box::pin(async move {
            let _operation = shared
                .operation
                .clone()
                .try_lock_owned()
                .map_err(|_| Error::Busy)?;
            if shared.dead.load(Ordering::Acquire) {
                return Err(Error::StaleContext);
            }
            {
                let mut state = shared.state()?;
                if state.epoch.is_some() {
                    return Err(Error::Busy);
                }
                state.epoch = Some(epoch);
            }
            let mut attempt = Incomplete(shared.clone(), false);
            let (app, callbacks) = {
                let control = shared.control.lock().await;
                GattExport::publish(&control, epoch).await?
            };
            let app = Arc::new(app);
            {
                let mut state = shared.state()?;
                if shared.dead.load(Ordering::Acquire) {
                    return Err(Error::StaleContext);
                }
                state.app = Some(app);
                state.callbacks = Some(callbacks);
                state.app_maybe = true;
            }
            let operation = shared.control.lock().await.register_application();
            operation.await?;
            if shared.dead.load(Ordering::Acquire) {
                return Err(Error::StaleContext);
            }
            shared.state()?.app_ready = true;
            attempt.1 = true;
            Ok(())
        })
    }
    fn register_advertisement(&mut self, epoch: Epoch, _: FixedAdvertisement) -> Pending<()> {
        let shared = self.shared.clone();
        Box::pin(async move {
            let _operation = shared
                .operation
                .clone()
                .try_lock_owned()
                .map_err(|_| Error::Busy)?;
            shared.app(epoch)?;
            if shared.state()?.advertisement.is_some() {
                return Err(Error::Busy);
            }
            let mut attempt = Incomplete(shared.clone(), false);
            let (advertisement, release) = {
                let control = shared.control.lock().await;
                AdvertisementExport::publish(&control, epoch).await?
            };
            {
                let mut state = shared.state()?;
                if shared.dead.load(Ordering::Acquire) {
                    return Err(Error::StaleContext);
                }
                state.advertisement = Some(Arc::new(advertisement));
                state.release = Some(release);
                state.adv_maybe = true;
            }
            let operation = shared.control.lock().await.register_advertisement();
            operation.await?;
            if shared.dead.load(Ordering::Acquire) {
                return Err(Error::StaleContext);
            }
            attempt.1 = true;
            Ok(())
        })
    }
    fn unregister_advertisement(&mut self, epoch: Epoch) -> Pending<()> {
        let shared = self.shared.clone();
        Box::pin(async move {
            let _operation = shared
                .operation
                .clone()
                .try_lock_owned()
                .map_err(|_| Error::Busy)?;
            shared.epoch(epoch)?;
            shared.stop()?;
            let mut attempt = Incomplete(shared.clone(), false);
            let (maybe, closed) = {
                let state = shared.state()?;
                (state.adv_maybe, state.closed)
            };
            if closed {
                attempt.1 = true;
                return if maybe { Err(Error::Unknown) } else { Ok(()) };
            }
            if maybe {
                let operation = shared.control.lock().await.unregister_advertisement();
                operation.await?;
                shared.state()?.adv_maybe = false;
            }
            if let Some(advertisement) = &shared.state()?.advertisement {
                advertisement.retire_local();
            }
            attempt.1 = true;
            Ok(())
        })
    }
    fn unregister_application(&mut self, epoch: Epoch) -> Pending<()> {
        let shared = self.shared.clone();
        Box::pin(async move {
            let _operation = shared
                .operation
                .clone()
                .try_lock_owned()
                .map_err(|_| Error::Busy)?;
            shared.epoch(epoch)?;
            shared.stop()?;
            let (maybe, closed) = {
                let state = shared.state()?;
                (state.app_maybe, state.closed)
            };
            if closed {
                let state = shared.state()?;
                return if state.uncertain || state.app_maybe || state.adv_maybe {
                    Err(Error::Unknown)
                } else {
                    Ok(())
                };
            }
            let mut attempt = Incomplete(shared.clone(), false);
            let result = if maybe {
                let operation = shared.control.lock().await.unregister_application();
                operation.await
            } else {
                Ok(())
            };
            if result.is_ok() {
                shared.state()?.app_maybe = false;
            }
            shared.fence();
            let close = shared
                .connection
                .clone()
                .close()
                .await
                .map_err(|_| Error::Backend);
            if close.is_ok() {
                shared.state()?.closed = true;
            }
            attempt.1 = true;
            result?;
            close?;
            let state = shared.state()?;
            if state.adv_maybe || state.uncertain {
                return Err(Error::Unknown);
            }
            Ok(())
        })
    }
    fn notify(
        &mut self,
        epoch: Epoch,
        peer: Peer,
        kind: Characteristic,
        frame: OpaqueBytes,
    ) -> Pending<()> {
        let shared = self.shared.clone();
        Box::pin(async move {
            let app = shared.app(epoch)?;
            app.notify(epoch, peer, kind, frame).await
        })
    }
}
#[cfg(test)]
mod tests;
