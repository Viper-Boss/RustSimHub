// SPDX-License-Identifier: AGPL-3.0-only
//! Local authorization client. No TCP listener or audio data crosses this protocol.
use crate::{
    protocol::{self, Command, Credentials, Reply, Request},
    Error, Permit, RemoteAuthority,
};
use std::{
    os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt},
    path::Path,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    time::timeout,
};
pub fn now_ms() -> u64 {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
    (t.tv_sec as u64)
        .saturating_mul(1000)
        .saturating_add(t.tv_nsec as u64 / 1_000_000)
}
pub fn read_credentials(path: &Path) -> Result<Credentials, Error> {
    use std::io::Read;
    let parent = std::fs::symlink_metadata(path.parent().ok_or(Error::Denied)?)
        .map_err(|_| Error::Denied)?;
    let uid = rustix::process::geteuid().as_raw();
    if !parent.is_dir() || parent.uid() != uid || parent.mode() & 0o077 != 0 {
        return Err(Error::Denied);
    }
    let f = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(path)
        .map_err(|_| Error::Denied)?;
    let m = f.metadata().map_err(|_| Error::Denied)?;
    if !m.is_file() || m.uid() != uid || m.mode() & 0o077 != 0 || m.nlink() != 1 || m.len() > 2048 {
        return Err(Error::Denied);
    }
    let mut bytes = Vec::new();
    f.take(2049)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Denied)?;
    serde_json::from_slice(&bytes).map_err(|_| Error::Denied)
}
pub struct Client {
    stream: UnixStream,
    authority: RemoteAuthority,
    id: u64,
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    #[test]
    fn private_ticket_refuses_symlink_hardlink_and_broad_permissions() {
        let d = tempfile::tempdir().unwrap();
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let p = d.path().join("ticket.json");
        let bytes = serde_json::to_vec(&Credentials {
            ticket: [8; 32],
            peer_key: [7; 32],
            boot: "a".repeat(64),
        })
        .unwrap();
        std::fs::write(&p, bytes).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(read_credentials(&p).is_ok());
        let alias = d.path().join("alias");
        symlink(&p, &alias).unwrap();
        assert!(read_credentials(&alias).is_err());
        let hard = d.path().join("hard");
        std::fs::hard_link(&p, &hard).unwrap();
        assert!(read_credentials(&p).is_err());
        std::fs::remove_file(hard).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_credentials(&p).is_err());
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o750)).unwrap();
        assert!(read_credentials(&p).is_err());
    }
}
impl Client {
    pub async fn redeem(
        socket: &Path,
        server_uid: u32,
        credentials: Credentials,
    ) -> Result<Self, Error> {
        let m = std::fs::symlink_metadata(socket).map_err(|_| Error::Denied)?;
        let parent = std::fs::symlink_metadata(socket.parent().ok_or(Error::Denied)?)
            .map_err(|_| Error::Denied)?;
        if !m.file_type().is_socket()
            || m.uid() != server_uid
            || !parent.is_dir()
            || parent.uid() != server_uid
            || parent.mode() & 0o022 != 0
        {
            return Err(Error::Denied);
        }
        let stream = timeout(Duration::from_millis(500), UnixStream::connect(socket))
            .await
            .map_err(|_| Error::Denied)?
            .map_err(|_| Error::Denied)?;
        if stream.peer_cred().map(|p| p.uid()).ok() != Some(server_uid) {
            return Err(Error::Denied);
        }
        let mut client = Self {
            stream,
            authority: RemoteAuthority::new(credentials.boot)?,
            id: 0,
        };
        client
            .exchange(Command::Redeem {
                ticket: credentials.ticket,
                peer_key: credentials.peer_key,
            })
            .await?;
        Ok(client)
    }
    pub fn permit(&self) -> Permit {
        self.authority.permit()
    }
    pub async fn pulse(&mut self) -> Result<(), Error> {
        self.exchange(Command::Pulse {}).await
    }
    async fn exchange(&mut self, command: Command) -> Result<(), Error> {
        self.id = self.id.checked_add(1).ok_or(Error::Expired)?;
        let id = self.id;
        let request = serde_json::to_vec(&Request {
            protocol: protocol::PROTOCOL,
            id,
            command,
        })
        .map_err(|_| Error::Denied)?;
        let response = timeout(Duration::from_millis(500), async {
            self.stream.write_u32(request.len() as u32).await?;
            self.stream.write_all(&request).await?;
            let length = self.stream.read_u32().await? as usize;
            if length == 0 || length > protocol::MAX_FRAME {
                return Err(std::io::Error::other("invalid media frame"));
            }
            let mut bytes = vec![0; length];
            self.stream.read_exact(&mut bytes).await?;
            Ok(bytes)
        })
        .await;
        let result = match response {
            Ok(Ok(bytes)) => match serde_json::from_slice::<Reply>(&bytes) {
                Ok(reply) if reply.protocol == protocol::PROTOCOL && reply.id == id => {
                    self.authority.update(&reply.boot, reply.until_ms, now_ms())
                }
                _ => Err(Error::Denied),
            },
            _ => Err(Error::Denied),
        };
        if result.is_err() {
            self.authority.revoke();
        }
        result
    }
}
