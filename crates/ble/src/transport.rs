// SPDX-License-Identifier: AGPL-3.0-only
//! Connects the actual bounded framer, injected security port and async lifecycle.
//! It never treats an opaque message, connection token or pairing as authority.
use crate::{
    framing::{
        Assembler, Budget, FrameTicket, InputKind, OpaqueBytes, Outbox, OutputKind, WriteOptions,
        QUEUED_RESULTS,
    },
    lifecycle::{Connection, Lifecycle},
    manifest::Characteristic,
    ports::{Epoch, GattBackendV1, Peer, Pending, SecurePortV1, SecureReply, State, DBUS_CALL_MS},
    Error,
};
use std::task::{Context, Poll};

struct Link {
    connection: Connection,
    assembler: Assembler,
    outbox: Outbox,
    budget: Budget,
    last_result: Option<OpaqueBytes>,
}

enum Work {
    Secure {
        kind: InputKind,
        future: Pending<SecureReply>,
    },
    Notify {
        ticket: FrameTicket,
        kind: OutputKind,
        bytes: OpaqueBytes,
        future: Pending<()>,
    },
}

struct Io {
    connection: Connection,
    deadline: u64,
    expired: bool,
    work: Work,
}

enum Done {
    Secure(InputKind, Result<SecureReply, Error>),
    Notify(FrameTicket, OutputKind, OpaqueBytes, Result<(), Error>),
}

#[must_use = "retain and poll through late I/O completion and cleanup"]
pub struct Transport<B, S> {
    lifecycle: Lifecycle<B, S>,
    link: Option<Link>,
    io: Option<Io>,
    last_error: Option<Error>,
}

impl<B: GattBackendV1, S: SecurePortV1> Transport<B, S> {
    pub fn new(
        backend: B,
        secure: Option<S>,
        owner: &str,
        domain: [u8; 16],
    ) -> Result<Self, Error> {
        Ok(Self {
            lifecycle: Lifecycle::new(backend, secure, owner, domain)?,
            link: None,
            io: None,
            last_error: None,
        })
    }

    pub fn state(&self) -> State {
        if self.io.as_ref().is_some_and(|io| io.expired)
            || (self.io.is_some() && self.lifecycle.state() == State::Disabled)
        {
            State::Unknown
        } else {
            self.lifecycle.state()
        }
    }
    pub fn last_error(&self) -> Option<Error> {
        self.last_error.or(self.lifecycle.last_error())
    }
    pub fn is_quiescent(&self) -> bool {
        self.io.is_none() && self.link.is_none() && self.lifecycle.is_quiescent()
    }
    pub fn deadline_ms(&self) -> Option<u64> {
        [
            self.lifecycle.deadline_ms(),
            self.io
                .as_ref()
                .filter(|io| !io.expired)
                .map(|io| io.deadline),
            self.link.as_ref().and_then(|l| l.assembler.deadline_ms()),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    pub fn start(&mut self, now: u64) -> Result<Epoch, Error> {
        if self.io.is_some() || self.link.is_some() {
            return Err(Error::Busy);
        }
        let epoch = self.lifecycle.start(now)?;
        self.last_error = None;
        Ok(epoch)
    }

    pub fn connected(&mut self, epoch: Epoch, peer: Peer, now: u64) -> Result<Connection, Error> {
        if let Some(link) = self.link.as_ref() {
            if link.connection.epoch() == epoch
                && link.connection.peer() == &peer
                && self.lifecycle.is_current(&link.connection)
            {
                return Ok(link.connection.clone());
            }
            return Err(Error::Busy);
        }
        if self.io.is_some() {
            return Err(Error::Busy);
        }
        self.lifecycle.connected(epoch, peer, now)?;
        let connection = self.lifecycle.connection().ok_or(Error::StaleContext)?;
        self.link = Some(Link {
            connection: connection.clone(),
            assembler: Assembler::new(),
            outbox: Outbox::new(),
            budget: Budget::INITIAL,
            last_result: None,
        });
        Ok(connection)
    }

    fn link(&self, connection: &Connection) -> Result<&Link, Error> {
        self.link
            .as_ref()
            .filter(|l| &l.connection == connection && self.lifecycle.is_current(connection))
            .ok_or(Error::StaleContext)
    }

    /// Call only with reliable MTU evidence from the actual backend callback.
    pub fn negotiated_mtu(&mut self, connection: &Connection, mtu: u16) -> Result<(), Error> {
        self.link(connection)?;
        let budget = Budget::negotiated_att_mtu(mtu)?;
        self.link.as_mut().ok_or(Error::StaleContext)?.budget = budget;
        Ok(())
    }

    pub fn write(
        &mut self,
        connection: &Connection,
        characteristic: Characteristic,
        options: WriteOptions,
        now: u64,
        frame: &[u8],
    ) -> Result<(), Error> {
        let link = self.link(connection)?;
        if self.io.is_some() || link.outbox.len() >= QUEUED_RESULTS {
            self.abort(now, Error::Busy);
            return Err(Error::Busy);
        }
        let Some(deadline) = now.checked_add(DBUS_CALL_MS) else {
            self.abort(now, Error::Deadline);
            return Err(Error::Deadline);
        };
        let link = self.link.as_mut().ok_or(Error::StaleContext)?;
        let result = link
            .assembler
            .receive(characteristic, options, link.budget, now, frame);
        match result {
            Ok(None) => Ok(()),
            Ok(Some(message)) => {
                let future =
                    self.lifecycle
                        .receive_secure(connection, message.kind, message.bytes)?;
                self.io = Some(Io {
                    connection: connection.clone(),
                    deadline,
                    expired: false,
                    work: Work::Secure {
                        kind: message.kind,
                        future,
                    },
                });
                Ok(())
            }
            Err(error) => {
                self.abort(now, error);
                Err(error)
            }
        }
    }

    /// Read only the last *confirmed encrypted Result frame* for this current
    /// connection, never an Auth frame or bytes retained from another session.
    /// This is a transport cache, not proof that Core still authorizes recovery.
    pub fn read_result(&self, connection: &Connection, offset: u16) -> Result<OpaqueBytes, Error> {
        let link = self.link(connection)?;
        if offset != 0 {
            return Err(Error::InvalidInput);
        }
        let bytes = link.last_result.as_ref().ok_or(Error::Unavailable)?;
        OpaqueBytes::new(bytes.as_slice().to_vec())
    }

    fn abort(&mut self, now: u64, error: Error) {
        self.last_error = Some(error);
        if let Some(mut link) = self.link.take() {
            link.assembler.close();
            link.outbox.close();
            // Pending I/O remains owned by self.io. No automatic retry and no
            // publication after this logical connection has been invalidated.
            if self.lifecycle.is_current(&link.connection) {
                if let Err(cleanup_error) = self.lifecycle.disconnected(&link.connection, now) {
                    self.last_error = Some(cleanup_error);
                }
            }
        }
    }

    pub fn disconnected(&mut self, connection: &Connection, now: u64) -> Result<(), Error> {
        self.link(connection)?;
        self.lifecycle.disconnected(connection, now)?;
        self.link = None;
        Ok(())
    }

    pub fn stop(&mut self, now: u64) -> Result<(), Error> {
        self.lifecycle.stop(now)?;
        self.link = None;
        Ok(())
    }

    pub fn advertisement_released(
        &mut self,
        epoch: Epoch,
        owner: &str,
        now: u64,
    ) -> Result<(), Error> {
        self.lifecycle.advertisement_released(epoch, owner, now)?;
        self.link = None;
        Ok(())
    }

    pub fn owner_lost(&mut self, owner: &str, now: u64) -> Result<(), Error> {
        self.lifecycle.owner_lost(owner, now)?;
        self.link = None;
        Ok(())
    }

    pub fn poll(&mut self, cx: &mut Context<'_>, now: u64) -> Result<State, Error> {
        self.lifecycle.poll(cx, now)?;
        if self
            .link
            .as_ref()
            .is_some_and(|l| !self.lifecycle.is_current(&l.connection))
        {
            self.link = None;
        }
        if let Some(link) = self.link.as_mut() {
            if let Err(error) = link.assembler.expire(now) {
                self.abort(now, error);
            }
        }
        for _ in 0..8 {
            if self.io.is_none() {
                let Some(link) = self.link.as_ref() else {
                    break;
                };
                let Some(frame) = link.outbox.front() else {
                    break;
                };
                let Some(deadline) = now.checked_add(DBUS_CALL_MS) else {
                    self.abort(now, Error::Deadline);
                    break;
                };
                let kind = if frame.characteristic == Characteristic::AuthExchange {
                    OutputKind::Auth
                } else {
                    OutputKind::Result
                };
                let bytes = OpaqueBytes::new(frame.bytes)?;
                let future = self.lifecycle.notify(
                    &link.connection,
                    kind,
                    OpaqueBytes::new(bytes.as_slice().to_vec())?,
                )?;
                self.io = Some(Io {
                    connection: link.connection.clone(),
                    deadline,
                    expired: false,
                    work: Work::Notify {
                        ticket: frame.ticket,
                        kind,
                        bytes,
                        future,
                    },
                });
            }
            let io = self.io.as_mut().ok_or(Error::Unknown)?;
            if now >= io.deadline && !io.expired {
                io.expired = true;
                self.abort(now, Error::Deadline);
            }
            let io = self.io.as_mut().ok_or(Error::Unknown)?;
            let ready = match &mut io.work {
                Work::Secure { future, .. } => future.as_mut().poll(cx).map(|r| (Some(r), None)),
                Work::Notify { future, .. } => future.as_mut().poll(cx).map(|r| (None, Some(r))),
            };
            let Poll::Ready((secure, notify)) = ready else {
                self.lifecycle.poll(cx, now)?;
                return Ok(self.state());
            };
            let io = self.io.take().ok_or(Error::Unknown)?;
            if io.expired
                || !self.lifecycle.is_current(&io.connection)
                || self.link(&io.connection).is_err()
            {
                continue;
            }
            let done = match io.work {
                Work::Secure { kind, .. } => Done::Secure(kind, secure.ok_or(Error::Unknown)?),
                Work::Notify {
                    ticket,
                    kind,
                    bytes,
                    ..
                } => Done::Notify(ticket, kind, bytes, notify.ok_or(Error::Unknown)?),
            };
            self.completed(done, now);
        }
        // More immediately-ready work may remain; do not monopolize the caller
        // or strand a queue when its backend resolves without scheduling a wake.
        if self.io.is_some() || self.link.as_ref().is_some_and(|l| !l.outbox.is_empty()) {
            cx.waker().wake_by_ref();
        }
        self.lifecycle.poll(cx, now)?;
        Ok(self.state())
    }

    fn completed(&mut self, done: Done, now: u64) {
        let Some(link) = self.link.as_mut() else {
            return;
        };
        let result = match done {
            Done::Secure(kind, Ok(reply)) => match (kind, reply) {
                (InputKind::Auth, SecureReply::Handshake(bytes)) => link
                    .outbox
                    .enqueue(OutputKind::Auth, bytes, link.budget)
                    .map(|_| ()),
                (InputKind::Command, SecureReply::Application(bytes)) => link
                    .outbox
                    .enqueue(OutputKind::Result, bytes, link.budget)
                    .map(|_| ()),
                (_, SecureReply::NoReply) => Ok(()),
                _ => Err(Error::Protocol),
            },
            Done::Secure(_, Err(error)) => Err(error),
            Done::Notify(ticket, kind, bytes, Ok(())) => {
                let result = link.outbox.acknowledge(ticket);
                if result.is_ok() && kind == OutputKind::Result {
                    link.last_result = Some(bytes);
                }
                result
            }
            Done::Notify(_, _, _, Err(error)) => Err(error),
        };
        if let Err(error) = result {
            self.abort(now, error);
        }
    }
}

#[cfg(test)]
mod tests;
