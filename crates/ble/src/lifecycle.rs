// SPDX-License-Identifier: AGPL-3.0-only
//! Pollable async registration owner. The host must poll at the exposed deadline
//! as well as on backend wakeups, and retain this owner while cleanup is pending.
//! No future is discarded merely because its five-second deadline expired.
use crate::{
    framing::{InputKind, OpaqueBytes, OutputKind},
    manifest::{FixedAdvertisement, FixedManifest},
    ports::SecureReply,
    ports::{Epoch, GattBackendV1, Peer, Pending, SecurePortV1, State, DBUS_CALL_MS},
    Error,
};
use std::task::{Context, Poll};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    RegisterApplication,
    RegisterAdvertisement,
    UnregisterAdvertisement,
    ClosePeer,
    UnregisterApplication,
}

struct Job {
    action: Action,
    deadline: u64,
    expired: bool,
    future: Pending<()>,
}

/// Minted for one accepted connection. This proves transport freshness only,
/// never cryptographic authentication or Core recovery authorization.
#[derive(Clone, PartialEq, Eq)]
pub struct Connection {
    epoch: Epoch,
    peer: Peer,
    serial: u64,
}

impl Connection {
    pub const fn epoch(&self) -> Epoch {
        self.epoch
    }
    pub fn peer(&self) -> &Peer {
        &self.peer
    }
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Connection(..)")
    }
}

/// This handles lifecycle only. D-19 transport/session integration must own and
/// clear connection-local framing/queues before exposing any characteristic I/O.
/// The security port and backend are explicit constructor dependencies; neither
/// a runtime mock switch nor a permissive fallback exists.
#[must_use = "retain and poll until resources and late operations are cleaned up"]
pub struct Lifecycle<B, S> {
    backend: B,
    secure: S,
    owner: String,
    owner_valid: bool,
    domain: [u8; 16],
    next_start: Option<u64>,
    epoch: Option<Epoch>,
    state: State,
    peer: Option<Connection>,
    next_connection: Option<u64>,
    app_maybe: bool,
    adv_maybe: bool,
    adv_cleanup_attempted: bool,
    app_cleanup_attempted: bool,
    stopping: bool,
    cleanup_failed: bool,
    last_ms: Option<u64>,
    last_error: Option<Error>,
    job: Option<Job>,
}

impl<B: GattBackendV1, S: SecurePortV1> Lifecycle<B, S> {
    /// `owner` must be the unique owner pinned by this exact backend instance.
    /// Fresh boot-domain allocation belongs to the trusted main process. A new
    /// instance cannot prove that its caller did not reuse another boot domain.
    pub fn new(
        backend: B,
        secure: Option<S>,
        owner: &str,
        domain: [u8; 16],
    ) -> Result<Self, Error> {
        Peer::from_backend_callback(owner, "/org/bluez/adapter")?;
        Epoch::new(domain, 1)?;
        Ok(Self {
            backend,
            secure: secure.ok_or(Error::Unavailable)?,
            owner: owner.to_owned(),
            owner_valid: true,
            domain,
            next_start: Some(1),
            epoch: None,
            state: State::Disabled,
            peer: None,
            next_connection: Some(1),
            app_maybe: false,
            adv_maybe: false,
            adv_cleanup_attempted: false,
            app_cleanup_attempted: false,
            stopping: false,
            cleanup_failed: false,
            last_ms: None,
            last_error: None,
            job: None,
        })
    }

    pub const fn state(&self) -> State {
        self.state
    }

    pub const fn epoch(&self) -> Option<Epoch> {
        self.epoch
    }

    pub const fn last_error(&self) -> Option<Error> {
        self.last_error
    }

    pub fn deadline_ms(&self) -> Option<u64> {
        self.job
            .as_ref()
            .filter(|job| !job.expired)
            .map(|job| job.deadline)
    }

    pub fn has_pending_backend(&self) -> bool {
        self.job.is_some()
    }

    pub fn is_quiescent(&self) -> bool {
        // Removing the local handle does not prove external session cleanup.
        // A failed close remains unknown even after all D-Bus objects are gone.
        !self.cleanup_failed
            && self.job.is_none()
            && !self.app_maybe
            && !self.adv_maybe
            && self.peer.is_none()
    }

    fn clock(&mut self, now: u64) -> Result<u64, Error> {
        if self.last_ms.is_some_and(|last| now < last) {
            return Err(Error::InvalidInput);
        }
        // Check before issuing *any* new backend call, including cleanup.
        let deadline = now.checked_add(DBUS_CALL_MS).ok_or(Error::Deadline)?;
        self.last_ms = Some(now);
        Ok(deadline)
    }

    pub fn start(&mut self, now: u64) -> Result<Epoch, Error> {
        if !self.owner_valid {
            return Err(Error::StaleContext);
        }
        if self.state != State::Disabled || !self.is_quiescent() {
            return Err(Error::Busy);
        }
        let deadline = self.clock(now)?;
        let start = self.next_start.ok_or(Error::Exhausted)?;
        let epoch = Epoch::new(self.domain, start)?;
        self.next_start = start.checked_add(1);
        self.epoch = Some(epoch);
        self.stopping = false;
        self.cleanup_failed = false;
        self.last_error = None;
        self.adv_cleanup_attempted = false;
        self.app_cleanup_attempted = false;
        self.app_maybe = true;
        self.state = State::Registering;
        self.job = Some(Job {
            action: Action::RegisterApplication,
            deadline,
            expired: false,
            future: self.backend.register_application(epoch, FixedManifest),
        });
        Ok(epoch)
    }

    fn context(&self, epoch: Epoch, owner: &str) -> Result<(), Error> {
        if self.epoch != Some(epoch) || owner != self.owner {
            return Err(Error::StaleContext);
        }
        Ok(())
    }

    /// Callback source must be established by the actual pinned BlueZ backend.
    /// This method never accepts a peer identity from application payload bytes.
    pub fn connected(&mut self, epoch: Epoch, peer: Peer, now: u64) -> Result<(), Error> {
        self.context(epoch, peer.owner())?;
        if !self.owner_valid || self.stopping || self.job.is_some() {
            return Err(Error::Busy);
        }
        self.clock(now)?;
        if self.state == State::Connected && self.peer.as_ref().is_some_and(|c| c.peer == peer) {
            return Ok(());
        }
        if self.state != State::Advertising || self.peer.is_some() {
            return Err(Error::Busy);
        }
        let serial = self.next_connection.ok_or(Error::Exhausted)?;
        self.next_connection = serial.checked_add(1);
        self.peer = Some(Connection {
            epoch,
            peer,
            serial,
        });
        self.state = State::Connected;
        Ok(())
    }

    /// True only while the exact connection remains live. The transport must
    /// repeat this check after every await before publishing any opaque reply.
    pub fn is_current(&self, connection: &Connection) -> bool {
        self.epoch == Some(connection.epoch)
            && self.owner_valid
            && !self.stopping
            && self.state == State::Connected
            && self.peer.as_ref() == Some(connection)
    }

    pub fn connection(&self) -> Option<Connection> {
        self.peer.as_ref().filter(|c| self.is_current(c)).cloned()
    }

    pub(crate) fn receive_secure(
        &mut self,
        connection: &Connection,
        kind: InputKind,
        bytes: OpaqueBytes,
    ) -> Result<Pending<SecureReply>, Error> {
        if !self.is_current(connection) {
            return Err(Error::StaleContext);
        }
        Ok(self
            .secure
            .receive(connection.epoch, connection.peer.clone(), kind, bytes))
    }

    pub(crate) fn notify(
        &mut self,
        connection: &Connection,
        kind: OutputKind,
        bytes: OpaqueBytes,
    ) -> Result<Pending<()>, Error> {
        if !self.is_current(connection) {
            return Err(Error::StaleContext);
        }
        Ok(self.backend.notify(
            connection.epoch,
            connection.peer.clone(),
            kind.characteristic(),
            bytes,
        ))
    }

    pub fn disconnected(&mut self, connection: &Connection, now: u64) -> Result<(), Error> {
        self.context(connection.epoch, connection.peer.owner())?;
        if self.peer.as_ref() != Some(connection) || self.job.is_some() {
            return Err(Error::StaleContext);
        }
        let deadline = self.clock(now)?;
        let peer = self.peer.take().ok_or(Error::StaleContext)?;
        self.state = State::Stopping;
        self.job = Some(Job {
            action: Action::ClosePeer,
            deadline,
            expired: false,
            future: self.secure.disconnected(peer.epoch, peer.peer),
        });
        Ok(())
    }

    pub fn stop(&mut self, now: u64) -> Result<(), Error> {
        self.clock(now)?;
        if self.state == State::Disabled {
            return Ok(());
        }
        self.stopping = true;
        if self.state != State::Unknown {
            self.state = State::Stopping;
        }
        if self.job.is_none() {
            self.cleanup(now)?;
        }
        Ok(())
    }

    /// Release is an authenticated backend callback, not a request to delete
    /// arbitrary advertisements. It makes repeated Release/unregister idempotent.
    pub fn advertisement_released(
        &mut self,
        epoch: Epoch,
        owner: &str,
        now: u64,
    ) -> Result<(), Error> {
        self.context(epoch, owner)?;
        self.clock(now)?;
        self.adv_maybe = false;
        self.stop(now)
    }

    pub fn owner_lost(&mut self, owner: &str, now: u64) -> Result<(), Error> {
        if owner != self.owner {
            return Err(Error::StaleContext);
        }
        self.owner_valid = false;
        self.stop(now)
    }

    /// Poll at least at `deadline_ms()`. Timeout closes the logical session but
    /// retains the owning future; its late success still receives compensation.
    /// At most eight immediate actions are processed per call, bounding work.
    pub fn poll(&mut self, cx: &mut Context<'_>, now: u64) -> Result<State, Error> {
        self.clock(now)?;
        for _ in 0..8 {
            let Some(job) = self.job.as_mut() else {
                break;
            };
            if now >= job.deadline && !job.expired {
                job.expired = true;
                self.last_error = Some(Error::Deadline);
                self.stopping = true;
                self.state = State::Unknown;
            }
            let Poll::Ready(result) = job.future.as_mut().poll(cx) else {
                break;
            };
            let action = job.action;
            self.job = None;
            self.completed(action, result, now)?;
        }
        Ok(self.state)
    }

    fn completed(
        &mut self,
        action: Action,
        result: Result<(), Error>,
        now: u64,
    ) -> Result<(), Error> {
        if let Err(error) = result {
            self.last_error = Some(error);
            self.stopping = true;
            self.state = State::Unknown;
            if matches!(action, Action::UnregisterApplication | Action::ClosePeer)
                || (action == Action::UnregisterAdvertisement && self.adv_maybe)
            {
                self.cleanup_failed = true;
            }
        }
        let epoch = self.epoch.ok_or(Error::StaleContext)?;
        match action {
            Action::RegisterApplication if result.is_ok() && !self.stopping => {
                self.adv_maybe = true;
                self.job = Some(Job {
                    action: Action::RegisterAdvertisement,
                    deadline: self.clock(now)?,
                    expired: false,
                    future: self
                        .backend
                        .register_advertisement(epoch, FixedAdvertisement),
                });
                return Ok(());
            }
            Action::RegisterAdvertisement if result.is_ok() && !self.stopping => {
                self.state = State::Advertising;
                return Ok(());
            }
            Action::UnregisterAdvertisement if result.is_ok() => self.adv_maybe = false,
            Action::UnregisterApplication if result.is_ok() => self.app_maybe = false,
            Action::ClosePeer if result.is_ok() && !self.stopping => {
                self.state = State::Advertising;
                return Ok(());
            }
            _ => {}
        }
        self.cleanup(now)
    }

    fn cleanup(&mut self, now: u64) -> Result<(), Error> {
        let epoch = self.epoch.ok_or(Error::StaleContext)?;
        let deadline = self.clock(now)?;
        let next = if self.adv_maybe && !self.adv_cleanup_attempted {
            self.adv_cleanup_attempted = true;
            Some((
                Action::UnregisterAdvertisement,
                self.backend.unregister_advertisement(epoch),
            ))
        } else if let Some(peer) = self.peer.take() {
            Some((
                Action::ClosePeer,
                self.secure.disconnected(epoch, peer.peer),
            ))
        } else if self.app_maybe && !self.app_cleanup_attempted {
            self.app_cleanup_attempted = true;
            Some((
                Action::UnregisterApplication,
                self.backend.unregister_application(epoch),
            ))
        } else {
            None
        };
        if let Some((action, future)) = next {
            self.job = Some(Job {
                action,
                deadline,
                expired: false,
                future,
            });
        } else if self.cleanup_failed || !self.is_quiescent() {
            self.state = State::Unknown;
        } else {
            self.state = State::Disabled;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
