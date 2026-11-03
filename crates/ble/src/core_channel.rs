// SPDX-License-Identifier: AGPL-3.0-only
//! Actual bounded opaque relay to the same Core. This low privilege process
//! never loads a private identity, parses application JSON or grants authority.
//! One fresh port per accepted BLE link; no retries or implicit reconnect.
use crate::{
    framing::{InputKind, OpaqueBytes},
    ports::{Epoch, Peer, Pending, SecurePortV1, SecureReply},
    Error,
};
use std::{
    net::Shutdown,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::Mutex as AsyncMutex,
};

pub const CORE_SOCKET: &str = "/run/rustsimhub/recovery/core.sock";
struct Binding {
    epoch: Epoch,
    peer: Peer,
    phase: u8,
}
struct State {
    stream: Arc<AsyncMutex<UnixStream>>,
    fence: std::os::unix::net::UnixStream,
    live: AtomicBool,
    binding: Mutex<Option<Binding>>,
}
impl State {
    fn close(&self) {
        self.live.store(false, Ordering::Release);
        // A duplicate of this exact connected descriptor; immediate EOF fences
        // the real Core even while an owned exchange is awaiting completion.
        let _ = self.fence.shutdown(Shutdown::Both);
    }
}
pub struct CoreRecoveryPort {
    state: Arc<State>,
}
struct DeferredState {
    uid: u32,
    port: Arc<AsyncMutex<Option<CoreRecoveryPort>>>,
    source: Mutex<Option<Arc<State>>>,
    live: AtomicBool,
}
impl DeferredState {
    fn close(&self) {
        self.live.store(false, Ordering::Release);
        if let Ok(source) = self.source.lock() {
            if let Some(s) = source.as_ref() {
                s.close();
            }
        }
    }
}
/// No Core connection or handshake deadline is consumed while advertising or
/// waiting for a browser gesture. Connect once, on the first actual Auth frame.
/// Disconnect/cancellation permanently closes this epoch, including queued work.
pub struct OnDemandCorePort {
    state: Arc<DeferredState>,
}
impl OnDemandCorePort {
    pub fn new(core_uid: u32) -> Result<Self, Error> {
        if core_uid == 0 || core_uid == u32::MAX {
            return Err(Error::InvalidInput);
        }
        Ok(Self {
            state: Arc::new(DeferredState {
                uid: core_uid,
                port: Arc::new(AsyncMutex::new(None)),
                source: Mutex::new(None),
                live: AtomicBool::new(true),
            }),
        })
    }
}
impl Drop for OnDemandCorePort {
    fn drop(&mut self) {
        self.state.close();
    }
}
struct DeferredFence(Arc<DeferredState>, bool);
impl Drop for DeferredFence {
    fn drop(&mut self) {
        if self.1 {
            self.0.close()
        }
    }
}
impl SecurePortV1 for OnDemandCorePort {
    fn receive(
        &mut self,
        epoch: Epoch,
        peer: Peer,
        kind: InputKind,
        bytes: OpaqueBytes,
    ) -> Pending<SecureReply> {
        let state = self.state.clone();
        let reserved = state.port.clone().try_lock_owned();
        let mut cancel = reserved
            .as_ref()
            .ok()
            .map(|_| DeferredFence(state.clone(), true));
        Box::pin(async move {
            let mut owned = reserved.map_err(|_| Error::Busy)?;
            if !state.live.load(Ordering::Acquire) {
                return Err(Error::StaleContext);
            }
            if owned.is_none() {
                if kind != InputKind::Auth {
                    return Err(Error::Protocol);
                }
                let port = CoreRecoveryPort::connect(state.uid).await?;
                *state.source.lock().map_err(|_| Error::Unknown)? = Some(port.state.clone());
                if !state.live.load(Ordering::Acquire) {
                    return Err(Error::StaleContext);
                }
                *owned = Some(port);
            }
            let result = owned
                .as_mut()
                .ok_or(Error::Unavailable)?
                .receive(epoch, peer, kind, bytes)
                .await;
            if !state.live.load(Ordering::Acquire) {
                return Err(Error::StaleContext);
            }
            if result.is_ok() {
                if let Some(c) = cancel.as_mut() {
                    c.1 = false;
                }
            }
            result
        })
    }
    fn disconnected(&mut self, _: Epoch, _: Peer) -> Pending<()> {
        self.state.close();
        Box::pin(async { Ok(()) })
    }
}
struct PendingFence(Arc<State>, bool);
impl Drop for PendingFence {
    fn drop(&mut self) {
        if self.1 {
            self.0.close();
        }
    }
}
impl CoreRecoveryPort {
    pub async fn connect(core_uid: u32) -> Result<Self, Error> {
        if core_uid == 0 || core_uid == u32::MAX {
            return Err(Error::InvalidInput);
        }
        let stream = tokio::time::timeout(Duration::from_secs(5), UnixStream::connect(CORE_SOCKET))
            .await
            .map_err(|_| Error::Deadline)?
            .map_err(|_| Error::Unavailable)?;
        Self::attach(stream, core_uid)
    }
    /// Only an actually owned connected Unix descriptor, never a caller path,
    /// PID, authorization flag or business capability. Kernel peer UID checked.
    pub fn attach(stream: UnixStream, core_uid: u32) -> Result<Self, Error> {
        if core_uid == 0
            || core_uid == u32::MAX
            || stream.peer_cred().map_err(|_| Error::Unavailable)?.uid() != core_uid
        {
            return Err(Error::Unavailable);
        }
        let stream = stream.into_std().map_err(|_| Error::Backend)?;
        let fence = stream.try_clone().map_err(|_| Error::Backend)?;
        let stream = UnixStream::from_std(stream).map_err(|_| Error::Backend)?;
        Ok(Self {
            state: Arc::new(State {
                stream: Arc::new(AsyncMutex::new(stream)),
                fence,
                live: AtomicBool::new(true),
                binding: Mutex::new(None),
            }),
        })
    }
}
impl Drop for CoreRecoveryPort {
    fn drop(&mut self) {
        self.state.close();
    }
}
impl SecurePortV1 for CoreRecoveryPort {
    fn receive(
        &mut self,
        epoch: Epoch,
        peer: Peer,
        kind: InputKind,
        bytes: OpaqueBytes,
    ) -> Pending<SecureReply> {
        let state = self.state.clone();
        // Reserve immediately, including while the returned future is unpolled.
        let stream = state.stream.clone().try_lock_owned();
        let cancellation = stream
            .as_ref()
            .ok()
            .map(|_| PendingFence(state.clone(), true));
        Box::pin(async move {
            let mut cancellation = cancellation;
            let mut stream = stream.map_err(|_| Error::Busy)?;
            if !state.live.load(Ordering::Acquire) {
                return Err(Error::StaleContext);
            }
            let expected_kind = {
                let mut binding = state.binding.lock().map_err(|_| Error::Backend)?;
                if binding.is_none() {
                    *binding = Some(Binding {
                        epoch,
                        peer: peer.clone(),
                        phase: 0,
                    });
                }
                let b = binding.as_ref().ok_or(Error::Backend)?;
                if b.epoch != epoch || b.peer != peer {
                    return Err(Error::StaleContext);
                }
                if b.phase < 3 {
                    InputKind::Auth
                } else {
                    InputKind::Command
                }
            };
            if kind != expected_kind {
                state.close();
                return Err(Error::Protocol);
            }
            // Dropping an uncompleted future closes this exact descriptor.
            let exchange = async {
                stream
                    .write_u32(
                        u32::try_from(bytes.as_slice().len() + 2).map_err(|_| Error::Protocol)?,
                    )
                    .await
                    .map_err(|_| Error::Backend)?;
                stream
                    .write_all(&[1, kind as u8])
                    .await
                    .map_err(|_| Error::Backend)?;
                stream
                    .write_all(bytes.as_slice())
                    .await
                    .map_err(|_| Error::Backend)?;
                let len = stream.read_u32().await.map_err(|_| Error::Backend)?;
                if !(3..=2050).contains(&len) {
                    return Err(Error::Protocol);
                }
                let mut wire = vec![0; len as usize];
                stream
                    .read_exact(&mut wire)
                    .await
                    .map_err(|_| Error::Backend)?;
                if wire[..2] != [1, kind as u8] {
                    return Err(Error::Protocol);
                }
                if !state.live.load(Ordering::Acquire) {
                    return Err(Error::StaleContext);
                }
                let payload = OpaqueBytes::new(wire.split_off(2))?;
                Ok(if kind == InputKind::Auth {
                    SecureReply::Handshake(payload)
                } else {
                    SecureReply::Application(payload)
                })
            };
            let result = tokio::time::timeout(Duration::from_secs(5), exchange)
                .await
                .map_err(|_| Error::Deadline)?;
            match result {
                Ok(reply) => {
                    let mut binding = state.binding.lock().map_err(|_| Error::Backend)?;
                    let b = binding.as_mut().ok_or(Error::StaleContext)?;
                    if b.epoch != epoch || b.peer != peer || !state.live.load(Ordering::Acquire) {
                        return Err(Error::StaleContext);
                    }
                    b.phase = b.phase.saturating_add(1).min(3);
                    if let Some(fence) = cancellation.as_mut() {
                        fence.1 = false;
                    }
                    Ok(reply)
                }
                Err(error) => Err(error),
            }
        })
    }
    fn disconnected(&mut self, epoch: Epoch, peer: Peer) -> Pending<()> {
        let state = self.state.clone();
        let matches = state
            .binding
            .lock()
            .map(|b| {
                b.as_ref()
                    .is_none_or(|b| b.epoch == epoch && b.peer == peer)
            })
            .unwrap_or(false);
        if matches {
            state.close();
        }
        Box::pin(async move {
            if matches {
                Ok(())
            } else {
                Err(Error::StaleContext)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> (Epoch, Peer) {
        (
            Epoch::new([1; 16], 1).unwrap(),
            Peer::from_backend_callback(":1.2", "/org/bluez/hci0/dev_00_11_22_33_44_55").unwrap(),
        )
    }
    fn pair() -> (CoreRecoveryPort, UnixStream) {
        let (client, server) = UnixStream::pair().unwrap();
        let uid = client.peer_cred().unwrap().uid();
        (CoreRecoveryPort::attach(client, uid).unwrap(), server)
    }
    #[tokio::test]
    async fn actual_descriptor_relay_has_three_handshake_steps_then_encrypted_application() {
        let (mut port, mut server) = pair();
        let (epoch, peer) = context();
        for kind in [
            InputKind::Auth,
            InputKind::Auth,
            InputKind::Auth,
            InputKind::Command,
        ] {
            let exchange = port.receive(
                epoch,
                peer.clone(),
                kind,
                OpaqueBytes::new(vec![9; 32]).unwrap(),
            );
            let remote = async {
                assert_eq!(server.read_u32().await.unwrap(), 34);
                let mut body = [0; 34];
                server.read_exact(&mut body).await.unwrap();
                assert_eq!(body[..2], [1, kind as u8]);
                server.write_u32(3).await.unwrap();
                server.write_all(&[1, kind as u8, 5]).await.unwrap();
            };
            let (result, ()) = tokio::join!(exchange, remote);
            match result.unwrap() {
                SecureReply::Handshake(b) | SecureReply::Application(b) => {
                    assert_eq!(b.as_slice(), &[5])
                }
                _ => panic!("missing opaque reply"),
            }
        }
        port.disconnected(epoch, peer).await.unwrap();
        let mut byte = [0];
        assert_eq!(server.read(&mut byte).await.unwrap(), 0);
    }
    #[tokio::test]
    async fn cancellation_shuts_down_real_descriptor_and_cannot_rebind() {
        for entered in [false, true] {
            let (mut port, mut server) = pair();
            let (epoch, peer) = context();
            let exchange = port.receive(
                epoch,
                peer.clone(),
                InputKind::Auth,
                OpaqueBytes::new(vec![9]).unwrap(),
            );
            let mut exchange = Box::pin(exchange);
            if entered {
                let mut frame = [0; 7];
                tokio::select! {result=&mut exchange=>panic!("reply completed before Core sent any: {}",result.is_ok()),result=server.read_exact(&mut frame)=>{result.unwrap();}}
                assert_eq!(frame, [0, 0, 0, 3, 1, 1, 9]);
            }
            drop(exchange);
            let mut bytes = vec![];
            server.read_to_end(&mut bytes).await.unwrap();
            assert!(bytes.is_empty());
            assert!(port
                .receive(
                    epoch,
                    peer,
                    InputKind::Auth,
                    OpaqueBytes::new(vec![9]).unwrap()
                )
                .await
                .is_err());
        }
    }
    #[tokio::test]
    async fn wrong_kernel_uid_and_early_business_are_denied() {
        let (client, _) = UnixStream::pair().unwrap();
        let uid = client.peer_cred().unwrap().uid();
        assert!(CoreRecoveryPort::attach(client, uid + 1).is_err());
        let (mut port, mut server) = pair();
        let (epoch, peer) = context();
        assert!(port
            .receive(
                epoch,
                peer,
                InputKind::Command,
                OpaqueBytes::new(vec![9]).unwrap()
            )
            .await
            .is_err());
        let mut byte = [0];
        assert_eq!(server.read(&mut byte).await.unwrap(), 0);
    }
}

#[cfg(test)]
mod deferred_tests {
    use super::*;
    fn context() -> (Epoch, Peer) {
        (
            Epoch::new([31; 16], 1).unwrap(),
            Peer::from_backend_callback(":1.2", "/org/bluez/hci0/dev_00_11_22_33_44_55").unwrap(),
        )
    }
    #[tokio::test]
    async fn deferred_constructor_and_early_business_never_connect_or_reconnect() {
        assert!(OnDemandCorePort::new(0).is_err());
        let mut port = OnDemandCorePort::new(1001).unwrap();
        assert!(port.state.source.lock().unwrap().is_none());
        let (e, p) = context();
        assert!(port
            .receive(e, p, InputKind::Command, OpaqueBytes::new(vec![1]).unwrap())
            .await
            .is_err());
        assert!(port.state.source.lock().unwrap().is_none());
        assert!(!port.state.live.load(Ordering::Acquire));
    }
    #[test]
    fn dropping_unpolled_first_auth_permanently_cancels_original_deferred_epoch() {
        let mut port = OnDemandCorePort::new(1001).unwrap();
        let (e, p) = context();
        drop(port.receive(e, p, InputKind::Auth, OpaqueBytes::new(vec![1]).unwrap()));
        assert!(!port.state.live.load(Ordering::Acquire));
        assert!(port.state.source.lock().unwrap().is_none());
    }
}
