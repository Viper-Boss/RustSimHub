// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::{
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    time::timeout,
};
struct Fence {
    live: AtomicBool,
    fd: rustix::fd::OwnedFd,
}
impl Fence {
    fn close(&self) {
        self.live.store(false, Ordering::Release);
        let _ = rustix::net::shutdown(&self.fd, rustix::net::Shutdown::Both);
    }
    fn check(&self) -> Result<(), Error> {
        if self.live.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err(Error::Denied)
        }
    }
}
struct InFlight {
    fence: Arc<Fence>,
    complete: bool,
}
/// Revoke this exact connection if an enclosing host operation is dropped or
/// fails, including while it is awaiting normal Core authorization. No resume,
/// reconnect or authority can be obtained from this guard.
pub struct CancellationGuard {
    fence: Arc<Fence>,
    armed: bool,
}
impl CancellationGuard {
    pub fn disarm(mut self) {
        self.armed = false;
    }
}
impl Drop for CancellationGuard {
    fn drop(&mut self) {
        if self.armed {
            self.fence.close();
        }
    }
}
impl Drop for InFlight {
    fn drop(&mut self) {
        if !self.complete {
            self.fence.close();
        }
    }
}
/// One actual accepted media-worker connection and one local session. No Clone,
/// automatic reconnect/replay or authority minted from a successful snapshot.
pub struct Client {
    stream: UnixStream,
    fence: Arc<Fence>,
    session: String,
    id: u64,
    started: bool,
}
impl Drop for Client {
    fn drop(&mut self) {
        self.fence.close();
    }
}
impl Client {
    pub fn cancellation_guard(&self) -> CancellationGuard {
        CancellationGuard {
            fence: self.fence.clone(),
            armed: true,
        }
    }
    pub async fn connect(path: &Path, server_uid: u32, session: String) -> Result<Self, Error> {
        use std::{os::unix::ffi::OsStrExt, path::Component};
        if server_uid == 0
            || !crate::worker::session_id(&session)
            || !path.is_absolute()
            || path.as_os_str().as_bytes().len() >= 108
            || path.components().any(|c| matches!(c, Component::ParentDir))
        {
            return Err(Error::Denied);
        }
        let metadata = std::fs::symlink_metadata(path).map_err(|_| Error::Denied)?;
        let parent = std::fs::symlink_metadata(path.parent().ok_or(Error::Denied)?)
            .map_err(|_| Error::Denied)?;
        if !metadata.file_type().is_socket()
            || metadata.uid() != server_uid
            || metadata.nlink() != 1
            || metadata.mode() & 0o7777 != 0o660
            || !parent.is_dir()
            || parent.uid() != server_uid
            || parent.mode() & 0o022 != 0
        {
            return Err(Error::Denied);
        }
        let stream = timeout(Duration::from_millis(500), UnixStream::connect(path))
            .await
            .map_err(|_| Error::Denied)?
            .map_err(|_| Error::Denied)?;
        let peer = stream.peer_cred().map_err(|_| Error::Denied)?;
        let after = std::fs::symlink_metadata(path).map_err(|_| Error::Denied)?;
        if peer.uid() != server_uid
            || peer.pid().is_none_or(|pid| pid <= 0)
            || (metadata.dev(), metadata.ino()) != (after.dev(), after.ino())
        {
            return Err(Error::Denied);
        }
        let fence = Arc::new(Fence {
            live: AtomicBool::new(true),
            fd: rustix::io::dup(&stream).map_err(|_| Error::Denied)?,
        });
        Ok(Self {
            stream,
            fence,
            session,
            id: 0,
            started: false,
        })
    }
    pub async fn exchange(&mut self, command: Command) -> Result<Reply, Error> {
        self.fence.check()?;
        if command.session() != self.session
            || !self.started && !matches!(command, Command::Prepare { .. })
            || self.started && matches!(command, Command::Prepare { .. })
        {
            return Err(Error::Denied);
        }
        let id = self.id.checked_add(1).ok_or(Error::Expired)?;
        let preparing = matches!(command, Command::Prepare { .. });
        let bytes = serde_json::to_vec(&Request {
            protocol: PROTOCOL,
            id,
            command,
        })
        .map_err(|_| Error::Denied)?;
        parse(&bytes)?;
        let mut guard = InFlight {
            fence: self.fence.clone(),
            complete: false,
        };
        self.id = id;
        let budget = Duration::from_millis(if preparing { PREPARE_MS } else { 1000 });
        let reply = timeout(budget, async {
            self.stream
                .write_u32(bytes.len() as u32)
                .await
                .map_err(|_| Error::Denied)?;
            self.stream
                .write_all(&bytes)
                .await
                .map_err(|_| Error::Denied)?;
            let n = self.stream.read_u32().await.map_err(|_| Error::Denied)? as usize;
            if n == 0 || n > MAX_FRAME {
                return Err(Error::Denied);
            }
            let mut bytes = vec![0; n];
            self.stream
                .read_exact(&mut bytes)
                .await
                .map_err(|_| Error::Denied)?;
            parse_reply(&bytes, id, &self.session)
        })
        .await
        .map_err(|_| Error::Denied)??;
        self.fence.check()?;
        self.started = true; // A failed Prepare also cannot be replayed on this connection.
        guard.complete = true;
        Ok(reply)
    }
}
