// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use rustsimhub_media::protocol::Credentials;
use rustsimhub_wire::{AuthCommand, AuthRequest, AUTH_PROTOCOL, MAX_FRAME};
use serde::{de::DeserializeOwned, Deserialize};
use std::{
    os::unix::{
        ffi::OsStrExt,
        fs::{FileTypeExt, MetadataExt},
    },
    path::{Component, Path},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    time::timeout,
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply<T> {
    protocol: u16,
    id: String,
    result: T,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Checked {
    media_lease_valid: bool,
    binding: String,
}
pub(super) struct Core {
    path: PathBuf,
    uid: u32,
    session: String,
    id: u64,
}
impl Core {
    pub fn new(path: PathBuf, uid: u32, session: String) -> Result<Self> {
        // Installed construction always selects Core986. Same-UID parser
        // fixtures may also run under Root; that exception cannot be selected
        // by the non-test host or any request/configuration payload.
        if uid == 0 && !cfg!(test)
            || !path.is_absolute()
            || path.as_os_str().as_bytes().len() >= 108
            || path.components().any(|c| matches!(c, Component::ParentDir))
        {
            return Err(Error::Denied);
        }
        Ok(Self {
            path,
            uid,
            session,
            id: 0,
        })
    }
    pub async fn check(&mut self, token: &str, lease: &CallLease) -> Result<String> {
        let reply: Checked = self
            .request(AuthCommand::CheckMediaLease {
                token: token.into(),
                lease: lease.clone(),
            })
            .await?;
        if !reply.media_lease_valid
            || reply.binding.len() != 64
            || !reply
                .binding
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(Error::Denied);
        }
        Ok(reply.binding)
    }
    pub async fn issue(
        &mut self,
        token: &str,
        lease: &CallLease,
        peer_key: [u8; 32],
    ) -> Result<Credentials> {
        let reply: Credentials = self
            .request(AuthCommand::IssueMedia {
                token: token.into(),
                lease: lease.clone(),
                peer_key,
            })
            .await?;
        if reply.peer_key != peer_key || reply.boot != lease.boot || reply.ticket == [0; 32] {
            return Err(Error::Denied);
        }
        Ok(reply)
    }
    async fn request<T: DeserializeOwned>(&mut self, command: AuthCommand) -> Result<T> {
        self.id = self.id.checked_add(1).ok_or(Error::Expired)?;
        let id = format!("{}:{}", self.session, self.id);
        let bytes = Zeroizing::new(
            serde_json::to_vec(&AuthRequest {
                protocol: AUTH_PROTOCOL,
                id: id.clone(),
                command,
            })
            .map_err(|_| Error::Denied)?,
        );
        rustsimhub_wire::parse_auth_request(&bytes).map_err(|_| Error::Denied)?;
        timeout(Duration::from_millis(1000), async {
            let before = metadata(&self.path, self.uid)?;
            let mut stream = UnixStream::connect(&self.path)
                .await
                .map_err(|_| Error::Denied)?;
            let peer = stream.peer_cred().map_err(|_| Error::Denied)?;
            if peer.uid() != self.uid
                || peer.pid().is_none_or(|p| p <= 0)
                || before != metadata(&self.path, self.uid)?
            {
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
            let mut response = Zeroizing::new(vec![0; length]);
            stream
                .read_exact(&mut response)
                .await
                .map_err(|_| Error::Denied)?;
            let shape: serde_json::Value =
                serde_json::from_slice(&response).map_err(|_| Error::Denied)?;
            if !shape.is_object() || !shape.get("result").is_some_and(|v| v.is_object()) {
                return Err(Error::Denied);
            }
            let reply: Reply<T> = serde_json::from_slice(&response).map_err(|_| Error::Denied)?;
            if reply.protocol != AUTH_PROTOCOL || reply.id != id {
                return Err(Error::Denied);
            }
            Ok(reply.result)
        })
        .await
        .map_err(|_| Error::Expired)?
    }
}
fn metadata(path: &Path, uid: u32) -> Result<(u64, u64)> {
    let m = std::fs::symlink_metadata(path).map_err(|_| Error::Denied)?;
    let parent = std::fs::symlink_metadata(path.parent().ok_or(Error::Denied)?)
        .map_err(|_| Error::Denied)?;
    if !m.file_type().is_socket()
        || m.uid() != uid
        || m.nlink() != 1
        || m.mode() & 0o7777 != 0o660
        || !parent.is_dir()
        || parent.uid() != uid
        || parent.mode() & 0o022 != 0
    {
        return Err(Error::Denied);
    }
    Ok((m.dev(), m.ino()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::UnixListener;
    fn lease() -> CallLease {
        CallLease {
            call_id: "11111111-1111-4111-8111-111111111111".into(),
            boot: "a".repeat(64),
            line: rustsimhub_domain::LineEpoch(1),
            generation: 1,
            expires_in_seconds: 30,
        }
    }
    #[tokio::test]
    async fn actual_core_socket_reply_version_id_objects_duplicates_and_positive_shape_are_checked()
    {
        for case in 0..8 {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("core.sock");
            let listener = UnixListener::bind(&path).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o660)).unwrap();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let n = stream.read_u32().await.unwrap();
                let mut body = vec![0; n as usize];
                stream.read_exact(&mut body).await.unwrap();
                let request = rustsimhub_wire::parse_auth_request(&body).unwrap();
                assert!(matches!(
                    request.command,
                    AuthCommand::CheckMediaLease { .. }
                ));
                let mut reply = serde_json::json!({"protocol":2,"id":request.id,"result":{"media_lease_valid":true,"binding":"a".repeat(64)}});
                match case {
                    0 => {}
                    1 => reply["protocol"] = 1.into(),
                    2 => reply["id"] = "other".into(),
                    3 => reply["result"] = serde_json::json!([true, "a".repeat(64)]),
                    4 => reply["result"]["media_lease_valid"] = false.into(),
                    5 => reply["result"]["binding"] = "bad".into(),
                    6 | 7 => {}
                    _ => unreachable!(),
                }
                let mut response = serde_json::to_vec(&reply).unwrap();
                if case == 6 {
                    response = String::from_utf8(response)
                        .unwrap()
                        .replacen("\"protocol\":2", "\"protocol\":2,\"protocol\":2", 1)
                        .into_bytes();
                }
                if case == 7 {
                    response = String::from_utf8(response)
                        .unwrap()
                        .replacen(
                            "\"media_lease_valid\":true",
                            "\"media_lease_valid\":true,\"media_lease_valid\":true",
                            1,
                        )
                        .into_bytes();
                }
                stream.write_u32(response.len() as u32).await.unwrap();
                stream.write_all(&response).await.unwrap();
            });
            let mut core =
                Core::new(path, rustix::process::geteuid().as_raw(), "b".repeat(32)).unwrap();
            assert_eq!(
                core.check(&"a".repeat(64), &lease()).await.is_ok(),
                case == 0,
                "case {case}"
            );
            server.await.unwrap();
        }
    }
    #[tokio::test]
    async fn actual_normal_core_wrong_uid_or_writable_parent_sends_no_request() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("core.sock");
        let listener = UnixListener::bind(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o660)).unwrap();
        let uid = rustix::process::geteuid().as_raw();
        let mut wrong = Core::new(path.clone(), uid + 1, "b".repeat(32)).unwrap();
        assert!(wrong.check(&"a".repeat(64), &lease()).await.is_err());
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o770)).unwrap();
        let mut wrong = Core::new(path, uid, "b".repeat(32)).unwrap();
        assert!(wrong.check(&"a".repeat(64), &lease()).await.is_err());
        assert!(timeout(Duration::from_millis(25), listener.accept())
            .await
            .is_err());
    }
}
