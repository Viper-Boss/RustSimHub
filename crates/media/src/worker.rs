// SPDX-License-Identifier: AGPL-3.0-only
//! Local signaling/worker control. This protocol cannot create or control a call.
use crate::{protocol::Credentials, Error};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

pub const PROTOCOL: u16 = 1;
pub const MAX_FRAME: usize = 24_576;
pub const MAX_SDP: usize = 16_384;

// No Debug: requests may contain ICE passwords or single-use Core credentials.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub protocol: u16,
    pub id: u64,
    pub command: Command,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Prepare {
        session: String,
        peer_ip: IpAddr,
        offer: String,
    },
    Inspect {
        session: String,
    },
    Authorize {
        session: String,
        credentials: Credentials,
    },
    Stop {
        session: String,
    },
}
impl Command {
    pub fn session(&self) -> &str {
        match self {
            Self::Prepare { session, .. }
            | Self::Inspect { session }
            | Self::Authorize { session, .. }
            | Self::Stop { session } => session,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Handshaking,
    Verified,
    Authorizing,
    Active,
    Closing,
    Closed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    BadRequest,
    Busy,
    Denied,
    Unavailable,
    Closed,
    Timeout,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub session: String,
    pub phase: Phase,
    pub peer_key: Option<[u8; 32]>,
    pub answer: Option<String>,
    pub pcm_opened: bool,
    pub failure: Option<Failure>,
    pub counters: Counters,
}
#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counters {
    pub sent: u64,
    pub received: u64,
    pub captured: u64,
    pub played: u64,
    pub dropped: u64,
    pub xruns: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub protocol: u16,
    pub id: u64,
    pub result: std::result::Result<Snapshot, Failure>,
}
pub fn local_ip(ip: IpAddr) -> bool {
    matches!(ip, IpAddr::V4(v) if v.is_loopback() || v.is_private() || v.is_link_local())
}
pub fn session_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn parse(bytes: &[u8]) -> Result<Request, Error> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(Error::Denied);
    }
    let request: Request = serde_json::from_slice(bytes).map_err(|_| Error::Denied)?;
    if request.protocol != PROTOCOL || request.id == 0 || !session_id(request.command.session()) {
        return Err(Error::Denied);
    }
    match &request.command {
        Command::Prepare { peer_ip, offer, .. }
            if !local_ip(*peer_ip) || offer.is_empty() || offer.len() > MAX_SDP =>
        {
            return Err(Error::Denied)
        }
        Command::Authorize { credentials, .. }
            if credentials.boot.len() != 64
                || !credentials
                    .boot
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) =>
        {
            return Err(Error::Denied)
        }
        _ => {}
    }
    Ok(request)
}

#[cfg(target_os = "linux")]
pub async fn exchange(
    path: &std::path::Path,
    server_uid: u32,
    request: Request,
) -> Result<Reply, Error> {
    use std::{
        os::unix::fs::{FileTypeExt, MetadataExt},
        time::Duration,
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::UnixStream,
        time::timeout,
    };
    let metadata = std::fs::symlink_metadata(path).map_err(|_| Error::Denied)?;
    let parent = std::fs::symlink_metadata(path.parent().ok_or(Error::Denied)?)
        .map_err(|_| Error::Denied)?;
    if !metadata.file_type().is_socket()
        || metadata.uid() != server_uid
        || !parent.is_dir()
        || parent.uid() != server_uid
        || parent.mode() & 0o022 != 0
    {
        return Err(Error::Denied);
    }
    let id = request.id;
    let session = request.command.session().to_owned();
    let bytes = serde_json::to_vec(&request).map_err(|_| Error::Denied)?;
    parse(&bytes)?;
    timeout(Duration::from_secs(1), async {
        let mut stream = UnixStream::connect(path).await.map_err(|_| Error::Denied)?;
        if stream.peer_cred().map(|c| c.uid()).ok() != Some(server_uid) {
            return Err(Error::Denied);
        }
        stream
            .write_u32(bytes.len() as u32)
            .await
            .map_err(|_| Error::Denied)?;
        stream.write_all(&bytes).await.map_err(|_| Error::Denied)?;
        let length = stream.read_u32().await.map_err(|_| Error::Denied)? as usize;
        if length == 0 || length > MAX_FRAME {
            return Err(Error::Denied);
        }
        let mut bytes = vec![0; length];
        stream
            .read_exact(&mut bytes)
            .await
            .map_err(|_| Error::Denied)?;
        let reply: Reply = serde_json::from_slice(&bytes).map_err(|_| Error::Denied)?;
        if reply.id != id
            || reply.protocol != PROTOCOL
            || reply.result.as_ref().is_ok_and(|s| {
                s.session != session || s.answer.as_ref().is_some_and(|a| a.len() > MAX_SDP)
            })
        {
            return Err(Error::Denied);
        }
        Ok(reply)
    })
    .await
    .map_err(|_| Error::Denied)?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worker_protocol_does_not_accept_calls_hardware_paths_or_unsafe_endpoints() {
        let session = "a".repeat(32);
        let valid = serde_json::json!({"protocol":1,"id":1,"command":{"op":"prepare","session":session,"peer_ip":"127.0.0.1","offer":"bounded"}});
        assert!(parse(&serde_json::to_vec(&valid).unwrap()).is_ok());
        for (field, value) in [
            ("op", "dial"),
            ("peer_ip", "8.8.8.8"),
            ("peer_ip", "0.0.0.0"),
            ("peer_ip", "::1"),
            ("session", "../path"),
            ("session", "A"),
            ("offer", ""),
        ] {
            let mut invalid = valid.clone();
            invalid["command"][field] = value.into();
            assert!(parse(&serde_json::to_vec(&invalid).unwrap()).is_err());
        }
        for field in [
            "pcm_device",
            "core_socket",
            "role",
            "uid",
            "recording",
            "token",
        ] {
            let mut invalid = valid.clone();
            invalid["command"][field] = "injected".into();
            assert!(parse(&serde_json::to_vec(&invalid).unwrap()).is_err());
        }
        let mut invalid = valid;
        invalid["command"]["offer"] = "s".repeat(MAX_SDP + 1).into();
        assert!(parse(&serde_json::to_vec(&invalid).unwrap()).is_err());
        assert!(parse(&vec![b' '; MAX_FRAME + 1]).is_err());
    }
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn worker_client_checks_actual_uid_and_reply_session_before_returning_signaling() {
        use std::os::unix::fs::PermissionsExt;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let d = tempfile::tempdir().unwrap();
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = d.path().join("worker.sock");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let request = || Request {
            protocol: PROTOCOL,
            id: 1,
            command: Command::Inspect {
                session: "a".repeat(32),
            },
        };
        assert!(exchange(&path, u32::MAX, request()).await.is_err());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let length = stream.read_u32().await.unwrap();
            let mut bytes = vec![0; length as usize];
            stream.read_exact(&mut bytes).await.unwrap();
            let reply = Reply {
                protocol: PROTOCOL,
                id: 1,
                result: Ok(Snapshot {
                    session: "b".repeat(32),
                    phase: Phase::Verified,
                    peer_key: Some([7; 32]),
                    answer: None,
                    pcm_opened: false,
                    failure: None,
                    counters: Counters::default(),
                }),
            };
            let bytes = serde_json::to_vec(&reply).unwrap();
            stream.write_u32(bytes.len() as u32).await.unwrap();
            stream.write_all(&bytes).await.unwrap();
        });
        assert!(
            exchange(&path, rustix::process::geteuid().as_raw(), request())
                .await
                .is_err()
        );
        server.await.unwrap();
    }
}
