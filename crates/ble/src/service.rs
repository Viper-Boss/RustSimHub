// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit one-epoch owning runtime. No automatic restart, Core provider,
//! credentials, pairing, decryptor or authorization is supplied by this module.
use crate::{
    bluez_backend::{BluezBackend, BluezHost},
    bluez_control::BluezControl,
    bluez_gatt::{CallbackOperation, CallbackReply, CallbackRequest},
    core_port::{CoreEndpoint, UnixPortFence, UnixSecurePort},
    framing::WriteOptions,
    lifecycle::Connection,
    ports::{Epoch, State},
    transport::Transport,
    Error,
};
use serde::Deserialize;
use std::{future::Future, path::PathBuf, time::Duration};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::Instant,
};

pub const CORE_SOCKET: &str = "/run/rustsimhub/core-recovery/recovery.sock";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    format: u8,
    core_uid: u32,
    ble_uid: u32,
    ble_gid: u32,
}
/// Bounded Root-provided role policy; not a phone message or enable grant.
/// Actual identities, directory/socket metadata and kernel peer UID are checked
/// independently by CoreEndpoint before any BlueZ registration.
pub fn endpoint_from_policy(bytes: &[u8]) -> Result<CoreEndpoint, Error> {
    if bytes.len() > 8192 || bytes.iter().copied().find(|b| !b.is_ascii_whitespace()) != Some(b'{')
    {
        return Err(Error::InvalidInput);
    }
    let p: Policy = serde_json::from_slice(bytes).map_err(|_| Error::InvalidInput)?;
    if p.format != 1 {
        return Err(Error::InvalidInput);
    }
    CoreEndpoint::from_installed_roles(PathBuf::from(CORE_SOCKET), p.core_uid, p.ble_uid, p.ble_gid)
}
struct Watch(Option<JoinHandle<()>>);
impl Drop for Watch {
    fn drop(&mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
        }
    }
}
struct Driver {
    transport: Transport<BluezBackend, UnixSecurePort>,
    host: BluezHost,
    fence: UnixPortFence,
    epoch: Epoch,
    clock: Instant,
    connection: Option<Connection>,
    callbacks: Option<mpsc::Receiver<CallbackRequest>>,
    release: Option<oneshot::Receiver<crate::bluez_advertisement::ReleaseEvent>>,
    lost: mpsc::Sender<()>,
    watch: Watch,
}
impl Driver {
    fn now(&self) -> u64 {
        self.clock.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
    }
    async fn poll(&mut self) -> Result<State, Error> {
        let now = self.now();
        std::future::poll_fn(|cx| std::task::Poll::Ready(self.transport.poll(cx, now))).await
    }
    async fn callback(&mut self, req: CallbackRequest) -> Result<(), Error> {
        let result = self.apply(&req).await;
        let failed = result.is_err();
        req.finish(result);
        if failed {
            Err(Error::StaleContext)
        } else {
            Ok(())
        }
    }
    async fn drain_notifications(&mut self, req: &CallbackRequest) -> Result<(), Error> {
        // Receiving the last packet can precede its backend post-send fence.
        // Retain the one existing callback until that actual notification has
        // drained; do not queue or dispatch another Core request concurrently.
        tokio::time::timeout_at(
            req.deadline(),
            std::future::poll_fn(|cx| {
                let now = self.now();
                match self.transport.poll(cx, now) {
                    Ok(State::Connected) if !self.transport.has_pending_notifications() => {
                        std::task::Poll::Ready(Ok(()))
                    }
                    Ok(State::Connected) => std::task::Poll::Pending,
                    Ok(_) => std::task::Poll::Ready(Err(Error::StaleContext)),
                    Err(error) => std::task::Poll::Ready(Err(error)),
                }
            }),
        )
        .await
        .map_err(|_| Error::Deadline)??;
        req.check_current().await
    }
    async fn apply(&mut self, req: &CallbackRequest) -> Result<CallbackReply, Error> {
        req.check_current().await?;
        if req.epoch() != self.epoch {
            return Err(Error::StaleContext);
        }
        let reply = match req.operation() {
            CallbackOperation::BindPeer => {
                if self.connection.is_some() || self.watch.0.is_some() {
                    return Err(Error::Busy);
                }
                let peer = self
                    .host
                    .bind_peer(self.epoch, req.peer().device_path().to_owned())
                    .await?;
                if &peer != req.peer() {
                    return Err(Error::StaleContext);
                }
                self.connection = Some(self.transport.connected(self.epoch, peer, self.now())?);
                // Install before the post-work RPC, which may be slow. This
                // task uses original signal queues and can only deny/close.
                let loss = self.host.wait_peer_lost(self.epoch);
                let fence = self.fence.clone();
                let lost = self.lost.clone();
                self.watch.0 = Some(tokio::spawn(async move {
                    let _ = loss.await;
                    fence.close();
                    let _ = lost.try_send(());
                }));
                CallbackReply::Accepted
            }
            CallbackOperation::Write { frame, budget } => {
                let connection = self.connection.clone().ok_or(Error::Unavailable)?;
                if connection.peer() != req.peer() {
                    return Err(Error::StaleContext);
                }
                self.drain_notifications(req).await?;
                self.transport
                    .negotiated_mtu(&connection, (budget.attribute_bytes() + 3) as u16)?;
                self.transport.write(
                    &connection,
                    req.characteristic(),
                    WriteOptions::REQUEST,
                    self.now(),
                    frame.as_slice(),
                )?;
                CallbackReply::Accepted
            }
            CallbackOperation::ReadResult { .. } => {
                let connection = self.connection.as_ref().ok_or(Error::Unavailable)?;
                if connection.peer() != req.peer() {
                    return Err(Error::StaleContext);
                }
                CallbackReply::Value(self.transport.read_result(connection, 0)?)
            }
        };
        req.check_current().await?;
        Ok(reply)
    }
    async fn step(&mut self) -> Result<(), Error> {
        let state = self.poll().await?;
        if matches!(state, State::Unknown | State::Disabled | State::Stopping) {
            return Err(self.transport.last_error().unwrap_or(Error::Unknown));
        }
        if self.callbacks.is_none() {
            match self.host.take_callbacks(self.epoch) {
                Ok(callbacks) => self.callbacks = Some(callbacks),
                Err(Error::Unavailable | Error::StaleContext) if state == State::Registering => {}
                Err(e) => return Err(e),
            }
        }
        if self.release.is_none() {
            match self.host.take_release(self.epoch) {
                Ok(release) => self.release = Some(release),
                Err(Error::Unavailable | Error::StaleContext | Error::Busy)
                    if state == State::Registering => {}
                Err(e) => return Err(e),
            }
        }
        tokio::select! {
            biased;
            event = async {
                match self.release.as_mut() {
                    Some(r) => r.await.map_err(|_| Error::Unavailable),
                    None => std::future::pending().await,
                }
            } => {
                let event = event?;
                self.transport.advertisement_released(event.epoch(), event.owner(), self.now())?;
                Err(Error::Unavailable)
            },
            request = async {
                match self.callbacks.as_mut() {
                    Some(r) => r.recv().await.ok_or(Error::Unavailable),
                    None => std::future::pending().await,
                }
            }, if matches!(state, State::Advertising | State::Connected) => self.callback(request?).await,
            _ = tokio::time::sleep(Duration::from_millis(10)) => Ok(()),
        }
    }
    async fn stop(&mut self) -> Result<(), Error> {
        self.fence.close();
        let now = self.now();
        let _ = self.transport.stop(now);
        // Original backend futures remain owned and polled for compensation.
        // Never label a forced close/expiry as successful remote cleanup.
        let drain = async {
            loop {
                let _ = self.poll().await;
                if self.transport.is_quiescent() {
                    return Ok::<(), Error>(());
                }
                if !self.transport.has_pending_work() {
                    return Err(Error::Unknown);
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        let drained = tokio::time::timeout(Duration::from_secs(20), drain).await;
        let closed =
            tokio::time::timeout(Duration::from_secs(4), self.host.close_connection()).await;
        match (drained, closed) {
            (Ok(Ok(())), Ok(Ok(()))) => Ok(()),
            _ => Err(Error::Unknown),
        }
    }
}
/// Owns a single actual BlueZ/Core lifecycle until shutdown, owner/device loss
/// or error. Caller supplies the approved Core endpoint and actual bus binding;
/// no fake crypto/provider is accepted by this production entry point.
pub async fn run(
    control: BluezControl,
    secure: UnixSecurePort,
    domain: [u8; 16],
    shutdown: impl Future<Output = ()>,
) -> Result<(), Error> {
    let owner = control.owner().to_owned();
    let (backend, mut host) = BluezBackend::new_with_peer_requests(control);
    let mut owner_loss = host.take_owner_loss()?;
    let fence = secure.fence();
    let mut transport = Transport::new(backend, Some(secure), &owner, domain)?;
    let clock = Instant::now();
    let epoch = transport.start(0)?;
    let (lost, mut loss) = mpsc::channel(1);
    let mut d = Driver {
        transport,
        host,
        fence,
        epoch,
        clock,
        connection: None,
        callbacks: None,
        release: None,
        lost,
        watch: Watch(None),
    };
    tokio::pin!(shutdown);
    let result = loop {
        tokio::select! {
            biased;
            _ = &mut shutdown => break Ok(()),
            _ = &mut owner_loss => { d.fence.close(); let _ = d.transport.owner_lost(&owner, d.now()); break Err(Error::StaleContext); },
            _ = loss.recv() => break Err(Error::StaleContext),
            step = d.step() => if let Err(e) = step { break Err(e); },
        }
    };
    let cleanup = d.stop().await;
    result.and(cleanup)
}

#[cfg(test)]
mod tests;
