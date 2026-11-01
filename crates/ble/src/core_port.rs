// SPDX-License-Identifier: AGPL-3.0-only
//! Opaque fixed-protocol forwarding to the actual installed Core UID. No crypto
//! or business authorization lives in BLE, and a failed session never reconnects.
use crate::{
    framing::{InputKind, OpaqueBytes, MESSAGE_BYTES},
    ports::{Epoch, Peer, Pending, SecurePortV1, SecureReply},
    Error,
};
use std::{
    net::Shutdown,
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::net::UnixStream;
const IO: Duration = Duration::from_secs(5);

/// Trusted installed role/endpoint configuration, never deserialize it from a
/// GATT message. Constructing it does not prove that Core has an A backend.
#[derive(Clone)]
pub struct CoreEndpoint {
    path: PathBuf,
    core_uid: u32,
    ble_uid: u32,
    ble_gid: u32,
}
impl CoreEndpoint {
    #[cfg(test)]
    pub(crate) fn same_uid_fixture(path: PathBuf) -> Self {
        Self {
            path,
            core_uid: rustix::process::geteuid().as_raw(),
            ble_uid: rustix::process::geteuid().as_raw(),
            ble_gid: rustix::process::getegid().as_raw(),
        }
    }
    pub fn from_installed_roles(
        path: PathBuf,
        core_uid: u32,
        ble_uid: u32,
        ble_gid: u32,
    ) -> Result<Self, Error> {
        if !path.is_absolute()
            || path.as_os_str().len() > 107
            || path.components().any(|c| {
                matches!(
                    c,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            })
            || [core_uid, ble_uid, ble_gid]
                .iter()
                .any(|u| *u == 0 || *u == u32::MAX)
            || core_uid == ble_uid
        {
            return Err(Error::InvalidInput);
        }
        Ok(Self {
            path,
            core_uid,
            ble_uid,
            ble_gid,
        })
    }
    fn local_role(&self) -> Result<(), Error> {
        if rustix::process::geteuid().as_raw() != self.ble_uid
            || rustix::process::getegid().as_raw() != self.ble_gid
        {
            return Err(Error::Unavailable);
        }
        // Only the cfg(test) same-UID lifecycle fixture can reach this path.
        // Installed-role construction always rejects equal UIDs. Unit tests
        // exercise real I/O/cleanup on runners with arbitrary inherited groups;
        // the separate non-cfg(test) UID target proves production role denial.
        #[cfg(test)]
        if self.core_uid == self.ble_uid {
            return Ok(());
        }
        if rustix::process::getgroups()
            .map_err(|_| Error::Unavailable)?
            .iter()
            .any(|g| g.as_raw() != self.ble_gid)
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    fn metadata(&self) -> Result<(u64, u64), Error> {
        self.local_role()?;
        let dir = std::fs::symlink_metadata(self.path.parent().ok_or(Error::InvalidInput)?)
            .map_err(|_| Error::Unavailable)?;
        let socket = std::fs::symlink_metadata(&self.path).map_err(|_| Error::Unavailable)?;
        if !dir.is_dir()
            || dir.uid() != self.core_uid
            || dir.gid() != self.ble_gid
            || dir.mode() & 0o7777 != 0o2750
            || !socket.file_type().is_socket()
            || socket.uid() != self.core_uid
            || socket.gid() != self.ble_gid
            || socket.mode() & 0o7777 != 0o660
            || socket.nlink() != 1
        {
            return Err(Error::Unavailable);
        }
        Ok((socket.dev(), socket.ino()))
    }
    async fn open(&self) -> Result<Arc<UnixStream>, Error> {
        let before = self.metadata()?;
        let stream = tokio::time::timeout(IO, UnixStream::connect(&self.path))
            .await
            .map_err(|_| Error::Deadline)?
            .map_err(|_| Error::Unavailable)?;
        let peer = stream.peer_cred().map_err(|_| Error::Unavailable)?;
        if peer.uid() != self.core_uid
            || peer.pid().is_none_or(|p| p <= 0)
            || self.metadata()? != before
        {
            return Err(Error::Unavailable);
        }
        Ok(Arc::new(stream))
    }
    /// Probe actual metadata and kernel peer UID without sending any packet.
    /// Core must only publish this dedicated endpoint with its approved A port.
    pub async fn probe(&self) -> Result<(), Error> {
        let s = self.open().await?;
        let _ = socket2::SockRef::from(s.as_ref()).shutdown(Shutdown::Both);
        Ok(())
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}
struct Link {
    epoch: Epoch,
    peer: Peer,
    live: AtomicBool,
    stream: Mutex<Option<Arc<UnixStream>>>,
}
impl Link {
    fn check(&self) -> Result<(), Error> {
        if self.live.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err(Error::StaleContext)
        }
    }
    fn close(&self) {
        self.live.store(false, Ordering::Release);
        if let Ok(s) = self.stream.lock() {
            if let Some(s) = s.as_ref() {
                let _ = socket2::SockRef::from(s.as_ref()).shutdown(Shutdown::Both);
            }
        }
    }
}
impl Drop for Link {
    fn drop(&mut self) {
        self.close();
    }
}
struct State {
    link: Mutex<Option<Arc<Link>>>,
    busy: AtomicBool,
}
/// At most one epoch/peer and one I/O operation. Dead links remain dead until
/// an explicit matching disconnect; no retry/rekey/command replay is hidden.
pub struct UnixSecurePort {
    endpoint: CoreEndpoint,
    state: Arc<State>,
}
/// An owning runtime can fence I/O immediately on a BlueZ disconnect/stop,
/// before waiting for the transport's late operation cleanup. Denial only.
#[derive(Clone)]
pub struct UnixPortFence {
    state: Arc<State>,
}
impl UnixPortFence {
    pub fn close(&self) {
        if let Ok(s) = self.state.link.lock() {
            if let Some(s) = s.as_ref() {
                s.close();
            }
        }
    }
}
struct Io {
    state: Arc<State>,
    link: Arc<Link>,
    completed: bool,
}
impl Drop for Io {
    fn drop(&mut self) {
        if !self.completed {
            self.link.close();
        }
        self.state.busy.store(false, Ordering::Release);
    }
}
impl UnixSecurePort {
    pub fn fence(&self) -> UnixPortFence {
        UnixPortFence {
            state: self.state.clone(),
        }
    }
    pub async fn connect(endpoint: CoreEndpoint) -> Result<Self, Error> {
        endpoint.probe().await?;
        Ok(Self {
            endpoint,
            state: Arc::new(State {
                link: Mutex::new(None),
                busy: AtomicBool::new(false),
            }),
        })
    }
    fn close_all(&self) {
        if let Ok(s) = self.state.link.lock() {
            if let Some(s) = s.as_ref() {
                s.close();
            }
        }
    }
}
impl Drop for UnixSecurePort {
    fn drop(&mut self) {
        self.close_all();
    }
}
async fn read_exact(s: &UnixStream, mut data: &mut [u8]) -> Result<(), Error> {
    while !data.is_empty() {
        s.readable().await.map_err(|_| Error::Unavailable)?;
        match s.try_read(data) {
            Ok(0) => return Err(Error::Unavailable),
            Ok(n) => data = &mut data[n..],
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => return Err(Error::Unavailable),
        }
    }
    Ok(())
}
async fn write_all(s: &UnixStream, mut data: &[u8]) -> Result<(), Error> {
    while !data.is_empty() {
        s.writable().await.map_err(|_| Error::Unavailable)?;
        match s.try_write(data) {
            Ok(0) => return Err(Error::Unavailable),
            Ok(n) => data = &data[n..],
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => return Err(Error::Unavailable),
        }
    }
    Ok(())
}
impl SecurePortV1 for UnixSecurePort {
    fn receive(
        &mut self,
        epoch: Epoch,
        peer: Peer,
        kind: InputKind,
        bytes: OpaqueBytes,
    ) -> Pending<SecureReply> {
        let state = self.state.clone();
        let endpoint = self.endpoint.clone();
        Box::pin(async move {
            state
                .busy
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .map_err(|_| Error::Busy)?;
            // Release busy on every path, including a poisoned state lock.
            let link = match state.link.lock() {
                Ok(mut s) => match s.as_ref() {
                    Some(l) if l.epoch == epoch && l.peer == peer => l.clone(),
                    Some(_) => {
                        state.busy.store(false, Ordering::Release);
                        return Err(Error::StaleContext);
                    }
                    None => {
                        let l = Arc::new(Link {
                            epoch,
                            peer,
                            live: AtomicBool::new(true),
                            stream: Mutex::new(None),
                        });
                        *s = Some(l.clone());
                        l
                    }
                },
                Err(_) => {
                    state.busy.store(false, Ordering::Release);
                    return Err(Error::Unknown);
                }
            };
            let mut io = Io {
                state,
                link: link.clone(),
                completed: false,
            };
            let result = tokio::time::timeout(IO, async {
                link.check()?;
                let existing = link.stream.lock().map_err(|_| Error::Unknown)?.clone();
                let stream = match existing {
                    Some(s) => s,
                    None => {
                        let s = endpoint.open().await?;
                        link.check()?;
                        *link.stream.lock().map_err(|_| Error::Unknown)? = Some(s.clone());
                        s
                    }
                };
                link.check()?;
                let tag = match kind {
                    InputKind::Auth => 1u8,
                    InputKind::Command => 2,
                };
                let mut frame = Vec::with_capacity(bytes.as_slice().len() + 6);
                frame.extend_from_slice(&((bytes.as_slice().len() + 2) as u32).to_be_bytes());
                frame.extend_from_slice(&[1, tag]);
                frame.extend_from_slice(bytes.as_slice());
                write_all(&stream, &frame).await?;
                link.check()?;
                let mut header = [0; 4];
                read_exact(&stream, &mut header).await?;
                let n = u32::from_be_bytes(header) as usize;
                if !(3..=MESSAGE_BYTES + 2).contains(&n) {
                    return Err(Error::Protocol);
                }
                let mut prefix = [0; 2];
                read_exact(&stream, &mut prefix).await?;
                if prefix != [1, tag] {
                    return Err(Error::Protocol);
                }
                let mut data = vec![0; n - 2];
                read_exact(&stream, &mut data).await?;
                link.check()?;
                let data = OpaqueBytes::new(data)?;
                Ok(match kind {
                    InputKind::Auth => SecureReply::Handshake(data),
                    InputKind::Command => SecureReply::Application(data),
                })
            })
            .await
            .map_err(|_| Error::Deadline)?;
            if result.is_ok() {
                io.completed = true;
            }
            result
        })
    }
    fn disconnected(&mut self, epoch: Epoch, peer: Peer) -> Pending<()> {
        let state = self.state.clone();
        Box::pin(async move {
            let mut s = state.link.lock().map_err(|_| Error::Unknown)?;
            if s.as_ref()
                .is_some_and(|l| l.epoch != epoch || l.peer != peer)
            {
                return Err(Error::StaleContext);
            }
            if let Some(l) = s.take() {
                l.close();
            }
            Ok(())
        })
    }
}
